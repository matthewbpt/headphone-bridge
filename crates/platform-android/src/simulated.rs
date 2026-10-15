//! The Simulated Device, in debug builds only: a Buds4 Pro or an XM5 that
//! answers the core in-process, for the emulator, which can't reach real
//! headphones. It starts from the frames the core's tests replay, and
//! confirms mode, Level and Equalizer changes the way the real Device does.

use std::sync::{Arc, Mutex};

use headphone_bridge_core::frame_log::{Direction, FrameRecord};
use headphone_bridge_core::platform::{ControlLink, DeviceAddress, DeviceIdentity};
use headphone_bridge_core::wire::{samsung, sony};
use headphone_bridge_core::Model;
use tokio::sync::mpsc;

/// Where each Model is simulated: locally administered addresses, so never a
/// real Device's.
fn address(model: Model) -> DeviceAddress {
    DeviceAddress(
        match model {
            Model::GalaxyBuds4Pro => "02:00:5E:00:00:01",
            Model::Wh1000Xm5 => "02:00:5E:00:00:02",
        }
        .into(),
    )
}

/// A Simulated Device, switched on: its settings last for as long as it's
/// Connected, across control links, as a real Device's do.
pub(crate) struct SimulatedDevice {
    model: Model,
    device: Arc<Mutex<Box<dyn Simulated>>>,
}

impl SimulatedDevice {
    pub(crate) fn new(model: Model) -> SimulatedDevice {
        let device: Box<dyn Simulated> = match model {
            Model::GalaxyBuds4Pro => Box::new(Buds::new()),
            Model::Wh1000Xm5 => Box::new(Headphones::new()),
        };
        SimulatedDevice {
            model,
            device: Arc::new(Mutex::new(device)),
        }
    }

    pub(crate) fn model(&self) -> Model {
        self.model
    }

    /// What the OS would report for it once Connected.
    pub(crate) fn identity(&self) -> DeviceIdentity {
        let service = match self.model {
            Model::GalaxyBuds4Pro => samsung::SERVICE,
            Model::Wh1000Xm5 => sony::SERVICE,
        };
        DeviceIdentity {
            address: address(self.model),
            name: format!("{} (Simulated)", self.model.display_name()),
            service_uuids: vec![service],
        }
    }

    /// Whether it's at `asked`.
    pub(crate) fn is_at(&self, asked: &DeviceAddress) -> bool {
        address(self.model) == *asked
    }

    /// Opens a control link to it, which it answers until the core drops the
    /// link, or until it's asked to Power Off: it then closes the link and
    /// calls `power_off`.
    pub(crate) fn open(&self, power_off: impl FnOnce() + Send + 'static) -> ControlLink {
        let (to_core, incoming) = mpsc::unbounded_channel();
        let (outgoing, mut from_core) = mpsc::unbounded_channel::<Vec<u8>>();
        let device = self.device.clone();
        device.lock().unwrap().link_opened();
        tokio::spawn(async move {
            while let Some(bytes) = from_core.recv().await {
                let (replies, powered_off) = {
                    let mut device = device.lock().unwrap();
                    (device.receive(&bytes), device.powered_off())
                };
                for reply in replies {
                    if to_core.send(reply).is_err() {
                        return;
                    }
                }
                if powered_off {
                    return power_off();
                }
            }
        });
        ControlLink { incoming, outgoing }
    }
}

/// The Device's end of a control link.
trait Simulated: Send {
    /// Starts afresh on a new control link, keeping its settings.
    fn link_opened(&mut self);

    /// Takes bytes from the core; returns the frames to answer with.
    fn receive(&mut self, bytes: &[u8]) -> Vec<Vec<u8>>;

    /// Whether it has been asked to Power Off.
    fn powered_off(&self) -> bool {
        false
    }
}

/// The frames a Device sent in a fixture, deframed with `deframe`.
fn received<F>(fixture: &str, mut deframe: impl FnMut(&[u8]) -> Vec<F>) -> Vec<F> {
    FrameRecord::parse_all(fixture)
        .expect("the fixture parses")
        .iter()
        .filter(|record| record.direction == Direction::Received)
        .flat_map(|record| deframe(&record.bytes))
        .collect()
}

/// A Galaxy Buds4 Pro, both buds in the ear.
struct Buds {
    deframer: samsung::Deframer,
    /// The extended status payload it answers the handshake with, kept up to
    /// date with every set.
    status: Vec<u8>,
}

mod samsung_id {
    pub const ACK: u8 = 66;
    pub const EXTENDED_STATUS_UPDATED: u8 = 97;
    pub const NOISE_CONTROLS: u8 = 120;
    pub const NOISE_REDUCTION_LEVEL: u8 = 131;
    pub const AMBIENT_VOLUME: u8 = 132;
    pub const EQUALIZER: u8 = 134;
    pub const MANAGER_INFO: u8 = 136;
}

