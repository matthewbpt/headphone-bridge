//! The native tray menu. It shows the Primary Device's name and battery, its
//! Noise Control Mode as radio items, "Open Headphone Bridge" and "Quit".
//!
//! Tray actions only use menu items, never tray click events, because Linux
//! doesn't emit them.

use std::sync::Mutex;

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{Battery, Command, Core, DeviceList, DeviceState, NoiseControlMode};
use tauri::menu::{CheckMenuItem, Menu, MenuBuilder, MenuEvent, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager};

const OPEN: &str = "open";
const QUIT: &str = "quit";
/// Prefixes a Noise Control Mode item's id, e.g. `mode:ambient`.
const MODE: &str = "mode:";

/// The tray icon and the menu it's showing.
struct Tray {
    icon: TrayIcon,
    menu: Mutex<TrayMenu>,
}

/// Adds the tray icon, showing "No device" until the first snapshot.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = TrayMenu::new(&DeviceList::default());
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Headphone Bridge")
        .menu(&build(app, &menu)?)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let icon = builder.build(app)?;
    app.manage(Tray {
        icon,
        menu: Mutex::new(menu),
    });
    Ok(())
}

/// Rebuilds the menu if `list` changes what it shows. Battery ticks that
/// don't change the text leave it alone, so an open menu doesn't flicker.
pub fn update(app: &AppHandle, list: &DeviceList) {
    let tray = app.state::<Tray>();
    let menu = TrayMenu::new(list);
    {
        let mut shown = tray.menu.lock().unwrap();
        if *shown == menu {
            return;
        }
        *shown = menu.clone();
    }
    // Building a menu waits on the main thread, so never hold the lock here.
    show(app, &tray.icon, &menu);
}

fn show(app: &AppHandle, icon: &TrayIcon, menu: &TrayMenu) {
    match build(app, menu) {
        Ok(built) => {
            let _ = icon.set_menu(Some(built));
        }
        Err(error) => eprintln!("couldn't build the tray menu: {error}"),
    }
}

fn build(app: &AppHandle, menu: &TrayMenu) -> tauri::Result<Menu<tauri::Wry>> {
    let mut builder = MenuBuilder::new(app).item(&MenuItem::with_id(
        app,
        "title",
        &menu.title,
        false,
        None::<&str>,
    )?);
    if let Some(battery) = &menu.battery {
        builder = builder.item(&MenuItem::with_id(
            app,
            "battery",
            battery,
            false,
            None::<&str>,
        )?);
    }
    if !menu.modes.is_empty() {
        builder = builder.separator();
        for item in &menu.modes {
            builder = builder.item(&CheckMenuItem::with_id(
                app,
                format!("{MODE}{}", mode_id(item.mode)),
                item.label,
                true,
                item.checked,
                None::<&str>,
            )?);
        }
    }
    builder
        .separator()
        .text(OPEN, "Open Headphone Bridge")
        .text(QUIT, "Quit")
        .build()
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        OPEN => crate::show_window(app),
        QUIT => app.exit(0),
        id => {
            let Some(mode) = id.strip_prefix(MODE).and_then(mode_from_id) else {
                return;
            };
            let tray = app.state::<Tray>();
            let shown = tray.menu.lock().unwrap().clone();
            // A check item toggles itself when clicked. Put the check marks
            // back as the Device last confirmed them: the radio selection
            // moves only once the Device confirms the new mode.
            show(app, &tray.icon, &shown);
            let Some(address) = shown.device.clone() else {
                return;
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let core = app.state::<Core>();
                let command = Command::SetNoiseControlMode(address, mode);
                if let Err(error) = core.command(command).await {
                    eprintln!("couldn't set the Noise Control Mode from the tray: {error:?}");
                }
            });
        }
    }
}

const MODES: [NoiseControlMode; 4] = [
    NoiseControlMode::Off,
    NoiseControlMode::NoiseCancelling,
    NoiseControlMode::Ambient,
    NoiseControlMode::Adaptive,
];

fn mode_id(mode: NoiseControlMode) -> &'static str {
    match mode {
        NoiseControlMode::Off => "off",
        NoiseControlMode::NoiseCancelling => "noiseCancelling",
        NoiseControlMode::Ambient => "ambient",
        NoiseControlMode::Adaptive => "adaptive",
    }
}

fn mode_from_id(id: &str) -> Option<NoiseControlMode> {
    MODES.into_iter().find(|&mode| mode_id(mode) == id)
}

/// What the tray menu shows, derived from a Device list snapshot. The menu is
/// rebuilt only when this changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenu {
    /// The Primary Device's name, or "No device".
    pub title: String,
    pub battery: Option<String>,
    /// The Primary Device, while one is Attached.
    pub device: Option<DeviceAddress>,
    /// Limited to the Primary Device's Capabilities.
    pub modes: Vec<ModeItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeItem {
    pub mode: NoiseControlMode,
    pub label: &'static str,
    /// Whether this is the Chosen Mode the Device last confirmed.
    pub checked: bool,
}

