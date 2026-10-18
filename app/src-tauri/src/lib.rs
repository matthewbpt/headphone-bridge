//! The Tauri shell: exposes the core's commands as Tauri commands, forwards
//! its Device list snapshots to the UI as `device-list` events and to the tray
//! menu, and keeps the app running in the tray while the window is closed.
//! On Android it's the App Screen alone: no tray, and the window is the app.

#[cfg(target_os = "android")]
mod android;
mod labels;
#[cfg(not(target_os = "android"))]
mod platform;
mod share;
#[cfg(desktop)]
mod tray;

use std::path::PathBuf;
#[cfg(not(target_os = "android"))]
use std::sync::OnceLock;
use std::sync::{Mutex, PoisonError};

#[cfg(not(target_os = "android"))]
use headphone_bridge_core::CoreBuilder;
use headphone_bridge_core::{Command, CommandError, Core, DeviceCommand, DeviceList};
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

/// Why the frame log wasn't switched on or off, as the UI names it. A log
/// directory failure goes under the same name as the core's frame log
/// failure, since the window can't tell them apart.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum SetFrameLogError {
    /// The app's log directory couldn't be found or made.
    #[serde(rename = "frameLogUnavailable")]
    LogDirectory(String),
    /// The core rejected the command.
    #[serde(untagged)]
    Core(CommandError),
}

impl From<CommandError> for SetFrameLogError {
    fn from(error: CommandError) -> SetFrameLogError {
        SetFrameLogError::Core(error)
    }
}

/// The frame log file, in the app's log directory, which is created if needed.
/// Kotlin's `frameLogIfOn` finds it on Android too.
fn frame_log_path(app: &AppHandle) -> Result<PathBuf, SetFrameLogError> {
    let unavailable =
        |error: &dyn std::fmt::Display| SetFrameLogError::LogDirectory(error.to_string());
    let dir = app.path().app_log_dir().map_err(|e| unavailable(&e))?;
    std::fs::create_dir_all(&dir).map_err(|e| unavailable(&e))?;
    Ok(dir.join("frame-log.frames"))
}

/// Exists while the frame log setting is on, so it stays on across launches
/// and logs the Attaches made at startup. Kotlin's `frameLogIfOn` reads it
/// on Android too, when the widget or tile starts the core.
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
fn start_core(app: &AppHandle) -> anyhow::Result<(Core, Option<PathBuf>)> {
    let mut core = Core::builder();
    let path = start_frame_log(app, &mut core);
    Ok((core.start(platform::start()?), path))
}

/// Switches the frame log on for the core `core` starts, if wanted. Returns
/// the log file while it's on.
#[cfg(not(target_os = "android"))]
fn start_frame_log(app: &AppHandle, core: &mut CoreBuilder) -> Option<PathBuf> {
    if !frame_log_wanted(app) {
        return None;
    }
    let path = frame_log_path(app)
        .inspect_err(|error| eprintln!("couldn't make the log directory: {error:?}"))
        .ok()?;
    if let Err(error) = core.frame_log(&path) {
        eprintln!("couldn't open the frame log: {error:?}");
        return None;
    }
    Some(path)
}

/// The process's one core, started here unless Kotlin got there first (with
/// the frame log on if the setting is), with the frame log switched on if
/// wanted. Devices stay Detached until the App Screen holds them, so no
/// Attach goes unlogged. Returns the log file while it's on.
#[cfg(target_os = "android")]
fn android_core(app: &AppHandle) -> anyhow::Result<(&'static Core, Option<PathBuf>)> {
    use anyhow::Context;
    let core =
        headphone_bridge_platform_android::android::core().context("couldn't start the core")?;
    if !frame_log_wanted(app) {
        return Ok((core, None));
    }
    let started = frame_log_path(app).and_then(|path| {
        tauri::async_runtime::block_on(core.command(Command::SetFrameLog(Some(path.clone()))))?;
        Ok(path)
    });
    match started {
        Ok(path) => Ok((core, Some(path))),
        Err(error) => {
            headphone_bridge_platform_android::android::log(&format!(
                "couldn't open the frame log: {error:?}"
            ));
            Ok((core, None))
        }
    }
}

/// Which kind of app the window is in, as the UI names it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum Platform {
    Desktop,
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

/// Runs a command for a Device: every device command the window sends comes
/// through here, as the one flat object Kotlin sends too.
#[tauri::command]
async fn command(
    core: State<'_, &'static Core>,
    command: DeviceCommand,
) -> Result<(), CommandError> {
    core.command(Command::Device(command)).await
}

/// The frame log file, or `None` while the frame log is off.
#[tauri::command]
fn frame_log(frame_log: State<FrameLog>) -> Option<PathBuf> {
    frame_log
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Switches the frame log on or off, from now and at later launches. Returns
/// the log file while it's on.
#[tauri::command]
async fn set_frame_log(
    app: AppHandle,
    core: State<'_, &'static Core>,
    frame_log: State<'_, FrameLog>,
    enabled: bool,
) -> Result<Option<PathBuf>, SetFrameLogError> {
    let path = if enabled {
        Some(frame_log_path(&app)?)
    } else {
        None
    };
    core.command(Command::SetFrameLog(path.clone())).await?;
    save_frame_log_setting(&app, enabled);
    *frame_log.0.lock().unwrap_or_else(PoisonError::into_inner) = path.clone();
    Ok(path)
}

/// The Simulated Device's Model while it's switched on. Debug Android builds
/// alone have it, for the emulator.
#[cfg(all(target_os = "android", debug_assertions))]
#[tauri::command]
fn simulated_device() -> Option<headphone_bridge_core::Model> {
    headphone_bridge_platform_android::android::simulated()
}

/// Switches the Simulated Device on as `model`, or off with `None`.
#[cfg(all(target_os = "android", debug_assertions))]
#[tauri::command]
fn set_simulated_device(model: Option<headphone_bridge_core::Model>) {
    headphone_bridge_platform_android::android::simulate(model);
}

/// Whether Android lets the app use Bluetooth: the Nearby devices permission,
/// as the UI names it.
#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
enum Permission {
    Granted,
    #[cfg_attr(
        desktop,
        expect(dead_code, reason = "only Android asks for permission")
    )]
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

