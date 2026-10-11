//! The Samsung Galaxy Buds protocol (pure, no I/O), starting with the
//! Galaxy Buds4 Pro. Ported from omarchy-buds (MIT) and checked against its
//! Buds4 Pro captures (firmware revision 4).

mod frame;

use uuid::{uuid, Uuid};

use crate::command::NoiseControlMode;
use crate::device_list::Battery;
use crate::model::Model;
use frame::{encode, Deframer};

/// The vendor's RFCOMM service.
pub const SERVICE: Uuid = uuid!("2e73a4ad-332d-41fc-90e2-16bef06523f2");

mod id {
    pub const STATUS_UPDATED: u8 = 96;
    pub const EXTENDED_STATUS_UPDATED: u8 = 97;
    pub const NOISE_CONTROLS_UPDATE: u8 = 119;
    pub const NOISE_CONTROLS: u8 = 120;
    pub const MANAGER_INFO: u8 = 136;
}

/// The Model the OS-reported identity points to, before Attaching. Only these
/// Devices are Attached, and so sent the handshake (ADR-0003).
pub fn candidate_model(name: &str) -> Option<Model> {
    name.to_lowercase()
        .contains("buds4 pro")
        .then_some(Model::GalaxyBuds4Pro)
}

/// What a control link session learned from the Device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    Identified(Model),
    /// The Device isn't a supported Model: the session must end without
    /// writing anything else.
    Unsupported,
    Battery(Battery),
    ChosenMode(NoiseControlMode),
}

/// One control link session with a Samsung Device.
#[derive(Debug, Default)]
pub struct Session {
    deframer: Deframer,
    identified: bool,
    buds: Buds,
    /// The mode the app last asked for, until the Device confirms it.
    pending_mode: Option<NoiseControlMode>,
}

impl Session {
    /// The frames to send when the link opens: the `MANAGER_INFO` handshake,
    /// introducing the app the way Samsung's own app does. The Device answers
    /// with an extended status, which identifies the Model.
    pub fn handshake() -> Vec<u8> {
        encode(id::MANAGER_INFO, &[0x01, 0x01, 0x22])
    }

    /// The frames that ask the Device to switch to `mode`, one per write. On
    /// a Buds4 Pro the ACK is only a receipt, and the confirming
    /// `NOISE_CONTROLS_UPDATE` may not come until the app next sends
    /// `MANAGER_INFO` (capture-2), so a set is followed by one as a poll.
    pub fn set_noise_control_mode(&mut self, mode: NoiseControlMode) -> Vec<Vec<u8>> {
        self.pending_mode = Some(mode);
        vec![
            encode(id::NOISE_CONTROLS, &[noise_control_value(mode)]),
            Self::handshake(),
        ]
    }

    fn confirm(&mut self, mode: NoiseControlMode) {
        if self.pending_mode == Some(mode) {
            self.pending_mode = None;
        }
    }

    /// Feeds bytes received from the Device, in chunks of any size.
    pub fn receive(&mut self, bytes: &[u8]) -> Vec<Update> {
        let mut updates = Vec::new();
        for frame in self.deframer.push(bytes) {
            let p = &frame.payload;
            match frame.id {
                id::EXTENDED_STATUS_UPDATED if p.len() > 17 => {
                    if !self.identified {
                        let device_id = |at: usize| u16::from_le_bytes([p[at], p[at + 1]]);
                        match [device_id(14), device_id(16)]
                            .into_iter()
                            .find_map(model_for_device_id)
                        {
                            Some(model) => {
                                self.identified = true;
                                updates.push(Update::Identified(model));
                            }
                            None => return vec![Update::Unsupported],
                        }
                    }
                    let charging = p.get(42).copied().unwrap_or(0);
                    self.buds.apply(p[6], charging, [p[2], p[3], p[7]]);
                    if let Some(mode) = noise_control_mode(p[12]) {
                        self.confirm(mode);
                        updates.push(Update::ChosenMode(mode));
                    }
                }
                id::NOISE_CONTROLS_UPDATE if self.identified && !p.is_empty() => {
                    let Some(mode) = noise_control_mode(p[0]) else {
                        continue;
                    };
                    // Byte 6 is the cause: 1 for a gesture, which chooses a
                    // mode. Other causes are placement changes, whose mode is
                    // only temporary, unless it is the one the app asked for:
                    // the Device confirms a set with such a cause too. Older
                    // Models send only the mode.
                    let gesture = p.len() < 7 || p[6] == 0x01;
                    if gesture || self.pending_mode == Some(mode) {
                        self.confirm(mode);
                        updates.push(Update::ChosenMode(mode));
                    }
                    continue;
                }
                id::STATUS_UPDATED if p.len() >= 8 => {
                    self.buds.apply(p[5], p[7], [p[1], p[2], p[6]]);
                }
                _ => continue,
            }
            if self.identified {
                updates.push(Update::Battery(self.buds.battery()));
            }
        }
        updates
    }
}

