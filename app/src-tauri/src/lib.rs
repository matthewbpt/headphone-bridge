//! The Tauri shell: exposes the core's commands as Tauri commands, forwards
//! its Device list snapshots to the UI as `device-list` events and to the tray
//! menu, and keeps the app running in the tray while the window is closed.
//! On Android it's the app screen alone: no tray, and the window is the app.

#[cfg(target_os = "android")]
mod android;
mod labels;
#[cfg(not(target_os = "android"))]
mod platform;
#[cfg(desktop)]
mod tray;

use std::path::PathBuf;
use std::sync::Mutex;
#[cfg(not(target_os = "android"))]
use std::sync::OnceLock;

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{Command, CommandError, Core, DeviceList, NoiseControlMode};
use tauri::{AppHandle, Emitter, Manager, State};

/// The process's one core. On Android it's the platform adapter's instead,
/// shared with the widget and the tile (ADR-0005).
#[cfg(not(target_os = "android"))]
static CORE: OnceLock<Core> = OnceLock::new();

/// The frame log file while the frame log is on.
#[derive(Default)]
struct FrameLog(Mutex<Option<PathBuf>>);

/// Set (to anything) to start with the frame log on, whatever the setting.
const FRAME_LOG_ENV: &str = "HEADPHONE_BRIDGE_FRAME_LOG";

/// The frame log file, in the app's log directory, which is created if needed.
fn frame_log_path(app: &AppHandle) -> Result<PathBuf, CommandError> {
    let unavailable =
        |error: &dyn std::fmt::Display| CommandError::FrameLogUnavailable(error.to_string());
    let dir = app.path().app_log_dir().map_err(|e| unavailable(&e))?;
    std::fs::create_dir_all(&dir).map_err(|e| unavailable(&e))?;
    Ok(dir.join("frame-log.frames"))
}

/// Exists while the frame log setting is on, so it stays on across launches
/// and logs the Attaches made at startup.
fn frame_log_setting(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("frame-log-on"))
}

/// Saves the frame log setting for the next launch, as best it can.
fn save_frame_log_setting(app: &AppHandle, enabled: bool) {
    let Some(setting) = frame_log_setting(app) else {
        return;
    };
    let saved = if enabled {
        setting
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&setting, ""))
    } else {
        std::fs::remove_file(&setting).or_else(|error| match error.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(error),
        })
    };
    if let Err(error) = saved {
        eprintln!("couldn't save the frame log setting: {error}");
    }
}

/// Whether the frame log should be on at startup: the setting or
/// [`FRAME_LOG_ENV`] says so.
fn frame_log_wanted(app: &AppHandle) -> bool {
    std::env::var_os(FRAME_LOG_ENV).is_some()
        || frame_log_setting(app).is_some_and(|setting| setting.exists())
}

/// Starts the core, with the frame log on if wanted. Returns the log file
/// while it's on.
#[cfg(not(target_os = "android"))]
fn start_core(app: &AppHandle) -> (Core, Option<PathBuf>) {
    let platform = platform::start();
    let path = frame_log_wanted(app)
        .then(|| frame_log_path(app))
        .transpose();
    match path {
        Ok(Some(path)) => match Core::start_with_frame_log(platform, &path) {
            (core, Ok(())) => (core, Some(path)),
            (core, Err(error)) => {
                eprintln!("couldn't open the frame log: {error:?}");
                (core, None)
            }
        },
        Ok(None) => (Core::start(platform), None),
        Err(error) => {
            eprintln!("couldn't open the frame log: {error:?}");
            (Core::start(platform), None)
        }
    }
}

/// The process's one core, started here unless Kotlin got there first, with
/// the frame log switched on if wanted. Devices stay Detached until the app
/// screen holds them, so no Attach goes unlogged. Returns the log file while
/// it's on.
#[cfg(target_os = "android")]
fn android_core(app: &AppHandle) -> (&'static Core, Option<PathBuf>) {
    let core = headphone_bridge_platform_android::android::core();
    if !frame_log_wanted(app) {
        return (core, None);
    }
    let started = frame_log_path(app).and_then(|path| {
        tauri::async_runtime::block_on(core.command(Command::SetFrameLog(Some(path.clone()))))?;
        Ok(path)
    });
    match started {
        Ok(path) => (core, Some(path)),
        Err(error) => {
            headphone_bridge_platform_android::android::log(&format!(
                "couldn't open the frame log: {error:?}"
            ));
            (core, None)
        }
    }
}

/// Which kind of app the window is in, as the UI names it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum Platform {
    #[cfg_attr(mobile, allow(dead_code))]
    Desktop,
    #[cfg_attr(desktop, allow(dead_code))]
    Android,
}

/// Which kind of app the window is in, so it shows only that one's settings.
#[tauri::command]
fn platform() -> Platform {
    if cfg!(target_os = "android") {
        Platform::Android
    } else {
        Platform::Desktop
    }
}

/// Every label the window shows, the same as the tray's.
#[tauri::command]
fn labels() -> labels::Labels {
    labels::all()
}

#[tauri::command]
fn device_list(core: State<&'static Core>) -> DeviceList {
    core.device_list()
}

#[tauri::command]
async fn set_noise_control_mode(
    core: State<'_, &'static Core>,
    address: String,
    mode: NoiseControlMode,
) -> Result<(), CommandError> {
    core.command(Command::SetNoiseControlMode(DeviceAddress(address), mode))
        .await
}

#[tauri::command]
async fn set_ambient_level(
    core: State<'_, &'static Core>,
    address: String,
    level: u8,
) -> Result<(), CommandError> {
    core.command(Command::SetAmbientLevel(DeviceAddress(address), level))
        .await
}