/// Where the extended status reports each setting.
mod samsung_status {
    pub const PLACEMENT: usize = 6;
    pub const EQUALIZER: usize = 9;
    pub const MODE: usize = 12;
    pub const AMBIENT_LEVEL: usize = 23;
    pub const NOISE_CANCELLING_LEVEL: usize = 24;
}

impl Buds {
    fn new() -> Buds {
        // capture-3's first extended status: L 100 %, R 97 %, Ambient.
        let mut deframer = samsung::Deframer::default();
        let mut status = received(
            include_str!("../../core/tests/fixtures/galaxy-buds4-pro/capture-3.frames"),
            |bytes| deframer.push(bytes),
        )
        .into_iter()
        .find(|frame| frame.id == samsung_id::EXTENDED_STATUS_UPDATED)
        .expect("capture-3 has an extended status")
        .payload;
        // Both buds in the ear, so the Effective Mode is the Chosen Mode.
        status[samsung_status::PLACEMENT] = 0x11;
        Buds {
            deframer: samsung::Deframer::default(),
            status,
        }
    }
}

impl Simulated for Buds {
    fn link_opened(&mut self) {
        self.deframer = samsung::Deframer::default();
    }

    fn receive(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut replies = Vec::new();
        for frame in self.deframer.push(bytes) {
            let value = frame.payload.first().copied();
            let setting = match frame.id {
                samsung_id::MANAGER_INFO => {
                    replies.push(samsung::encode(
                        samsung_id::EXTENDED_STATUS_UPDATED,
                        &self.status,
                    ));
                    continue;
                }
                samsung_id::NOISE_CONTROLS => samsung_status::MODE,
                samsung_id::AMBIENT_VOLUME => samsung_status::AMBIENT_LEVEL,
                samsung_id::NOISE_REDUCTION_LEVEL => samsung_status::NOISE_CANCELLING_LEVEL,
                samsung_id::EQUALIZER => samsung_status::EQUALIZER,
                _ => continue,
            };
            let Some(value) = value else { continue };
            self.status[setting] = value;
            // A Buds4 Pro confirms a set with an ACK carrying the applied
            // value (real-noise-control, real-nc-level-set).
            replies.push(samsung::encode(samsung_id::ACK, &[frame.id, value]));
        }
        replies
    }
}

/// Table 1 command ids: payload byte 0.
mod sony_command {
    pub const CONNECT_GET_PROTOCOL_INFO: u8 = 0x00;
    pub const CONNECT_GET_DEVICE_INFO: u8 = 0x04;
    pub const CONNECT_GET_SUPPORT_FUNCTION: u8 = 0x06;
    pub const POWER_GET_STATUS: u8 = 0x22;
    pub const POWER_SET_STATUS: u8 = 0x24;
    pub const NCASM_GET_PARAM: u8 = 0x66;
    pub const NCASM_RET_PARAM: u8 = 0x67;
    pub const NCASM_SET_PARAM: u8 = 0x68;
    pub const NCASM_NTFY_PARAM: u8 = 0x69;
    pub const EQEBB_GET_STATUS: u8 = 0x52;
    pub const EQEBB_GET_PARAM: u8 = 0x56;
    pub const EQEBB_RET_PARAM: u8 = 0x57;
    pub const EQEBB_SET_PARAM: u8 = 0x58;
    pub const EQEBB_NTFY_PARAM: u8 = 0x59;
}

/// `EqEbbInquiredType::PRESET_EQ`.
const PRESET_EQ: u8 = 0x00;

/// A WH-1000XM5. It acknowledges every request, answers the Attach's
/// requests as in attach.frames, and confirms a noise control set with a
/// notification of the new params. A Preset picked is notified with its
/// curve, as real-equalizer-presets reports each fixed Preset's. A curve edit
/// is only acknowledged unless it moves onto another Preset, as
/// real-equalizer-curve shows.
struct Headphones {
    deframer: sony::Deframer,
    /// The sequence number of its next data frame.
    seq: u8,
    /// attach.frames' answer to each request the core sends to Attach, by
    /// command id.
    answers: Vec<(u8, Vec<u8>)>,
    /// The noise control params: NCASM type, status, effect, mode, Focus on
    /// Voice and Ambient Level, as a RET or NTFY carries them after the id.
    noise_control: Vec<u8>,
    /// The Equalizer's Preset.
    preset: u8,
    /// Each Preset's curve, as a RET or NTFY carries it after the Preset: the
    /// step count, then Clear Bass and each Band, each + 10.
    curves: Vec<(u8, Vec<u8>)>,
    powered_off: bool,
}

