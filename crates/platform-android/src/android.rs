//! The JNI glue: the process's one [`Host`], started by whichever asks first
//! (the Tauri setup or Kotlin's `NativeCore.start`), and the way back to
//! Kotlin. The app crate exports the JNI symbols that call in here.

use std::path::Path;
use std::sync::{Once, OnceLock};

use headphone_bridge_core::Core;
use jni::objects::{GlobalRef, JClass, JValue};
use jni::{JNIEnv, JavaVM};
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
    let text = CString::new(format!("[rust] {message}").replace('\0', " "))
        .expect("no NUL left in the text");
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

/// The process's one core, started on first use. Safe to call from any
/// thread, inside an async runtime or not.
pub fn core() -> &'static Core {
    host().core()
}

fn host() -> &'static Host {
    HOST.get_or_init(|| {
        log_panics();
        static RUNTIME: OnceLock<Tokio> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| Tokio::new().expect("no Tokio runtime"));
        let _entered = runtime.enter();
        let host = Host::start(to_kotlin);
        log("core started");
        host
    })
}

/// Connects Kotlin, once per process, and starts the core if the Tauri setup
/// hasn't. From `NativeCore.start`.
pub fn start(env: &mut JNIEnv, class: &JClass) {
    log_panics();
    if BRIDGE.get().is_none() {
        match (env.get_java_vm(), env.new_global_ref(class)) {
            (Ok(vm), Ok(class)) => {
                let _ = BRIDGE.set(Bridge { vm, class });
                log("Kotlin connected");
            }
            (Err(error), _) | (_, Err(error)) => {
                return log(&format!("can't reach Kotlin: {error}"))
            }
        }
    }
    host().resend_device_list();
}

/// An event from Kotlin. From `NativeCore.event`.
pub fn event(json: &str) {
    if let Err(BadMessage(error)) = host().event(json) {
        log(&format!("bad event {json}: {error}"));
    }
}

/// A command from Kotlin, answered with a `commandResult`. From
/// `NativeCore.command`.
pub fn command(request: i64, json: &str) {
    host().command(request, json);
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
    let text = message.to_json();
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

/// The app screen's `BluetoothPlugin`, for calling its commands from Rust.
struct BluetoothPlugin<R: Runtime>(PluginHandle<R>);

/// The Tauri plugin that registers `BluetoothPlugin`, which asks for the
/// Nearby devices permission, holds every Connected Device while the app
/// screen is in the foreground, and shares files.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("android-bluetooth")
        .setup(|app, api| {
            let plugin = api.register_android_plugin(PACKAGE, "BluetoothPlugin")?;
            app.manage(BluetoothPlugin(plugin));
            Ok(())
        })
        .build()
}

/// Hands `file` to Android's share sheet, titled `title`. Resolves once the
/// sheet is open, or with why it couldn't be.
pub async fn share_file<R: Runtime>(
    app: &AppHandle<R>,
    file: &Path,
    title: &str,
) -> Result<(), String> {
    let plugin = app
        .try_state::<BluetoothPlugin<R>>()
        .ok_or("the Android plugin isn't registered")?;
    plugin
        .0
        .run_mobile_plugin_async::<serde_json::Value>(
            "shareFile",
            serde_json::json!({ "path": file, "title": title }),
        )
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}
