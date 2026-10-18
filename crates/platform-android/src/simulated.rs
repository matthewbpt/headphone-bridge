//! The Simulated Device, in debug builds only: a Buds4 Pro, a Buds2 or an XM5 that
//! answers the core in-process, for the emulator, which can't reach real
//! headphones. It starts from the frames the core's tests replay, and
//! confirms mode, Level and Equalizer changes the way the real Device does.

use std::sync::{Arc, Mutex, PoisonError};

use headphone_bridge_core::frame_log::{Direction, FrameRecord};
use headphone_bridge_core::platform::{ControlLink, DeviceAddress, DeviceIdentity};
use headphone_bridge_core::wire::{samsung, sony};
use headphone_bridge_core::Model;
use headphone_bridge_fixtures as fixtures;
use tokio::sync::mpsc;

/// Where each Model is simulated: locally administered addresses, so never a
/// real Device's.
const fn address(model: Model) -> DeviceAddress {
    match model {
        Model::GalaxyBuds4Pro => BUDS4PRO,
        Model::Wh1000Xm5 => XM5,
        Model::GalaxyBuds2 => BUDS2,
    }
}

const BUDS4PRO: DeviceAddress = DeviceAddress::from_octets([0x02, 0x00, 0x5E, 0x00, 0x00, 0x01]);
const XM5: DeviceAddress = DeviceAddress::from_octets([0x02, 0x00, 0x5E, 0x00, 0x00, 0x02]);
const BUDS2: DeviceAddress = DeviceAddress::from_octets([0x02, 0x00, 0x5E, 0x00, 0x00, 0x03]);

/// A Simulated Device, switched on: its settings last for as long as it's
/// Connected, across control links, as a real Device's do.
pub(crate) struct SimulatedDevice {
    model: Model,
    device: Arc<Mutex<Box<dyn Simulated>>>,
}

impl SimulatedDevice {
    /// `None` if the Model's fixture doesn't decode, which its tests rule out.
    pub(crate) fn new(model: Model) -> Option<SimulatedDevice> {
        let device: Box<dyn Simulated> = match model {
            Model::GalaxyBuds4Pro => Box::new(Buds::buds4pro()?),
            Model::GalaxyBuds2 => Box::new(Buds::buds2()?),
            Model::Wh1000Xm5 => Box::new(Headphones::new()?),
        };
        Some(SimulatedDevice {
            model,
            device: Arc::new(Mutex::new(device)),
        })
    }

    pub(crate) fn model(&self) -> Model {
        self.model
    }

    /// What the OS would report for it once Connected.
    pub(crate) fn identity(&self) -> DeviceIdentity {
        let service = match self.model {
            Model::GalaxyBuds4Pro | Model::GalaxyBuds2 => samsung::SERVICE,
            Model::Wh1000Xm5 => sony::SERVICE,
        };
        DeviceIdentity {
            address: address(self.model),
            name: format!("{} (Simulated)", self.model.display_name()),
            service_uuids: vec![service],
        }
    }

    /// Whether it's at `asked`.
    pub(crate) fn is_at(&self, asked: DeviceAddress) -> bool {
        address(self.model) == asked
    }