impl Headphones {
    fn new() -> Headphones {
        let mut deframer = sony::Deframer::default();
        let records = FrameRecord::parse_all(include_str!(
            "../../core/tests/fixtures/wh-1000xm5/attach.frames"
        ))
        .expect("attach.frames parses");
        // Each of the core's requests, paired with the Device's next message.
        let mut answers = Vec::new();
        let mut asked = None;
        for record in records {
            for frame in deframer.push(&record.bytes) {
                if frame.data_type != sony::DataType::Mdr {
                    continue;
                }
                match record.direction {
                    Direction::Sent => asked = frame.data.first().copied(),
                    Direction::Received => {
                        if let Some(command) = asked.take() {
                            answers.push((command, frame.data));
                        }
                    }
                }
            }
        }
        let noise_control = answers
            .iter()
            .find(|(command, _)| *command == sony_command::NCASM_GET_PARAM)
            .map(|(_, answer)| answer[1..].to_vec())
            .expect("attach.frames reports noise control");
        let reported = answers
            .iter()
            .find(|(command, _)| *command == sony_command::EQEBB_GET_PARAM)
            .map(|(_, answer)| answer.clone())
            .expect("attach.frames reports the Equalizer");
        let mut deframer = sony::Deframer::default();
        let mut curves: Vec<(u8, Vec<u8>)> = received(
            include_str!("../../core/tests/fixtures/wh-1000xm5/real-equalizer-presets.frames"),
            |bytes| deframer.push(bytes),
        )
        .into_iter()
        .filter(|frame| frame.data.first() == Some(&sony_command::EQEBB_NTFY_PARAM))
        .map(|frame| (frame.data[2], frame.data[3..].to_vec()))
        .collect();
        curves.insert(0, (reported[2], reported[3..].to_vec()));
        Headphones {
            deframer: sony::Deframer::default(),
            seq: 0,
            answers,
            noise_control,
            preset: reported[2],
            curves,
            powered_off: false,
        }
    }

    fn message(&mut self, data: &[u8]) -> Vec<u8> {
        let frame = sony::encode(sony::DataType::Mdr, self.seq, data);
        self.seq = 1 - self.seq;
        frame
    }

    /// The noise control params after `id`.
    fn noise_control(&self, id: u8) -> Vec<u8> {
        [&[id][..], &self.noise_control].concat()
    }

    /// The Equalizer params after `id`: the Preset and its curve, flat for a
    /// Preset with none yet.
    fn equalizer(&self, id: u8) -> Vec<u8> {
        let flat = [6, 10, 10, 10, 10, 10, 10];
        let curve = self
            .curves
            .iter()
            .find(|(preset, _)| *preset == self.preset)
            .map_or(&flat[..], |(_, curve)| curve);
        [&[id, PRESET_EQ, self.preset][..], curve].concat()
    }
}

impl Simulated for Headphones {
    fn link_opened(&mut self) {
        self.deframer = sony::Deframer::default();
        self.seq = 0;
    }

    fn receive(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut replies = Vec::new();
        for frame in self.deframer.push(bytes) {
            if frame.data_type != sony::DataType::Mdr {
                continue;
            }
            replies.push(sony::encode(sony::DataType::Ack, 1 - (frame.seq & 1), &[]));
            let answer = match frame.data.first().copied() {
                Some(sony_command::NCASM_GET_PARAM) => {
                    self.noise_control(sony_command::NCASM_RET_PARAM)
                }
                // Type, status, effect, mode, Focus on Voice and level.
                Some(sony_command::NCASM_SET_PARAM) if frame.data.len() == 7 => {
                    self.noise_control = frame.data[1..].to_vec();
                    self.noise_control(sony_command::NCASM_NTFY_PARAM)
                }
                Some(sony_command::EQEBB_GET_PARAM) => {
                    self.equalizer(sony_command::EQEBB_RET_PARAM)
                }
                // The Preset, then its curve, or none to keep its own.
                Some(sony_command::EQEBB_SET_PARAM) if frame.data.len() >= 4 => {
                    let preset = frame.data[2];
                    let moved = preset != self.preset;
                    self.preset = preset;
                    let curve = &frame.data[3..];
                    if curve != [0] {
                        self.curves.retain(|(of, _)| *of != preset);
                        self.curves.insert(0, (preset, curve.to_vec()));
                        if !moved {
                            continue;
                        }
                    }
                    self.equalizer(sony_command::EQEBB_NTFY_PARAM)
                }
                // `POWER_OFF`, `ON`: only acknowledged, then it switches off.
                Some(sony_command::POWER_SET_STATUS) if frame.data[1..] == [0x03, 0x01] => {
                    self.powered_off = true;
                    continue;
                }
                Some(
                    command @ (sony_command::CONNECT_GET_PROTOCOL_INFO
                    | sony_command::CONNECT_GET_DEVICE_INFO
                    | sony_command::CONNECT_GET_SUPPORT_FUNCTION
                    | sony_command::POWER_GET_STATUS
                    | sony_command::EQEBB_GET_STATUS),
                ) => match self.answers.iter().find(|(asked, _)| *asked == command) {
                    Some((_, answer)) => answer.clone(),
                    None => continue,
                },
                _ => continue,
            };
            let reply = self.message(&answer);
            replies.push(reply);
        }
        replies
    }

    fn powered_off(&self) -> bool {
        self.powered_off
    }
}
