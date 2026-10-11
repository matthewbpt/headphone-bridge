//! The Tauri shell: exposes the core's commands as Tauri commands and forwards
//! its Device list snapshots to the UI as `device-list` events.

mod platform;
mod tray;

use std::path::PathBuf;
use std::sync::Mutex;

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{Command, CommandError, Core, DeviceList, NoiseControlMode};
use tauri::{AppHandle, Emitter, Manager, State};

/// The frame log file while the frame log is on. It starts off at every launch.
#[derive(Default)]
struct FrameLog(Mutex<Option<PathBuf>>);

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
async fn retry_attach(core: State<'_, Core>, address: String) -> Result<(), CommandError> {
    core.command(Command::RetryAttach(DeviceAddress(address)))
        .await
}

/// The frame log file, or `None` while the frame log is off.
#[tauri::command]
fn frame_log(frame_log: State<FrameLog>) -> Option<PathBuf> {
    frame_log.0.lock().unwrap().clone()
}

/// Switches the frame log on or off. Returns the log file while it's on.
#[tauri::command]
async fn set_frame_log(
    app: AppHandle,
    core: State<'_, Core>,
    frame_log: State<'_, FrameLog>,
    enabled: bool,
) -> Result<Option<PathBuf>, CommandError> {
    let path = if enabled {
        let unavailable =
            |error: &dyn std::fmt::Display| CommandError::FrameLogUnavailable(error.to_string());
        let dir = app.path().app_log_dir().map_err(|e| unavailable(&e))?;
        std::fs::create_dir_all(&dir).map_err(|e| unavailable(&e))?;
        Some(dir.join("frame-log.frames"))
    } else {
        None
    };
    core.command(Command::SetFrameLog(path.clone())).await?;
    *frame_log.0.lock().unwrap() = path.clone();
    Ok(path)
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let core = tauri::async_runtime::block_on(async { Core::start(platform::start()) });
            let mut snapshots = core.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while snapshots.changed().await.is_ok() {
                    let snapshot = snapshots.borrow_and_update().clone();
                    let _ = handle.emit("device-list", snapshot);
                }
            });
            app.manage(core);
            app.manage(FrameLog::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            device_list,
            set_noise_control_mode,
            set_ambient_level,
            retry_attach,
            frame_log,
            set_frame_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running Headphone Bridge");
}
