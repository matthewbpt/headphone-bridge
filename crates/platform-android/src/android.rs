//! The JNI glue: the process's one [`Host`], started by whichever asks first
//! (the Tauri setup or Kotlin's `NativeCore.start`), and the way back to
//! Kotlin. The app crate exports the JNI symbols that call in here.

use std::path::Path;
use std::sync::{Mutex, Once, OnceLock, PoisonError};

use headphone_bridge_core::Core;
use jni::objects::{GlobalRef, JClass, JValue};
use jni::{JNIEnv, JavaVM};
use tauri::plugin::mobile::PluginInvokeError;
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{AppHandle, Manager, Runtime};
use tokio::runtime::Runtime as Tokio;

use crate::messages::{BadMessage, Message};
use crate::Host;

/// The Kotlin package with `NativeCore`, `Hub` and `BluetoothPlugin`.
const PACKAGE: &str = "dev.matthewbpt.headphonebridge.bluetooth";

/// Writes to logcat under the `HeadphoneBridge` tag, alongside the Kotlin
/// side's lines; stderr doesn't reach logcat.
pub fn log(message: &str) {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};
    #[link(name = "log")]
    extern "C" {
        fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }
    const INFO: c_int = 4;
    let tag = c"HeadphoneBridge";
    let Ok(text) = CString::new(format!("[rust] {message}").replace('\0', " ")) else {
        return;
    };
    // SAFETY: both are NUL-terminated strings that outlive the call.
    unsafe { __android_log_write(INFO, tag.as_ptr(), text.as_ptr()) };
}

/// Logs every Rust panic to logcat, once per process, whoever starts first:
/// a widget or tile tap can start the process without the Tauri app.
pub fn log_panics() {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log(&format!("panic: {info}"));
            previous(info);
        }));
    });
}

/// The way back to Kotlin: `NativeCore`, whose static `fromCore` takes every
/// message.
struct Bridge {
    vm: JavaVM,
    class: GlobalRef,
}

static BRIDGE: OnceLock<Bridge> = OnceLock::new();
static HOST: OnceLock<Host> = OnceLock::new();

/// Why the process's one core couldn't start.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// Tokio's runtime couldn't be built, e.g. the OS refused it a thread.
    #[error("couldn't start Tokio")]
    Runtime(#[source] std::io::Error),
    /// Kotlin's `NativeCore` couldn't be reached over JNI, so the core would
    /// have no one to send its messages to.
    #[error("can't reach Kotlin")]
    Kotlin(#[source] jni::errors::Error),
}

/// The process's one core, started on first use. Safe to call from any
/// thread, inside an async runtime or not.
pub fn core() -> Result<&'static Core, StartError> {
    Ok(host()?.core())
}

fn host() -> Result<&'static Host, StartError> {
    host_with(None)
}

/// The process's one host, started with the frame log on, appending to
/// `frame_log`, if this call starts it.
fn host_with(frame_log: Option<&Path>) -> Result<&'static Host, StartError> {
    if let Some(host) = HOST.get() {
        return Ok(host);
    }
    let runtime = runtime()?;
    Ok(HOST.get_or_init(|| {
        log_panics();
        let _entered = runtime.enter();
        let mut core = Core::builder();
        if let Some(path) = frame_log {
            if let Err(error) = core.frame_log(path) {
                log(&format!("couldn't open the frame log: {error:?}"));
            }
        }
        let host = Host::start(to_kotlin, core);
        log("core started");
        host
    }))
}

/// The Tokio runtime the core runs on, built on first use. A failed build is
/// tried again on the next call.
fn runtime() -> Result<&'static Tokio, StartError> {
    static RUNTIME: OnceLock<Tokio> = OnceLock::new();
    // Held while building, so no thread builds a runtime only to drop it.
    static BUILDING: Mutex<()> = Mutex::new(());
    let _building = BUILDING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let runtime = Tokio::new().map_err(StartError::Runtime)?;
    Ok(RUNTIME.get_or_init(|| runtime))
}

/// Switches the Simulated Device in front of the process's one core,
/// started on first use. Debug builds alone have it.
#[cfg(debug_assertions)]
pub fn simulator() -> Result<&'static headphone_bridge_simulated::Simulator, StartError> {
    Ok(host()?.simulator())
}

/// Switches the Simulated Device on as `model`, or off with `None`, once the
/// core has started; before that there's nothing to switch. Debug builds
/// alone have it.
#[cfg(debug_assertions)]
pub fn simulate(model: Option<headphone_bridge_core::Model>) {
    if let Some(host) = HOST.get() {
        if let Err(error) = host.simulator().simulate(model) {
            log(&format!("couldn't switch the Simulated Device: {error}"));
        }
    }
}

/// The Simulated Device's Model while it's switched on: never before the
/// core has started.
#[cfg(debug_assertions)]
pub fn simulated() -> Option<headphone_bridge_core::Model> {
    HOST.get().and_then(|host| host.simulator().model())
}

