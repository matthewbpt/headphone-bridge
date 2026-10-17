//! The Samsung Galaxy Buds protocol (pure, no I/O), for the Galaxy Buds4 Pro
//! and Galaxy Buds2. Ported from omarchy-buds (MIT) and checked against its
//! Buds4 Pro captures (firmware revision 4). The Buds2 uses the same messages
//! at the same offsets, after GalaxyBudsClient's Buds2 definitions; no real
//! Buds2 capture exists yet.

pub(crate) mod frame;

use uuid::{uuid, Uuid};

use crate::command::NoiseControlMode;
use crate::device_list::{Battery, EffectiveModeReason};
use crate::model::Model;
pub use crate::update::Update;
pub use frame::Frame;
use frame::{encode, Deframer};

/// The vendor's RFCOMM service.
pub const SERVICE: Uuid = uuid!("2e73a4ad-332d-41fc-90e2-16bef06523f2");

mod id {
    pub const ACK: u8 = 66;
    pub const STATUS_UPDATED: u8 = 96;
    pub const EXTENDED_STATUS_UPDATED: u8 = 97;
    pub const NOISE_CONTROLS_UPDATE: u8 = 119;
    pub const NOISE_CONTROLS: u8 = 120;
    pub const NOISE_REDUCTION_LEVEL: u8 = 131;
    pub const AMBIENT_VOLUME: u8 = 132;
    pub const MANAGER_INFO: u8 = 136;
}

/// The Model the OS-reported identity points to, before Attaching. Only these
/// Devices are Attached, and so sent the handshake (ADR-0003).
pub fn candidate_model(name: &str) -> Option<Model> {
    let name = name.to_lowercase();
    if name.contains("buds4 pro") {
        Some(Model::GalaxyBuds4Pro)
    } else if name.contains("buds2") && !name.contains("buds2 pro") {
        Some(Model::GalaxyBuds2)
    } else {
        None
    }
}

/// One control link session with a Samsung Device.
#[derive(Debug, Default)]
pub struct Session {
    deframer: Deframer,
    /// The Model the Device's name points to: the only one its device id
    /// may confirm.
    candidate: Option<Model>,
    /// `None` until the extended status identifies the Model.
    model: Option<Model>,
    buds: Buds,
    /// The mode the app last asked for, until the Device confirms it.
    pending_mode: Option<NoiseControlMode>,
    /// The Ambient Level the app last asked for, until the Device confirms it.
    pending_ambient_level: Option<u8>,
    /// The Noise Cancelling Level the app last asked for, until the Device
    /// confirms it. Kept apart from the Ambient Level's, so that sets of the
    /// two can't confirm each other.
    pending_noise_cancelling_level: Option<u8>,
    /// Whether a Chosen Mode has been reported yet.
    chosen_known: bool,
}

impl Session {
    /// A session with a Device whose name points to `candidate`.
    pub fn new(candidate: Option<Model>) -> Session {
        Session {
            candidate,
            ..Session::default()
        }
    }

    /// The frames to send when the link opens: the `MANAGER_INFO` handshake,
    /// introducing the app the way Samsung's own app does. The Device answers
    /// with an extended status, which identifies the Model. That a Buds2
    /// answers it too is a hypothesis until a real one is captured.
    pub fn handshake() -> Vec<u8> {
        encode(id::MANAGER_INFO, &[0x01, 0x01, 0x22])
    }

    /// The frames that ask the Device to switch to `mode`, one per write. A
    /// Buds4 Pro confirms with an ACK carrying the applied mode
    /// (real-noise-control). In capture-2 it sent no ACK, and confirmed with
    /// a `NOISE_CONTROLS_UPDATE` only after the next `MANAGER_INFO`, so a set
    /// is followed by one as a poll. That a Buds2's ACKs also carry the
    /// applied value is a hypothesis; if not, the poll's extended status
    /// confirms the set.
    pub fn set_noise_control_mode(&mut self, mode: NoiseControlMode) -> Vec<Vec<u8>> {
        self.pending_mode = Some(mode);
        vec![
            encode(id::NOISE_CONTROLS, &[noise_control_value(mode)]),
            Self::handshake(),
        ]
    }