#[tauri::command]
async fn set_noise_cancelling_level(
    core: State<'_, &'static Core>,
    address: String,
    level: u8,
) -> Result<(), CommandError> {
    core.command(Command::SetNoiseCancellingLevel(
        DeviceAddress(address),
        level,
    ))
    .await
}

#[tauri::command]
async fn retry_attach(core: State<'_, &'static Core>, address: String) -> Result<(), CommandError> {
    core.command(Command::RetryAttach(DeviceAddress(address)))
        .await
}

#[tauri::command]
async fn power_off(core: State<'_, &'static Core>, address: String) -> Result<(), CommandError> {
    core.command(Command::PowerOff(DeviceAddress(address)))
        .await
}

/// The frame log file, or `None` while the frame log is off.
#[tauri::command]
fn frame_log(frame_log: State<FrameLog>) -> Option<PathBuf> {
    frame_log.0.lock().unwrap().clone()
}

/// Switches the frame log on or off, from now and at later launches. Returns
/// the log file while it's on.
#[tauri::command]
async fn set_frame_log(
    app: AppHandle,
    core: State<'_, &'static Core>,
    frame_log: State<'_, FrameLog>,
    enabled: bool,
) -> Result<Option<PathBuf>, CommandError> {
    let path = if enabled {
        Some(frame_log_path(&app)?)
    } else {
        None
    };
    core.command(Command::SetFrameLog(path.clone())).await?;
    save_frame_log_setting(&app, enabled);
    *frame_log.0.lock().unwrap() = path.clone();
    Ok(path)
}

/// Whether Android lets the app use Bluetooth: the Nearby devices permission,
/// as the UI names it.
#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum Permission {
    Granted,
    #[cfg_attr(desktop, allow(dead_code))]
    Refused,
}

/// The Nearby devices permission. Desktop has none, so it's always granted.
#[tauri::command]
async fn nearby_devices_permission(app: AppHandle) -> Permission {
    #[cfg(target_os = "android")]
    if !headphone_bridge_platform_android::android::nearby_devices_granted(&app).await {
        return Permission::Refused;
    }
    let _ = app;
    Permission::Granted
}

/// Re-reads the Nearby devices permission and sends it to the window as a
/// `nearby-devices-permission` event. On Android, the listener may have just
/// answered the permission prompt or changed it in the app's settings.
#[cfg(target_os = "android")]
fn report_nearby_devices_permission(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let permission = nearby_devices_permission(app.clone()).await;
        let _ = app.emit("nearby-devices-permission", permission);
    });
}

/// Opens the app's page in Android's settings, where the listener can grant
/// the Nearby devices permission.
#[tauri::command]
async fn open_app_settings(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "android")]
    return headphone_bridge_platform_android::android::open_app_settings(&app).await;
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err("only Android has app settings".into())
    }
}

/// Passed by "Launch at login", so the app starts in the tray.
#[cfg(desktop)]
const HIDDEN: &str = "--hidden";

/// Shows the window. On macOS the Dock icon appears only while it's visible.
#[cfg(desktop)]
fn show_window(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "android")]
    headphone_bridge_platform_android::android::log_panics();
    let builder = tauri::Builder::default();
    #[cfg(target_os = "android")]
    let builder = builder.plugin(headphone_bridge_platform_android::android::init());
    #[cfg(desktop)]
    let builder = builder
        // First, so a second launch shows this one's window instead of
        // fighting it for the Devices' control links.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app)
        }))
        .plugin(tauri_plugin_autostart::Builder::new().arg(HIDDEN).build());
    builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            #[cfg(desktop)]
            tray::install(app.handle())?;
            #[cfg(not(target_os = "android"))]
            let (core, frame_log) = {
                let mut frame_log = None;
                let core = CORE.get_or_init(|| {
                    let (core, log) =
                        tauri::async_runtime::block_on(async { start_core(app.handle()) });
                    frame_log = log;
                    core
                });
                (core, frame_log)
            };
            #[cfg(target_os = "android")]
            let (core, frame_log) = android_core(app.handle());
            let mut snapshots = core.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while snapshots.changed().await.is_ok() {
                    let snapshot = snapshots.borrow_and_update().clone();
                    #[cfg(desktop)]
                    tray::update(&handle, &snapshot);
                    let _ = handle.emit("device-list", snapshot);
                }
            });
            app.manage(core);
            app.manage(FrameLog(Mutex::new(frame_log)));
            #[cfg(desktop)]
            if !std::env::args().any(|arg| arg == HIDDEN) {
                show_window(app.handle());
            }
            // The config's window starts hidden for the tray; a phone has none.
            #[cfg(mobile)]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it to the tray; "Quit" is in the tray.
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                #[cfg(target_os = "macos")]
                let _ = window
                    .app_handle()
                    .set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
            // Back from the permission prompt or the app's settings.
            #[cfg(target_os = "android")]
            if matches!(
                event,
                tauri::WindowEvent::Resumed | tauri::WindowEvent::Focused(true)
            ) {
                report_nearby_devices_permission(window.app_handle());
            }
            let _ = (window, event);
        })
        .invoke_handler(tauri::generate_handler![
            platform,
            labels,
            device_list,
            set_noise_control_mode,
            set_ambient_level,
            set_noise_cancelling_level,
            retry_attach,
            power_off,
            frame_log,
            set_frame_log,
            nearby_devices_permission,
            open_app_settings
        ])
        .build(tauri::generate_context!())
        .expect("error while building Headphone Bridge")
        .run(|app, event| {
            // Clicking the Dock icon while the window is hidden.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } = event
            {
                show_window(app);
            }
            let _ = (app, event);
        });
}