impl TrayMenu {
    pub fn new(list: &DeviceList) -> TrayMenu {
        // Until Primary Device selection lands, the Primary Device is the one
        // Attached Device.
        let primary = list
            .devices
            .iter()
            .find(|device| device.state == DeviceState::Attached);
        let Some(device) = primary else {
            return TrayMenu {
                title: "No device".into(),
                battery: None,
                device: None,
                modes: vec![],
            };
        };
        TrayMenu {
            title: device.name.clone(),
            battery: device.battery.map(battery_text),
            device: Some(device.address.clone()),
            modes: device
                .capabilities
                .noise_control_modes
                .iter()
                .map(|&mode| ModeItem {
                    mode,
                    label: mode_label(mode),
                    checked: device.chosen_mode == Some(mode),
                })
                .collect(),
        }
    }
}

fn battery_text(battery: Battery) -> String {
    let percent = |level: Option<u8>| level.map_or("–".into(), |level| format!("{level}%"));
    match battery {
        Battery::Buds { left, right, case } => format!(
            "Battery: L {} · R {} · Case {}",
            percent(left),
            percent(right),
            percent(case)
        ),
        Battery::Headphones { level } => format!("Battery: {level}%"),
    }
}

fn mode_label(mode: NoiseControlMode) -> &'static str {
    match mode {
        NoiseControlMode::Off => "Off",
        NoiseControlMode::NoiseCancelling => "Noise Cancelling",
        NoiseControlMode::Ambient => "Ambient",
        NoiseControlMode::Adaptive => "Adaptive",
    }
}

#[cfg(test)]
mod tests {
    use headphone_bridge_core::{Device, Model};

    use super::*;

    #[test]
    fn reads_no_device_when_nothing_is_attached() {
        assert_eq!(
            TrayMenu::new(&DeviceList::default()),
            TrayMenu {
                title: "No device".into(),
                battery: None,
                device: None,
                modes: vec![],
            }
        );
    }

    fn buds(state: DeviceState) -> Device {
        Device {
            address: DeviceAddress("AA:BB:CC:DD:EE:FF".into()),
            name: "Matt's Buds4 Pro".into(),
            model: Some(Model::GalaxyBuds4Pro),
            state,
            capabilities: Model::GalaxyBuds4Pro.capabilities(),
            chosen_mode: Some(NoiseControlMode::Ambient),
            ambient_level: None,
            battery: Some(Battery::Buds {
                left: Some(80),
                right: Some(75),
                case: None,
            }),
        }
    }

    fn mode(mode: NoiseControlMode, label: &'static str, checked: bool) -> ModeItem {
        ModeItem {
            mode,
            label,
            checked,
        }
    }

    #[test]
    fn shows_the_attached_device_with_its_chosen_mode_checked() {
        let list = DeviceList {
            devices: vec![buds(DeviceState::Attached)],
        };
        assert_eq!(
            TrayMenu::new(&list),
            TrayMenu {
                title: "Matt's Buds4 Pro".into(),
                battery: Some("Battery: L 80% · R 75% · Case –".into()),
                device: Some(DeviceAddress("AA:BB:CC:DD:EE:FF".into())),
                modes: vec![
                    mode(NoiseControlMode::Off, "Off", false),
                    mode(NoiseControlMode::NoiseCancelling, "Noise Cancelling", false),
                    mode(NoiseControlMode::Ambient, "Ambient", true),
                    mode(NoiseControlMode::Adaptive, "Adaptive", false),
                ],
            }
        );
    }

    #[test]
    fn reads_no_device_while_the_only_device_is_not_attached() {
        let list = DeviceList {
            devices: vec![buds(DeviceState::Attaching)],
        };
        assert_eq!(TrayMenu::new(&list).title, "No device");
        assert_eq!(TrayMenu::new(&list).modes, vec![]);
    }

    #[test]
    fn offers_only_the_modes_in_the_headphones_capabilities() {
        let list = DeviceList {
            devices: vec![Device {
                address: DeviceAddress("11:22:33:44:55:66".into()),
                name: "WH-1000XM5".into(),
                model: Some(Model::Wh1000Xm5),
                state: DeviceState::Attached,
                capabilities: Model::Wh1000Xm5.capabilities(),
                chosen_mode: Some(NoiseControlMode::NoiseCancelling),
                ambient_level: None,
                battery: Some(Battery::Headphones { level: 70 }),
            }],
        };
        assert_eq!(
            TrayMenu::new(&list),
            TrayMenu {
                title: "WH-1000XM5".into(),
                battery: Some("Battery: 70%".into()),
                device: Some(DeviceAddress("11:22:33:44:55:66".into())),
                modes: vec![
                    mode(NoiseControlMode::Off, "Off", false),
                    mode(NoiseControlMode::NoiseCancelling, "Noise Cancelling", true),
                    mode(NoiseControlMode::Ambient, "Ambient", false),
                ],
            }
        );
    }
}