    /// The frames that ask the Device to set the Ambient Level, in the
    /// Model's own steps. A Buds4 Pro confirms with an ACK carrying the
    /// applied level, one per set even when sets arrive in quick succession
    /// (frame log, 2026-10-11). Like a mode set it is followed by
    /// `MANAGER_INFO`, whose extended status also reports the level.
    pub fn set_ambient_level(&mut self, level: u8) -> Vec<Vec<u8>> {
        self.pending_ambient_level = Some(level);
        vec![encode(id::AMBIENT_VOLUME, &[level]), Self::handshake()]
    }

    /// The frames that ask the Device to set the Noise Cancelling Level, in
    /// the Model's own steps. A hypothesis by analogy with the Ambient Level
    /// set, until confirmed on hardware: GalaxyBudsClient sends this message
    /// to the original Buds Pro, and treats its ACK as `83 <level>`.
    pub fn set_noise_cancelling_level(&mut self, level: u8) -> Vec<Vec<u8>> {
        self.pending_noise_cancelling_level = Some(level);
        vec![
            encode(id::NOISE_REDUCTION_LEVEL, &[level]),
            Self::handshake(),
        ]
    }

    /// The update for a mode the Device reports applying while the buds are
    /// at `placement`: the Chosen Mode if `chosen` or the app asked for it,
    /// otherwise the Effective Mode alone.
    fn mode_update(&mut self, mode: NoiseControlMode, placement: u8, chosen: bool) -> Update {
        if chosen || self.pending_mode == Some(mode) {
            if self.pending_mode == Some(mode) {
                self.pending_mode = None;
            }
            self.chosen_known = true;
            return Update::ChosenMode(mode);
        }
        let reason = (!both_worn(placement)).then_some(EffectiveModeReason::BudOutOfEar);
        Update::EffectiveMode(mode, reason)
    }

    /// The Ambient Level a Device reports as `value`, if it is in the
    /// identified Model's range.
    fn ambient_level(&self, value: Option<&u8>) -> Option<Update> {
        let range = self.model?.capabilities().ambient_level?;
        let level = *value?;
        range.contains(level).then_some(Update::AmbientLevel(level))
    }

    /// The Noise Cancelling Level a Device reports as `value`, if it is in
    /// the identified Model's range.
    fn noise_cancelling_level(&self, value: Option<&u8>) -> Option<Update> {
        let range = self.model?.capabilities().noise_cancelling_level?;
        let level = *value?;
        range
            .contains(level)
            .then_some(Update::NoiseCancellingLevel(level))
    }

    /// Feeds bytes received from the Device, in chunks of any size, and
    /// returns the whole frames they complete.
    pub fn deframe(&mut self, bytes: &[u8]) -> Vec<Frame> {
        self.deframer.push(bytes)
    }