/// Connects Kotlin, once per process, and starts the core if the Tauri setup
/// hasn't, with the frame log on at `frame_log` if the setting is on, so a
/// widget or tile tap's Attach is logged too. From `NativeCore.start`.
pub fn start(env: &mut JNIEnv, class: &JClass, frame_log: Option<&Path>) -> Result<(), StartError> {
    log_panics();
    if BRIDGE.get().is_none() {
        match (env.get_java_vm(), env.new_global_ref(class)) {
            (Ok(vm), Ok(class)) => {
                let _ = BRIDGE.set(Bridge { vm, class });
                log("Kotlin connected");
            }
            (Err(error), _) | (_, Err(error)) => return Err(StartError::Kotlin(error)),
        }
    }
    host_with(frame_log)?.resend_device_list();
    Ok(())
}

/// An event from Kotlin. From `NativeCore.event`. A bad event is logged
/// here: only Kotlin could fix it.
pub fn event(json: &str) -> Result<(), StartError> {
    if let Err(BadMessage(error)) = host()?.event(json) {
        log(&format!("bad event {json}: {error}"));
    }
    Ok(())
}

/// A command from Kotlin, answered with a `commandResult`. From
/// `NativeCore.command`.
pub fn command(request: i64, json: &str) -> Result<(), StartError> {
    host()?.command(request, json);
    Ok(())
}

/// Sends a message to `NativeCore.fromCore`, from any thread. False if Kotlin
/// hasn't connected or the call failed.
fn to_kotlin(message: Message) -> bool {
    let Some(bridge) = BRIDGE.get() else {
        return false;
    };
    let mut env = match bridge.vm.attach_current_thread_permanently() {
        Ok(env) => env,
        Err(error) => {
            log(&format!("can't attach to the JVM: {error}"));
            return false;
        }
    };
    let text = match message.to_json() {
        Ok(text) => text,
        Err(error) => {
            log(&format!("can't serialize a message for Kotlin: {error}"));
            return false;
        }
    };
    let called = env.with_local_frame(4, |env| -> jni::errors::Result<()> {
        let text = env.new_string(&text)?;
        let class: &JClass = bridge.class.as_obj().into();
        env.call_static_method(
            class,
            "fromCore",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&text)],
        )?;
        Ok(())
    });
    if let Err(error) = called {
        log(&format!("fromCore failed: {error}"));
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        return false;
    }
    true
}

/// Why a call to the App Screen's `BluetoothPlugin` failed.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// The Tauri plugin from [`init`] isn't registered on this app.
    #[error("the Android plugin isn't registered")]
    NotRegistered,
    /// Kotlin rejected the call or couldn't be reached. Its text is
    /// Kotlin's, e.g. "No app can share files".
    #[error(transparent)]
    Call(#[from] PluginInvokeError),
}

/// The App Screen's `BluetoothPlugin`, for calling its commands from Rust.
struct BluetoothPlugin<R: Runtime>(PluginHandle<R>);

/// The Tauri plugin that registers `BluetoothPlugin`, which asks for the
/// Nearby devices permission, holds every Connected Device while the App
/// Screen is in the foreground, and shares files.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("android-bluetooth")
        .setup(|app, api| {
            let plugin = api.register_android_plugin(PACKAGE, "BluetoothPlugin")?;
            app.manage(BluetoothPlugin(plugin));
            Ok(())
        })
        .build()
}

/// Whether the app has the Nearby devices permission, without which Kotlin
/// sees no Device. Assumed granted if Kotlin can't say, so a failed check
/// doesn't hide the Devices behind the banner.
pub async fn nearby_devices_granted<R: Runtime>(app: &AppHandle<R>) -> bool {
    #[derive(serde::Deserialize)]
    struct Answer {
        granted: bool,
    }
    let Some(plugin) = app.try_state::<BluetoothPlugin<R>>() else {
        return true;
    };
    match plugin
        .0
        .run_mobile_plugin_async::<Answer>("nearbyDevicesPermission", ())
        .await
    {
        Ok(answer) => answer.granted,
        Err(error) => {
            log(&format!(
                "can't check the Nearby devices permission: {error}"
            ));
            true
        }
    }
}

/// Opens the app's page in Android's settings, where the listener can grant
/// the Nearby devices permission.
pub async fn open_app_settings<R: Runtime>(app: &AppHandle<R>) -> Result<(), PluginError> {
    call(app, "openAppSettings", ()).await
}

/// Hands `file` to Android's share sheet, titled `title`. Resolves once the
/// sheet is open, or with why it couldn't be.
pub async fn share_file<R: Runtime>(
    app: &AppHandle<R>,
    file: &Path,
    title: &str,
) -> Result<(), PluginError> {
    let payload = serde_json::json!({ "path": file, "title": title });
    call(app, "shareFile", payload).await
}

/// Runs `command` on the App Screen's `BluetoothPlugin`, for its side effect.
async fn call<R: Runtime>(
    app: &AppHandle<R>,
    command: &str,
    payload: impl serde::Serialize,
) -> Result<(), PluginError> {
    let plugin = app
        .try_state::<BluetoothPlugin<R>>()
        .ok_or(PluginError::NotRegistered)?;
    plugin
        .0
        .run_mobile_plugin_async::<serde_json::Value>(command, payload)
        .await?;
    Ok(())
}
