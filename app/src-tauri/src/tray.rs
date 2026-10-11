//! The native tray menu. It shows the Primary Device's name and battery, its
//! Noise Control Mode as radio items, "Open Headphone Bridge" and "Quit".
//!
//! Tray actions only use menu items, never tray click events, because Linux
//! doesn't emit them.

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{Battery, DeviceList, DeviceState, NoiseControlMode};

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
    format!(
        "Battery: L {} · R {} · Case {}",
        percent(battery.left),
        percent(battery.right),
        percent(battery.case)
    )
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
            battery: Some(Battery {
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
}