/// The Noise Control Mode a Device reports or is sent as `value`.
fn noise_control_mode(value: u8) -> Option<NoiseControlMode> {
    match value {
        0 => Some(NoiseControlMode::Off),
        1 => Some(NoiseControlMode::NoiseCancelling),
        2 => Some(NoiseControlMode::Ambient),
        3 => Some(NoiseControlMode::Adaptive),
        _ => None,
    }
}

fn noise_control_value(mode: NoiseControlMode) -> u8 {
    match mode {
        NoiseControlMode::Off => 0,
        NoiseControlMode::NoiseCancelling => 1,
        NoiseControlMode::Ambient => 2,
        NoiseControlMode::Adaptive => 3,
    }
}

/// Device ids are per colour: 359 Black, 360 White, 361 Apricot.
fn model_for_device_id(device_id: u16) -> Option<Model> {
    (359..=361)
        .contains(&device_id)
        .then_some(Model::GalaxyBuds4Pro)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Placement {
    #[default]
    Disconnected,
    Wearing,
    Idle,
    Case,
    CaseClosed,
}

impl Placement {
    fn from_nibble(nibble: u8) -> Placement {
        match nibble {
            1 => Placement::Wearing,
            2 => Placement::Idle,
            3 => Placement::Case,
            4 => Placement::CaseClosed,
            _ => Placement::Disconnected,
        }
    }
}

/// Battery bookkeeping across status messages.
#[derive(Debug, Default)]
struct Buds {
    left: Option<u8>,
    right: Option<u8>,
    /// The last case level read while a bud was docked.
    last_live_case: Option<u8>,
    docked: bool,
}

impl Buds {
    /// Applies placement nibbles (L high, R low), charging bits (0x10 L,
    /// 0x04 R) and raw levels for left, right and case.
    fn apply(&mut self, placement: u8, charging: u8, [left, right, case]: [u8; 3]) {
        let mut left_at = Placement::from_nibble(placement >> 4);
        let mut right_at = Placement::from_nibble(placement & 0x0F);
        // The charging bit flips before the placement nibble when a bud drops
        // into the case.
        for (at, charging) in [
            (&mut left_at, charging & 0x10 != 0),
            (&mut right_at, charging & 0x04 != 0),
        ] {
            if charging && matches!(at, Placement::Idle | Placement::Disconnected) {
                *at = Placement::Case;
            }
        }
        let level =
            |raw: u8, at: Placement| (at != Placement::Disconnected).then_some(raw.min(100));
        self.left = level(left, left_at);
        self.right = level(right, right_at);
        // The buds read the case over the charging contacts, so the case byte
        // is a measurement only while a bud is docked.
        self.docked = [left_at, right_at]
            .iter()
            .any(|at| matches!(at, Placement::Case | Placement::CaseClosed));
        if self.docked && case <= 100 {
            self.last_live_case = Some(case);
        }
    }

    fn battery(&self) -> Battery {
        Battery {
            left: self.left,
            right: self.right,
            case: if self.docked {
                self.last_live_case
            } else {
                None
            },
        }
    }
}