/// Why the app's settings page wasn't opened, as the UI names it.
#[derive(Debug, serde::Serialize)]
enum AppSettingsError {
    /// Only Android has an app settings page.
    #[serde(rename = "appSettingsUnsupported")]
    #[cfg_attr(
        all(target_os = "android", not(test)),
        expect(dead_code, reason = "Android has app settings")
    )]
    Unsupported,
    /// Android couldn't open it.
    #[serde(rename = "appSettingsFailed")]
    #[cfg_attr(
        all(not(target_os = "android"), not(test)),
        expect(dead_code, reason = "only Android opens app settings")
    )]
    Failed(String),
}

/// Opens the app's page in Android's settings, where the listener can grant
/// the Nearby devices permission.
#[tauri::command]
async fn open_app_settings(app: AppHandle) -> Result<(), AppSettingsError> {
    #[cfg(target_os = "android")]
    return headphone_bridge_platform_android::android::open_app_settings(&app)
        .await
        .map_err(|error| AppSettingsError::Failed(error.to_string()));
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Err(AppSettingsError::Unsupported)
    }
}

/// Hands the frame log file to Android's share sheet, so it can be saved or
/// sent from the phone. Desktop shows where the file is instead.
#[tauri::command]
async fn share_frame_log(
    app: AppHandle,
    frame_log: State<'_, FrameLog>,
) -> Result<(), share::ShareError> {
    let file = frame_log
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let file = share::frame_log_to_share(file.as_deref())?;
    #[cfg(target_os = "android")]
    return headphone_bridge_platform_android::android::share_file(&app, file, "Share frame log")
        .await
        .map_err(|error| share::ShareError::ShareFailed(error.to_string()));
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, file);
        Err(share::ShareError::ShareFailed(
            "sharing is only on Android".into(),
        ))
    }
}

/// `error` for the setup hook to return. Tauri keeps only the Display of
/// what the hook returns, so the context chain goes into it here.
fn setup_hook_error(error: &anyhow::Error) -> Box<dyn std::error::Error> {
    format!("{error:#}").into()
}

/// The one message a startup failure prints, with its context chain.
fn startup_failure(error: tauri::Error) -> String {
    format!(
        "{:#}",
        anyhow::Error::from(error).context("Headphone Bridge couldn't start")
    )
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
            // The setup hook runs once, so this is the only core started.
            #[cfg(not(target_os = "android"))]
            let (core, frame_log) = {
                let (core, frame_log) =
                    tauri::async_runtime::block_on(async { start_core(app.handle()) })
                        .map_err(|error| setup_hook_error(&error))?;
                (CORE.get_or_init(|| core), frame_log)
            };
            #[cfg(target_os = "android")]
            let (core, frame_log) =
                android_core(app.handle()).map_err(|error| setup_hook_error(&error))?;
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
            command,
            frame_log,
            set_frame_log,
            nearby_devices_permission,
            open_app_settings,
            share_frame_log,
            #[cfg(all(target_os = "android", debug_assertions))]
            simulated_device,
            #[cfg(all(target_os = "android", debug_assertions))]
            set_simulated_device
        ])
        .build(tauri::generate_context!())
        .unwrap_or_else(|error| {
            let message = startup_failure(error);
            // Android drops stderr, so the failure goes to logcat there.
            #[cfg(target_os = "android")]
            headphone_bridge_platform_android::android::log(&message);
            eprintln!("{message}");
            std::process::exit(1)
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_a_log_directory_failure_as_the_ui_names_the_frame_log_unavailable() {
        let json = |error| serde_json::to_string(&error).unwrap();
        assert_eq!(
            json(SetFrameLogError::LogDirectory(
                "Permission denied (os error 13)".into()
            )),
            r#"{"frameLogUnavailable":"Permission denied (os error 13)"}"#
        );
        assert_eq!(
            json(SetFrameLogError::Core(CommandError::CoreStopped)),
            r#""coreStopped""#
        );
    }

    #[test]
    fn names_each_app_settings_failure_as_the_ui_does() {
        let json = |error| serde_json::to_string(&error).unwrap();
        assert_eq!(
            json(AppSettingsError::Unsupported),
            r#""appSettingsUnsupported""#
        );
        assert_eq!(
            json(AppSettingsError::Failed("no settings app".into())),
            r#"{"appSettingsFailed":"no settings app"}"#
        );
    }

    #[test]
    fn reports_a_startup_failure_in_one_message_with_its_context_chain() {
        let failure = anyhow::anyhow!("Bluetooth is off").context("couldn't start Bluetooth");
        let error = tauri::Error::Setup(setup_hook_error(&failure).into());
        assert_eq!(
            startup_failure(error),
            "Headphone Bridge couldn't start: error encountered during setup hook: \
             couldn't start Bluetooth: Bluetooth is off"
        );
    }
}