    /// Handles a frame received from the Device.
    pub fn receive(&mut self, frame: &Frame) -> Vec<Update> {
        let p = &frame.payload;
        let mut updates = Vec::new();
        match frame.id {
            id::EXTENDED_STATUS_UPDATED if p.len() > 17 => {
                if self.model.is_none() {
                    let device_id = |at: usize| u16::from_le_bytes([p[at], p[at + 1]]);
                    // The device id, not the name, decides the Model, and it
                    // must be the one the name pointed to (ADR-0003).
                    match [device_id(14), device_id(16)]
                        .into_iter()
                        .find_map(model_for_device_id)
                        .filter(|model| Some(*model) == self.candidate)
                    {
                        Some(model) => {
                            self.model = Some(model);
                            updates.push(Update::Identified(model));
                        }
                        None => return vec![Update::Unsupported],
                    }
                }
                let charging = match self.model {
                    // A Buds2 reports its charging bits at byte 36, and only
                    // from revision 10 (byte 0); before that they're unknown
                    // and placement alone docks a bud (GalaxyBudsClient).
                    Some(Model::GalaxyBuds2) => p.get(36).filter(|_| p[0] >= 10),
                    _ => p.get(42),
                }
                .copied()
                .unwrap_or(0);
                self.buds.apply(p[6], charging, [p[2], p[3], p[7]]);
                // Byte 12 is the mode the Device is applying: Off while it has
                // paused the Chosen Mode because a bud is out (capture-1c).
                // Any other mode, or Off with both buds in, is the Chosen
                // Mode, as is the first one reported, for want of a better
                // guess.
                if let Some(mode) = noise_control_mode(p[12]) {
                    let chosen =
                        !self.chosen_known || mode != NoiseControlMode::Off || both_worn(p[6]);
                    updates.push(self.mode_update(mode, p[6], chosen));
                }
                updates.extend(self.ambient_level(p.get(23)));
                // Byte 24, whatever the mode (real-nc-level-0 to 4).
                updates.extend(self.noise_cancelling_level(p.get(24)));
            }
            id::AMBIENT_VOLUME => return self.ambient_level(p.first()).into_iter().collect(),
            // An ACK is `<id of the message> <value>`. It confirms a set only
            // when the value is the one the app asked for: other firmware may
            // send a result code there, and then the ACK is only a receipt.
            id::ACK if self.model.is_some() && p.len() >= 2 => {
                return match p[0] {
                    id::NOISE_CONTROLS => match noise_control_mode(p[1]) {
                        Some(mode) if self.pending_mode == Some(mode) => {
                            vec![self.mode_update(mode, 0x11, true)]
                        }
                        _ => Vec::new(),
                    },
                    id::AMBIENT_VOLUME if self.pending_ambient_level == Some(p[1]) => {
                        self.pending_ambient_level = None;
                        self.ambient_level(p.get(1)).into_iter().collect()
                    }
                    id::NOISE_REDUCTION_LEVEL
                        if self.pending_noise_cancelling_level == Some(p[1]) =>
                    {
                        self.pending_noise_cancelling_level = None;
                        self.noise_cancelling_level(p.get(1)).into_iter().collect()
                    }
                    _ => Vec::new(),
                };
            }
            id::NOISE_CONTROLS_UPDATE if self.model.is_some() && !p.is_empty() => {
                let Some(mode) = noise_control_mode(p[0]) else {
                    return Vec::new();
                };
                // Byte 1 is the placement and byte 6 the cause: 1 for a
                // gesture, which chooses a mode. Other causes are placement
                // changes, whose mode is only the Effective Mode, unless it is
                // the one the app asked for: the Device confirms a set with
                // such a cause too. Older Models send only the mode.
                let gesture = p.len() < 7 || p[6] == 0x01;
                let placement = p.get(1).copied().unwrap_or(0x11);
                return vec![self.mode_update(mode, placement, gesture)];
            }
            // Older firmware sends 7 bytes, without the charging byte.
            id::STATUS_UPDATED if p.len() >= 7 => {
                let charging = p.get(7).copied().unwrap_or(0);
                self.buds.apply(p[5], charging, [p[1], p[2], p[6]]);
            }
            _ => return Vec::new(),
        }
        if self.model.is_some() {
            updates.push(Update::Battery(self.buds.battery()));
        }
        updates
    }
}

/// The Noise Control Mode a Device reports as `value`.
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

/// Device ids are per colour. Buds4 Pro: 359 Black, 360 White, 361 Apricot.
/// Buds2: 313 White to 321 Grey, as GalaxyBudsClient lists them; its 14337,
/// an unknown Buds2, isn't accepted until a real Device reports it.
fn model_for_device_id(device_id: u16) -> Option<Model> {
    match device_id {
        359..=361 => Some(Model::GalaxyBuds4Pro),
        313..=321 => Some(Model::GalaxyBuds2),
        _ => None,
    }
}

/// Whether placement nibbles (L high, R low) put both buds in the ear.
fn both_worn(placement: u8) -> bool {
    [placement >> 4, placement & 0x0F]
        .into_iter()
        .all(|nibble| Placement::from_nibble(nibble) == Placement::Wearing)
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
        Battery::Buds {
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
