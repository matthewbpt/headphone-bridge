//! The Tauri shell: exposes the core's commands as Tauri commands and forwards
//! its Device list snapshots to the UI as `device-list` events.

mod platform;

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{Command, CommandError, Core, DeviceList, NoiseControlMode};
use tauri::{Emitter, Manager, State};

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

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let core = tauri::async_runtime::block_on(async { Core::start(platform::NoBluetooth) });
            let mut snapshots = core.subscribe();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while snapshots.changed().await.is_ok() {
                    let snapshot = snapshots.borrow_and_update().clone();
                    let _ = handle.emit("device-list", snapshot);
                }
            });
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            device_list,
            set_noise_control_mode,
            set_ambient_level,
            retry_attach
        ])
        .run(tauri::generate_context!())
        .expect("error while running Headphone Bridge");
}