    /// Opens a control link to it, which it answers until the core drops the
    /// link, or until it's asked to Power Off: it then closes the link and
    /// calls `power_off`.
    pub(crate) fn open(&self, power_off: impl FnOnce() + Send + 'static) -> ControlLink {
        let (to_core, incoming) = mpsc::unbounded_channel();
        let (outgoing, mut from_core) = mpsc::unbounded_channel::<Vec<u8>>();
        let device = self.device.clone();
        device
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .link_opened();
        tokio::spawn(async move {
            while let Some(bytes) = from_core.recv().await {
                let (replies, powered_off) = {
                    let mut device = device.lock().unwrap_or_else(PoisonError::into_inner);
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

/// The frames a Device sent in a fixture, deframed with `deframe`, or `None`
/// if the fixture doesn't parse.
fn received<F>(fixture: &str, mut deframe: impl FnMut(&[u8]) -> Vec<F>) -> Option<Vec<F>> {
    let records = FrameRecord::parse_all(fixture).ok()?;
    Some(
        records
            .iter()
            .filter(|record| record.direction == Direction::Received)
            .flat_map(|record| deframe(&record.bytes))
            .collect(),
    )
}

/// A Galaxy Buds4 Pro or Buds2, both buds in the ear.
struct Buds {
    deframer: samsung::Deframer,
    /// The extended status payload it answers the handshake with, kept up to
    /// date with every set.
    status: Vec<u8>,
}

impl Buds {
    /// A Buds4 Pro from capture-3's first extended status: L 100 %, R 97 %,
    /// Ambient.
    fn buds4pro() -> Option<Buds> {
        Buds::new(fixtures::BUDS4PRO_CAPTURE)
    }

    /// A Buds2 from the extended status in GalaxyBudsClient's tests: device
    /// id 316, L and R 100 %, Noise Cancelling, Ambient Level 1. A Buds2's
    /// sets reach the same bytes as a Buds4 Pro's.
    fn buds2() -> Option<Buds> {
        Buds::new(fixtures::BUDS2_ATTACH)
    }

    /// Starts from the first extended status the Device sent in `fixture`.
    fn new(fixture: &str) -> Option<Buds> {
        let mut deframer = samsung::Deframer::default();
        let mut status = received(fixture, |bytes| deframer.push(bytes))?
            .into_iter()
            .find(|frame| frame.id == samsung::id::EXTENDED_STATUS_UPDATED)?
            .payload;
        // Both buds in the ear, so the Effective Mode is the Chosen Mode.
        *status.get_mut(samsung::extended_status::PLACEMENT)? = samsung::BOTH_WORN;
        Some(Buds {
            deframer: samsung::Deframer::default(),
            status,
        })
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
                samsung::id::MANAGER_INFO => {
                    replies.push(samsung::encode(
                        samsung::id::EXTENDED_STATUS_UPDATED,
                        &self.status,
                    ));
                    continue;
                }
                samsung::id::NOISE_CONTROLS => samsung::extended_status::NOISE_CONTROL_MODE,
                samsung::id::AMBIENT_VOLUME => samsung::extended_status::AMBIENT_LEVEL,
                samsung::id::NOISE_REDUCTION_LEVEL => {
                    samsung::extended_status::NOISE_CANCELLING_LEVEL
                }
                samsung::id::EQUALIZER => samsung::extended_status::EQUALIZER,
                _ => continue,
            };
            let Some(value) = value else { continue };
            self.status[setting] = value;
            // A Buds4 Pro and a Buds2 confirm a set with an ACK carrying the
            // applied value (real-noise-control, real-nc-level-set, and
            // galaxy-buds2's real-noise-control and real-ambient-level).
            replies.push(samsung::encode(samsung::id::ACK, &[frame.id, value]));
        }
        replies
    }
}

/// A WH-1000XM5. It acknowledges every request, answers the Attach's
/// requests as in attach.frames, and confirms a noise control set with a
/// notification of the new params. A Preset picked is notified with its
/// curve, as real-equalizer-presets reports each fixed Preset's. A curve edit
/// is only acknowledged unless it moves onto another Preset, as
/// real-equalizer-curve shows.
struct Headphones {
    deframer: sony::Deframer,
    /// The sequence number of its next data frame.
    seq: sony::Sequence,
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
    fn new() -> Option<Headphones> {
        let mut deframer = sony::Deframer::default();
        let records = FrameRecord::parse_all(fixtures::XM5_ATTACH).ok()?;
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
            .find(|(command, _)| *command == sony::command::NCASM_GET_PARAM)
            .and_then(|(_, answer)| answer.get(1..))?
            .to_vec();
        let reported = answers
            .iter()
            .find(|(command, _)| *command == sony::command::EQEBB_GET_PARAM)
            .map(|(_, answer)| answer)?;
        let preset = *reported.get(2)?;
        let mut deframer = sony::Deframer::default();
        let mut curves: Vec<(u8, Vec<u8>)> = received(fixtures::XM5_EQUALIZER_PRESETS, |bytes| {
            deframer.push(bytes)
        })?
        .into_iter()
        .filter(|frame| frame.data.first() == Some(&sony::command::EQEBB_NTFY_PARAM))
        .filter_map(|frame| Some((*frame.data.get(2)?, frame.data.get(3..)?.to_vec())))
        .collect();
        curves.insert(0, (preset, reported.get(3..)?.to_vec()));
        Some(Headphones {
            deframer: sony::Deframer::default(),
            seq: sony::Sequence::FIRST,
            answers,
            noise_control,
            preset,
            curves,
            powered_off: false,
        })
    }

    fn message(&mut self, data: &[u8]) -> Vec<u8> {
        let frame = sony::encode(sony::DataType::Mdr, self.seq, data);
        self.seq = self.seq.flipped();
        frame
    }

    /// The noise control params after `id`.
    fn noise_control(&self, id: u8) -> Vec<u8> {
        [&[id][..], &self.noise_control].concat()
    }

    /// The Equalizer params after `id`: the Preset and its curve, flat for a
    /// Preset with none yet.
    fn equalizer(&self, id: u8) -> Vec<u8> {
        let curve = self
            .curves
            .iter()
            .find(|(preset, _)| *preset == self.preset)
            .map_or(&sony::FLAT_CURVE[..], |(_, curve)| curve);
        [&[id, sony::PRESET_EQ, self.preset][..], curve].concat()
    }
}

impl Simulated for Headphones {
    fn link_opened(&mut self) {
        self.deframer = sony::Deframer::default();
        self.seq = sony::Sequence::FIRST;
    }

    fn receive(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut replies = Vec::new();
        for frame in self.deframer.push(bytes) {
            if frame.data_type != sony::DataType::Mdr {
                continue;
            }
            let ack = sony::Sequence::from_byte(frame.seq).ack();
            replies.push(sony::encode(sony::DataType::Ack, ack, &[]));
            let answer = match frame.data.first().copied() {
                Some(sony::command::NCASM_GET_PARAM) => {
                    self.noise_control(sony::command::NCASM_RET_PARAM)
                }
                // Type, status, effect, mode, Focus on Voice and level.
                Some(sony::command::NCASM_SET_PARAM)
                    if frame.data.len() == sony::NOISE_CONTROL_SET_LEN =>
                {
                    self.noise_control = frame.data[1..].to_vec();
                    self.noise_control(sony::command::NCASM_NTFY_PARAM)
                }
                Some(sony::command::EQEBB_GET_PARAM) => {
                    self.equalizer(sony::command::EQEBB_RET_PARAM)
                }
                // The Preset, then its curve, or none to keep its own.
                Some(sony::command::EQEBB_SET_PARAM) if frame.data.len() >= 4 => {
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
                    self.equalizer(sony::command::EQEBB_NTFY_PARAM)
                }
                // Only acknowledged, then it switches off.
                Some(sony::command::POWER_SET_STATUS) if frame.data == sony::POWER_OFF => {
                    self.powered_off = true;
                    continue;
                }
                Some(
                    command @ (sony::command::CONNECT_GET_PROTOCOL_INFO
                    | sony::command::CONNECT_GET_DEVICE_INFO
                    | sony::command::CONNECT_GET_SUPPORT_FUNCTION
                    | sony::command::POWER_GET_STATUS
                    | sony::command::EQEBB_GET_STATUS),
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
