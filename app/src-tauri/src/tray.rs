//! The native tray menu, labelled from [`crate::labels`]. It shows the Primary Device's name and battery, its
//! Chosen Mode as radio items with a hint while the Device has paused it,
//! "Power Off" when it has that Capability, "Open Headphone Bridge" and
//! "Quit".
//!
//! Tray actions only use menu items, never tray click events, because Linux
//! doesn't emit them.

use std::sync::Mutex;

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{
    Battery, Command, Core, DeviceList, DeviceState, NoiseControlMode, NotAttachedReason,
};
use tauri::menu::{CheckMenuItem, Menu, MenuBuilder, MenuEvent, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager};

use crate::labels;

const OPEN: &str = "open";
const QUIT: &str = "quit";
const POWER_OFF: &str = "power-off";
/// Prefixes a Noise Control Mode item's id, e.g. `mode:ambient`.
const MODE: &str = "mode:";

/// The tray icon and the menu it's showing.
struct Tray {
    icon: TrayIcon,
    menu: Mutex<TrayMenu>,
}

/// Adds the tray icon, showing "No Device" until the first snapshot.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = TrayMenu::new(&DeviceList::default());
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Headphone Bridge")
        .menu(&build(app, &menu)?)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event);
    // macOS tints a template image to suit a light or dark menu bar. Linux
    // panels don't, and a black icon would vanish on GNOME's dark panel.
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .icon(tauri::include_image!("icons/tray-template.png"))
            .icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
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
    let mut builder = MenuBuilder::new(app);
    for entry in menu.entries() {
        builder = match entry {
            Entry::Separator => builder.separator(),
            Entry::Info { id, text } => {
                builder.item(&MenuItem::with_id(app, id, text, false, None::<&str>)?)
            }
            Entry::Check {
                id,
                label,
                enabled,
                checked,
            } => builder.item(&CheckMenuItem::with_id(
                app,
                id,
                label,
                enabled,
                checked,
                None::<&str>,
            )?),
            Entry::Action { id, label, enabled } => {
                builder.item(&MenuItem::with_id(app, id, label, enabled, None::<&str>)?)
            }
        };
    }
    builder.build()
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let tray = app.state::<Tray>();
    let shown = tray.menu.lock().unwrap().clone();
    match shown.click(event.id().as_ref()) {
        Some(Click::Open) => crate::show_window(app),
        Some(Click::Quit) => app.exit(0),
        Some(Click::Send(command)) => {
            if let Command::SetNoiseControlMode(..) = command {
                // A check item toggles itself when clicked. Put the check
                // marks back as the Device last confirmed them: the radio
                // selection moves only once the Device confirms the new mode.
                show(app, &tray.icon, &shown);
            }
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let core = app.state::<Core>();
                if let Err(error) = core.command(command.clone()).await {
                    eprintln!("couldn't send {command:?} from the tray: {error:?}");
                }
            });
        }
        None => {}
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
    /// The Primary Device's name, or "No Device".
    pub title: String,
    pub battery: Option<String>,
    /// Why the Device isn't applying the Chosen Mode, while it isn't.
    pub hint: Option<String>,
    /// Why the Device named in the title isn't Attached.
    pub status: Option<String>,
    /// The Primary Device, while one is Attached or Powering Off.
    pub device: Option<DeviceAddress>,
    /// Limited to the Primary Device's Capabilities.
    pub modes: Vec<ModeItem>,
    /// Shown after the modes when the Primary Device has the Power Off
    /// Capability.
    pub power_off: Option<PowerOffItem>,
}

/// One line of the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Text that can't be clicked.
    Info {
        id: &'static str,
        text: String,
    },
    /// A Noise Control Mode.
    Check {
        id: String,
        label: &'static str,
        enabled: bool,
        checked: bool,
    },
    Action {
        id: &'static str,
        label: &'static str,
        enabled: bool,
    },
    Separator,
}

/// What clicking a tray menu item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Click {
    Open,
    Quit,
    /// Sends a command for the Primary Device to the core.
    Send(Command),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerOffItem {
    pub label: &'static str,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeItem {
    pub mode: NoiseControlMode,
    pub label: &'static str,
    /// Whether this is the Chosen Mode the Device last confirmed.
    pub checked: bool,
    /// False while the Device is Powering Off.
    pub enabled: bool,
}

