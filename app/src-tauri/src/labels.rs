//! Every label the tray and the window show for a Device's state and
//! settings. The window gets them from the `labels` command, so the two
//! can't drift apart.

use std::collections::HashMap;

use headphone_bridge_core::platform::LinkFailure;
use headphone_bridge_core::{EffectiveModeReason, Model, NoiseControlMode, NotAttachedReason};
use serde::Serialize;

pub const NO_DEVICE: &str = "No Device";

#[cfg_attr(mobile, expect(dead_code, reason = "only the tray uses it"))]
pub const POWER_OFF: &str = "Power Off";
#[cfg_attr(mobile, expect(dead_code, reason = "only the tray uses it"))]
pub const POWERING_OFF: &str = "Powering Off…";

pub fn mode(mode: NoiseControlMode) -> &'static str {
    match mode {
        NoiseControlMode::Off => "Off",
        NoiseControlMode::NoiseCancelling => "Noise Cancelling",
        NoiseControlMode::Ambient => "Ambient",
        NoiseControlMode::Adaptive => "Adaptive",
    }
}

/// Why the Device isn't applying the Chosen Mode.
pub fn hint(reason: EffectiveModeReason) -> &'static str {
    match reason {
        EffectiveModeReason::BudOutOfEar => "Paused: an earbud is out of the ear",
        EffectiveModeReason::Unknown => "Paused by the Device",
    }
}

/// Why a Device isn't Attached. A link failure without a cause has one
/// label; the window shows its detail beneath it.
pub fn reason(reason: &NotAttachedReason) -> &'static str {
    match reason {
        NotAttachedReason::UnsupportedModel => "Unsupported Model",
        NotAttachedReason::LinkDropped => "Another app or device may be using it",
        NotAttachedReason::NoResponse => "The Device didn't respond",
        NotAttachedReason::LinkFailed(failure) => match failure {
            LinkFailure::InUseByAnotherProcess => "In use by another app",
            LinkFailure::BluetoothPermissionDenied => {
                "Bluetooth permission denied: enable it in System Settings › Privacy & Security › Bluetooth"
            }
            LinkFailure::ServiceNotFound => "Control service not found",
            LinkFailure::DeviceNotConnected => "Not Connected",
            LinkFailure::Other(_) => "Couldn't open the control link",
        },
    }
}

const MODES: [NoiseControlMode; 4] = [
    NoiseControlMode::Off,
    NoiseControlMode::NoiseCancelling,
    NoiseControlMode::Ambient,
    NoiseControlMode::Adaptive,
];

const MODELS: [Model; 3] = [Model::GalaxyBuds4Pro, Model::GalaxyBuds2, Model::Wh1000Xm5];

const HINTS: [EffectiveModeReason; 2] = [
    EffectiveModeReason::BudOutOfEar,
    EffectiveModeReason::Unknown,
];

/// Every reason, one per kind: `Other`'s label doesn't depend on its detail.
const REASONS: [NotAttachedReason; 8] = [
    NotAttachedReason::UnsupportedModel,
    NotAttachedReason::LinkFailed(LinkFailure::InUseByAnotherProcess),
    NotAttachedReason::LinkFailed(LinkFailure::BluetoothPermissionDenied),
    NotAttachedReason::LinkFailed(LinkFailure::ServiceNotFound),
    NotAttachedReason::LinkFailed(LinkFailure::DeviceNotConnected),
    NotAttachedReason::LinkDropped,
    NotAttachedReason::NoResponse,
    NotAttachedReason::LinkFailed(LinkFailure::Other(String::new())),
];

/// The labels, as the window gets them.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Labels {
    no_device: &'static str,
    modes: HashMap<NoiseControlMode, &'static str>,
    models: HashMap<Model, &'static str>,
    hints: HashMap<EffectiveModeReason, &'static str>,
    reasons: Vec<ReasonLabel>,
}

#[derive(Debug, Serialize)]
struct ReasonLabel {
    reason: NotAttachedReason,
    text: &'static str,
}

pub fn all() -> Labels {
    Labels {
        no_device: NO_DEVICE,
        modes: MODES.into_iter().map(|m| (m, mode(m))).collect(),
        models: MODELS.into_iter().map(|m| (m, m.display_name())).collect(),
        hints: HINTS.into_iter().map(|r| (r, hint(r))).collect(),
        reasons: REASONS
            .into_iter()
            .map(|r| ReasonLabel {
                text: reason(&r),
                reason: r,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::to_value;

    use super::*;

    #[test]
    fn labels_every_reason_the_window_can_get() {
        let labels = to_value(all()).unwrap();
        let reasons = labels["reasons"].as_array().unwrap();

        let kinds: Vec<_> = reasons
            .iter()
            .map(|label| label["reason"]["kind"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            [
                "unsupportedModel",
                "inUseByAnotherProcess",
                "bluetoothPermissionDenied",
                "serviceNotFound",
                "deviceNotConnected",
                "linkDropped",
                "noResponse",
                "other",
            ]
        );
        for label in reasons {
            assert_ne!(label["text"], "", "{} has no label", label["reason"]);
        }
    }

    #[test]
    fn labels_a_link_failure_without_a_cause_the_same_whatever_its_detail() {
        let reason = NotAttachedReason::LinkFailed(LinkFailure::Other(
            "IOBluetooth error 0xE00002BC".into(),
        ));
        assert_eq!(super::reason(&reason), "Couldn't open the control link");
    }
}
