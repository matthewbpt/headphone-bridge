//! The Tauri shell: exposes the core's commands as Tauri commands, forwards
//! its Device list snapshots to the UI as `device-list` events and to the tray
//! menu, and keeps the app running in the tray while the window is closed.

mod labels;
mod platform;
mod tray;

use std::path::PathBuf;
use std::sync::Mutex;

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{Command, CommandError, Core, DeviceList, NoiseControlMode};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

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

/// Starts the core, with the frame log on if the setting or
/// [`FRAME_LOG_ENV`] says so. Returns the log file while it's on.
fn start_core(app: &AppHandle) -> (Core, Option<PathBuf>) {
    let platform = platform::start();
    let enabled = std::env::var_os(FRAME_LOG_ENV).is_some()
        || frame_log_setting(app).is_some_and(|setting| setting.exists());
    let path = enabled.then(|| frame_log_path(app)).transpose();
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

/// Every label the window shows, the same as the tray's.
#[tauri::command]
fn labels() -> labels::Labels {
    labels::all()
}

#[tauri::command]
fn device_list(core: State<Core>) -> DeviceList {
    core.device_list()
}

#[tauri::command]
async fn set_noise_control_mode(
    core: State<'_, Core>,
    address: String,
    mode: NoiseControlMode,
) -> Result<(), CommandError> {
    core.command(Command::SetNoiseControlMode(DeviceAddress(address), mode))
        .await
}

#[tauri::command]
async fn set_ambient_level(
    core: State<'_, Core>,
    address: String,
    level: u8,
) -> Result<(), CommandError> {
    core.command(Command::SetAmbientLevel(DeviceAddress(address), level))
        .await
}

#[tauri::command]
async fn set_noise_cancelling_level(
    core: State<'_, Core>,
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
async fn retry_attach(core: State<'_, Core>, address: String) -> Result<(), CommandError> {
    core.command(Command::RetryAttach(DeviceAddress(address)))
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
    core: State<'_, Core>,
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

/// Passed by "Launch at login", so the app starts in the tray.
const HIDDEN: &str = "--hidden";

/// Shows the window. On macOS the Dock icon appears only while it's visible.
fn show_window(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        // First, so a second launch shows this one's window instead of
        // fighting it for the Devices' control links.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app)
        }))
        .plugin(tauri_plugin_autostart::Builder::new().arg(HIDDEN).build())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray::install(app.handle())?;
            let (core, frame_log) =
                tauri::async_runtime::block_on(async { start_core(app.handle()) });
            let mut snapshots = core.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while snapshots.changed().await.is_ok() {
                    let snapshot = snapshots.borrow_and_update().clone();
                    tray::update(&handle, &snapshot);
                    let _ = handle.emit("device-list", snapshot);
                }
            });
            app.manage(core);
            app.manage(FrameLog(Mutex::new(frame_log)));
            if !std::env::args().any(|arg| arg == HIDDEN) {
                show_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it to the tray; "Quit" is in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                #[cfg(target_os = "macos")]
                let _ = window
                    .app_handle()
                    .set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
        })
        .invoke_handler(tauri::generate_handler![
            labels,
            device_list,
            set_noise_control_mode,
            set_ambient_level,
            set_noise_cancelling_level,
            retry_attach,
            frame_log,
            set_frame_log
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