impl TrayMenu {
    pub fn new(list: &DeviceList) -> TrayMenu {
        let primary = list.primary.as_ref().and_then(|primary| {
            list.devices.iter().find(|device| {
                device.address == *primary
                    && matches!(
                        device.state,
                        DeviceState::Attached | DeviceState::PoweringOff
                    )
            })
        });
        let Some(device) = primary else {
            // Failing that, a supported Device that couldn't be Attached, and why.
            let not_attached = list.devices.iter().find_map(|device| match &device.state {
                // An unsupported Model is left out.
                DeviceState::NotAttached { reason }
                    if *reason != NotAttachedReason::UnsupportedModel =>
                {
                    Some((device.name.clone(), labels::reason(reason)))
                }
                _ => None,
            });
            let (title, status) = match not_attached {
                Some((name, status)) => (name, Some(status)),
                None => (labels::NO_DEVICE.into(), None),
            };
            return TrayMenu {
                title,
                battery: None,
                hint: None,
                status,
                device: None,
                modes: vec![],
                power_off: None,
            };
        };
        let powering_off = device.state == DeviceState::PoweringOff;
        TrayMenu {
            title: device.name.clone(),
            battery: device.battery.map(battery_text),
            hint: device
                .effective_mode_reason
                .map(|reason| labels::hint(reason).into()),
            status: None,
            device: Some(device.address.clone()),
            modes: device
                .capabilities
                .noise_control_modes
                .iter()
                .map(|&mode| ModeItem {
                    mode,
                    label: labels::mode(mode),
                    checked: device.chosen_mode == Some(mode),
                    enabled: !powering_off,
                })
                .collect(),
            power_off: device.capabilities.power_off.then_some(PowerOffItem {
                label: if powering_off {
                    labels::POWERING_OFF
                } else {
                    labels::POWER_OFF
                },
                enabled: !powering_off,
            }),
        }
    }

    /// The menu's lines, in order: the title, status and battery; the modes
    /// and hint; Power Off; then "Open Headphone Bridge" and "Quit", each
    /// group after a separator.
    pub fn entries(&self) -> Vec<Entry> {
        let mut entries = vec![Entry::Info {
            id: "title",
            text: self.title.clone(),
        }];
        if let Some(status) = &self.status {
            entries.push(Entry::Info {
                id: "status",
                text: status.clone(),
            });
        }
        if let Some(battery) = &self.battery {
            entries.push(Entry::Info {
                id: "battery",
                text: battery.clone(),
            });
        }
        if !self.modes.is_empty() {
            entries.push(Entry::Separator);
            entries.extend(self.modes.iter().map(|item| Entry::Check {
                id: format!("{MODE}{}", mode_id(item.mode)),
                label: item.label,
                enabled: item.enabled,
                checked: item.checked,
            }));
            if let Some(hint) = &self.hint {
                entries.push(Entry::Info {
                    id: "hint",
                    text: hint.clone(),
                });
            }
        }
        if let Some(item) = &self.power_off {
            entries.push(Entry::Separator);
            entries.push(Entry::Action {
                id: POWER_OFF,
                label: item.label,
                enabled: item.enabled,
            });
        }
        entries.push(Entry::Separator);
        entries.push(Entry::Action {
            id: OPEN,
            label: "Open Headphone Bridge",
            enabled: true,
        });
        entries.push(Entry::Action {
            id: QUIT,
            label: "Quit",
            enabled: true,
        });
        entries
    }

