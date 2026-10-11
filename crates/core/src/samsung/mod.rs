//! The Samsung Galaxy Buds protocol (pure, no I/O), starting with the
//! Galaxy Buds4 Pro. Ported from omarchy-buds (MIT) and checked against its
//! Buds4 Pro captures (firmware revision 4).

mod frame;

use uuid::{uuid, Uuid};

use crate::device_list::Battery;
use crate::model::Model;
pub use crate::update::Update;
pub use frame::Frame;
use frame::{encode, Deframer};

/// The vendor's RFCOMM service.
pub const SERVICE: Uuid = uuid!("2e73a4ad-332d-41fc-90e2-16bef06523f2");

mod id {
    pub const STATUS_UPDATED: u8 = 96;
    pub const EXTENDED_STATUS_UPDATED: u8 = 97;
    pub const MANAGER_INFO: u8 = 136;
}

/// The Model the OS-reported identity points to, before Attaching. Only these
/// Devices are Attached, and so sent the handshake (ADR-0003).
pub fn candidate_model(name: &str) -> Option<Model> {
    name.to_lowercase()
        .contains("buds4 pro")
        .then_some(Model::GalaxyBuds4Pro)
}

/// One control link session with a Samsung Device.
#[derive(Debug, Default)]
pub struct Session {
    deframer: Deframer,
    identified: bool,
    buds: Buds,
}

impl Session {
    /// The frames to send when the link opens: the `MANAGER_INFO` handshake,
    /// introducing the app the way Samsung's own app does. The Device answers
    /// with an extended status, which identifies the Model.
    pub fn handshake() -> Vec<u8> {
        encode(id::MANAGER_INFO, &[0x01, 0x01, 0x22])
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
            }
            id::STATUS_UPDATED if p.len() >= 8 => {
                self.buds.apply(p[5], p[7], [p[1], p[2], p[6]]);
            }
            _ => return Vec::new(),
        }
        if self.identified {
            updates.push(Update::Battery(self.buds.battery()));
        }
        updates
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