    /// What clicking the item with `id` does. Every item id is read here.
    pub fn click(&self, id: &str) -> Option<Click> {
        match id {
            OPEN => return Some(Click::Open),
            QUIT => return Some(Click::Quit),
            _ => {}
        }
        let address = self.device.clone()?;
        if id == POWER_OFF {
            return self
                .power_off
                .is_some()
                .then_some(Click::Send(Command::PowerOff(address)));
        }
        let mode = id.strip_prefix(MODE).and_then(mode_from_id)?;
        Some(Click::Send(Command::SetNoiseControlMode(address, mode)))
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

#[cfg(test)]
mod tests {
    use headphone_bridge_core::{Device, EffectiveModeReason, Model};

    use super::*;

    #[test]
    fn reads_no_device_when_nothing_is_attached() {
        assert_eq!(
            TrayMenu::new(&DeviceList::default()),
            TrayMenu {
                title: "No Device".into(),
                battery: None,
                hint: None,
                status: None,
                device: None,
                modes: vec![],
                power_off: None,
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
            effective_mode: Some(NoiseControlMode::Ambient),
            effective_mode_reason: None,
            ambient_level: None,
            noise_cancelling_level: None,
            battery: Some(Battery::Buds {
                left: Some(80),
                right: Some(75),
                case: None,
            }),
            power_off_failed: false,
        }
    }

    fn mode(mode: NoiseControlMode, label: &'static str, checked: bool) -> ModeItem {
        ModeItem {
            mode,
            label,
            checked,
            enabled: true,
        }
    }

    #[test]
    fn shows_the_attached_device_with_its_chosen_mode_checked() {
        let list = DeviceList {
            devices: vec![buds(DeviceState::Attached)],
            primary: Some(DeviceAddress("AA:BB:CC:DD:EE:FF".into())),
        };
        assert_eq!(
            TrayMenu::new(&list),
            TrayMenu {
                title: "Matt's Buds4 Pro".into(),
                battery: Some("Battery: L 80% · R 75% · Case –".into()),
                hint: None,
                status: None,
                device: Some(DeviceAddress("AA:BB:CC:DD:EE:FF".into())),
                modes: vec![
                    mode(NoiseControlMode::Off, "Off", false),
                    mode(NoiseControlMode::NoiseCancelling, "Noise Cancelling", false),
                    mode(NoiseControlMode::Ambient, "Ambient", true),
                    mode(NoiseControlMode::Adaptive, "Adaptive", false),
                ],
                power_off: None,
            }
        );
    }

    #[test]
    fn reads_no_device_while_the_only_device_is_not_attached() {
        let list = DeviceList {
            devices: vec![buds(DeviceState::Attaching)],
            primary: None,
        };
        assert_eq!(TrayMenu::new(&list).title, "No Device");
        assert_eq!(TrayMenu::new(&list).modes, vec![]);
    }

    #[test]
    fn says_why_a_device_is_not_attached() {
        for (reason, status) in [
            (
                NotAttachedReason::InUseByAnotherProcess,
                "In use by another app",
            ),
            (
                NotAttachedReason::BluetoothPermissionDenied,
                "Bluetooth permission denied: enable it in System Settings › Privacy & Security › Bluetooth",
            ),
        ] {
            let list = DeviceList {
                devices: vec![buds(DeviceState::NotAttached { reason })],
                primary: None,
            };
            assert_eq!(
                TrayMenu::new(&list),
                TrayMenu {
                    title: "Matt's Buds4 Pro".into(),
                    battery: None,
                    hint: None,
                    status: Some(status.into()),
                    device: None,
                    modes: vec![],
                    power_off: None,
                }
            );
        }
    }

    #[test]
    fn leaves_out_an_unsupported_model() {
        let list = DeviceList {
            devices: vec![buds(DeviceState::NotAttached {
                reason: NotAttachedReason::UnsupportedModel,
            })],
            primary: None,
        };
        assert_eq!(TrayMenu::new(&list).title, "No Device");
        assert_eq!(TrayMenu::new(&list).status, None);
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
                effective_mode: Some(NoiseControlMode::NoiseCancelling),
                effective_mode_reason: None,
                ambient_level: None,
                noise_cancelling_level: None,
                battery: Some(Battery::Headphones { level: 70 }),
                power_off_failed: false,
            }],
            primary: Some(DeviceAddress("11:22:33:44:55:66".into())),
        };
        assert_eq!(
            TrayMenu::new(&list),
            TrayMenu {
                title: "WH-1000XM5".into(),
                battery: Some("Battery: 70%".into()),
                hint: None,
                status: None,
                device: Some(DeviceAddress("11:22:33:44:55:66".into())),
                modes: vec![
                    mode(NoiseControlMode::Off, "Off", false),
                    mode(NoiseControlMode::NoiseCancelling, "Noise Cancelling", true),
                    mode(NoiseControlMode::Ambient, "Ambient", false),
                ],
                power_off: Some(PowerOffItem {
                    label: "Power Off",
                    enabled: true,
                }),
            }
        );
    }

    #[test]
    fn keeps_the_chosen_mode_checked_with_a_hint_while_a_bud_is_out() {
        let list = DeviceList {
            devices: vec![Device {
                effective_mode: Some(NoiseControlMode::Off),
                effective_mode_reason: Some(EffectiveModeReason::BudOutOfEar),
                ..buds(DeviceState::Attached)
            }],
            primary: Some(DeviceAddress("AA:BB:CC:DD:EE:FF".into())),
        };
        let menu = TrayMenu::new(&list);
        assert_eq!(
            menu.hint.as_deref(),
            Some("Paused: an earbud is out of the ear")
        );
        assert_eq!(
            menu.modes[2],
            mode(NoiseControlMode::Ambient, "Ambient", true)
        );
    }

    fn xm5() -> Device {
        Device {
            address: DeviceAddress("11:22:33:44:55:66".into()),
            name: "WH-1000XM5".into(),
            model: Some(Model::Wh1000Xm5),
            state: DeviceState::Attached,
            capabilities: Model::Wh1000Xm5.capabilities(),
            chosen_mode: Some(NoiseControlMode::NoiseCancelling),
            effective_mode: Some(NoiseControlMode::NoiseCancelling),
            effective_mode_reason: None,
            ambient_level: None,
            noise_cancelling_level: None,
            battery: Some(Battery::Headphones { level: 70 }),
            power_off_failed: false,
        }
    }

    #[test]
    fn shows_the_primary_device_when_two_are_attached() {
        let mut list = DeviceList {
            devices: vec![buds(DeviceState::Attached), xm5()],
            primary: Some(xm5().address),
        };
        assert_eq!(TrayMenu::new(&list).title, "WH-1000XM5");
        assert_eq!(TrayMenu::new(&list).device, Some(xm5().address));

        list.primary = Some(DeviceAddress("AA:BB:CC:DD:EE:FF".into()));
        assert_eq!(TrayMenu::new(&list).title, "Matt's Buds4 Pro");
    }

    #[test]
    fn offers_to_power_off_attached_headphones() {
        let list = DeviceList {
            devices: vec![xm5()],
            primary: Some(xm5().address),
        };
        assert_eq!(
            TrayMenu::new(&list).power_off,
            Some(PowerOffItem {
                label: "Power Off",
                enabled: true,
            })
        );
    }

    #[test]
    fn offers_no_power_off_for_headphones_that_are_not_attached() {
        let list = DeviceList {
            devices: vec![Device {
                state: DeviceState::Attaching,
                ..xm5()
            }],
            primary: None,
        };
        assert_eq!(TrayMenu::new(&list).power_off, None);
    }

    #[test]
    fn disables_everything_while_the_primary_device_is_powering_off() {
        let list = DeviceList {
            devices: vec![Device {
                state: DeviceState::PoweringOff,
                ..xm5()
            }],
            primary: Some(xm5().address),
        };
        let menu = TrayMenu::new(&list);
        assert_eq!(menu.title, "WH-1000XM5");
        assert_eq!(menu.battery.as_deref(), Some("Battery: 70%"));
        assert_eq!(
            menu.power_off,
            Some(PowerOffItem {
                label: "Powering Off…",
                enabled: false,
            })
        );
        assert!(menu.modes.iter().all(|item| !item.enabled));
    }

    #[test]
    fn offers_no_power_off_for_a_primary_device_that_is_not_attached() {
        for state in [
            DeviceState::Attaching,
            DeviceState::NotAttached {
                reason: NotAttachedReason::LinkDropped,
            },
        ] {
            // Reattaching, it keeps its Model and Capabilities.
            let list = DeviceList {
                devices: vec![Device {
                    state: state.clone(),
                    chosen_mode: None,
                    battery: None,
                    ..xm5()
                }],
                primary: Some(xm5().address),
            };
            let menu = TrayMenu::new(&list);
            assert_eq!(menu.power_off, None, "{state:?}");
            assert_eq!(menu.click("power-off"), None, "{state:?}");
        }
    }

    /// The menu's item ids in order, with `—` for a separator.
    fn layout(list: &DeviceList) -> Vec<String> {
        TrayMenu::new(list)
            .entries()
            .into_iter()
            .map(|entry| match entry {
                Entry::Separator => "—".into(),
                Entry::Info { id, .. } | Entry::Action { id, .. } => id.into(),
                Entry::Check { id, .. } => id,
            })
            .collect()
    }

    #[test]
    fn places_power_off_after_the_modes_and_before_open_and_quit() {
        let list = DeviceList {
            devices: vec![xm5()],
            primary: Some(xm5().address),
        };
        assert_eq!(
            layout(&list),
            [
                "title",
                "battery",
                "—",
                "mode:off",
                "mode:noiseCancelling",
                "mode:ambient",
                "—",
                "power-off",
                "—",
                "open",
                "quit",
            ]
        );
    }

    #[test]
    fn has_no_power_off_separator_without_the_capability() {
        let list = DeviceList {
            devices: vec![buds(DeviceState::Attached)],
            primary: Some(buds(DeviceState::Attached).address),
        };
        assert_eq!(
            layout(&list),
            [
                "title",
                "battery",
                "—",
                "mode:off",
                "mode:noiseCancelling",
                "mode:ambient",
                "mode:adaptive",
                "—",
                "open",
                "quit",
            ]
        );
    }

    #[test]
    fn powers_off_the_primary_device_when_clicked() {
        let list = DeviceList {
            devices: vec![buds(DeviceState::Attached), xm5()],
            primary: Some(xm5().address),
        };
        assert_eq!(
            TrayMenu::new(&list).click("power-off"),
            Some(Click::Send(Command::PowerOff(xm5().address)))
        );
    }

    #[test]
    fn sets_the_primary_devices_mode_when_clicked() {
        let list = DeviceList {
            devices: vec![xm5()],
            primary: Some(xm5().address),
        };
        assert_eq!(
            TrayMenu::new(&list).click("mode:ambient"),
            Some(Click::Send(Command::SetNoiseControlMode(
                xm5().address,
                NoiseControlMode::Ambient
            )))
        );
    }

    #[test]
    fn opens_the_window_and_quits_without_a_device() {
        let menu = TrayMenu::new(&DeviceList::default());
        assert_eq!(menu.click("open"), Some(Click::Open));
        assert_eq!(menu.click("quit"), Some(Click::Quit));
        assert_eq!(menu.click("power-off"), None);
    }
}
