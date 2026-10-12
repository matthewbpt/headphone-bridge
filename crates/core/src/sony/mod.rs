//! The Sony MDR v2 protocol (pure, no I/O), for the WH-1000XM5. Ported from
//! libmdr (MIT).
//!
//! Every data frame carries a 1-bit sequence number and is acknowledged by the
//! other side with an ACK whose sequence number is the opposite bit. The app
//! keeps one request outstanding at a time and sends the next only once the
//! Device has acknowledged it. A request that isn't acknowledged is resent,
//! up to [`MAX_SENDS`] times in all, as libmdr does.

pub(crate) mod frame;

use std::collections::VecDeque;

use uuid::{uuid, Uuid};

use crate::command::{CommandError, Equalizer, NoiseControlMode};
use crate::device_list::Battery;
use crate::model::{Model, WH_1000XM5_BANDS};
pub use crate::update::Update;
pub use frame::Frame;
use frame::{encode, DataType, Deframer};

/// The vendor's v2 RFCOMM service.
pub const SERVICE: Uuid = uuid!("956c7b26-d49a-4ba8-b03f-b17d393cb6e2");

/// Table 1 command ids: payload byte 0.
mod command {
    pub const CONNECT_GET_PROTOCOL_INFO: u8 = 0x00;
    pub const CONNECT_RET_PROTOCOL_INFO: u8 = 0x01;
    pub const CONNECT_GET_DEVICE_INFO: u8 = 0x04;
    pub const CONNECT_RET_DEVICE_INFO: u8 = 0x05;
    pub const CONNECT_GET_SUPPORT_FUNCTION: u8 = 0x06;
    pub const CONNECT_RET_SUPPORT_FUNCTION: u8 = 0x07;
    pub const POWER_GET_STATUS: u8 = 0x22;
    pub const POWER_RET_STATUS: u8 = 0x23;
    pub const POWER_SET_STATUS: u8 = 0x24;
    pub const POWER_NTFY_STATUS: u8 = 0x25;
    pub const NCASM_GET_PARAM: u8 = 0x66;
    pub const NCASM_RET_PARAM: u8 = 0x67;
    pub const NCASM_SET_PARAM: u8 = 0x68;
    pub const NCASM_NTFY_PARAM: u8 = 0x69;
    pub const EQEBB_GET_STATUS: u8 = 0x52;
    pub const EQEBB_RET_STATUS: u8 = 0x53;
    pub const EQEBB_NTFY_STATUS: u8 = 0x55;
    pub const EQEBB_GET_PARAM: u8 = 0x56;
    pub const EQEBB_RET_PARAM: u8 = 0x57;
    pub const EQEBB_SET_PARAM: u8 = 0x58;
    pub const EQEBB_NTFY_PARAM: u8 = 0x59;
}

/// `ConnectInquiredType::FIXED_VALUE`.
const FIXED_VALUE: u8 = 0x00;
/// `DeviceInfoType::MODEL_NAME`.
const MODEL_NAME: u8 = 0x01;

/// `PowerInquiredType`s for a single battery.
mod power {
    pub const BATTERY: u8 = 0x00;
    pub const BATTERY_WITH_THRESHOLD: u8 = 0x08;
    pub const POWER_OFF: u8 = 0x03;
}

/// `FunctionType`s in the support function list.
mod function {
    pub const BATTERY_LEVEL_INDICATOR: u8 = 0x20;
    pub const BATTERY_LEVEL_WITH_THRESHOLD: u8 = 0x28;
}

/// How many times a request is sent before the Device is given up on.
pub const MAX_SENDS: u32 = 10;

/// `NcAsmInquiredType` for the WH-1000XM5: noise cancelling and ambient sound
/// with a level, as its support function `0x6B` declares (libmdr).
const NCASM_TYPE: u8 = 0x17;
/// `ValueChangeStatus::CHANGED`: a set that is final, not a slider mid-drag.
const CHANGED: u8 = 0x01;

/// `EqEbbInquiredType::PRESET_EQ`.
const PRESET_EQ: u8 = 0x00;
/// `EqEbbStatus::DISABLE`: the Device refuses Equalizer changes for now.
const EQUALIZER_DISABLE: u8 = 0x01;
/// How far an Equalizer step is offset on the wire: a step of −10 is sent
/// as 0.
const EQUALIZER_STEP_OFFSET: i8 = 10;
/// How many steps an Equalizer curve has on the wire: Clear Bass, then each
/// Band.
const CURVE_STEPS: usize = 1 + WH_1000XM5_BANDS.len();

/// NCASM params as the Device reports and takes them, bytes 3–6 of a 0x17
/// RET, NTFY or SET.
#[derive(Debug, Clone, Copy)]
struct NoiseControl {
    /// `00` off, `01` on: Off when off, whatever the mode byte says.
    effect: u8,
    /// `00` noise cancelling, `01` ambient.
    mode: u8,
    /// Not exposed by the app, so always sent back as last reported.
    focus_on_voice: u8,
    level: u8,
}

/// What the Device acknowledging a request tells the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestKind {
    /// Only that the next request can go.
    Plain,
    /// That the Device has taken the Power Off.
    PowerOff,
}

/// A request waiting for its ACK.
#[derive(Debug)]
struct InFlight {
    frame: Vec<u8>,
    /// How many times it has been sent.
    sends: u32,
    kind: RequestKind,
}

/// One control link session with a Sony Device.
///
/// Attaching only reads: it asks for the protocol info, then the model name,
/// and only once the Model is on the allowlist for its support functions and
/// battery. Nothing that changes a setting is sent before identification.
#[derive(Debug)]
pub struct Session {
    deframer: Deframer,
    /// The sequence number of the next data frame the app sends.
    seq: u8,
    /// The request sent and not yet acknowledged, if any.
    in_flight: Option<InFlight>,
    /// Data frames sent so far, resends included.
    sends: u64,
    /// Data frame payloads waiting for the outstanding one to be acknowledged.
    requests: VecDeque<(Vec<u8>, RequestKind)>,
    outgoing: Vec<Vec<u8>>,
    identified: bool,
    /// The NCASM params last reported, or since sent. Every set changes one
    /// field of these and sends the rest back as they were. `None` until the
    /// Device first reports them.
    noise_control: Option<NoiseControl>,
    /// The Equalizer last reported, or since sent. Bands and Clear Bass are
    /// sent together, so a set giving one keeps the other from here. `None`
    /// until the Device first reports it.
    equalizer: Option<Equalizer>,
}

impl Session {
    pub fn new() -> Session {
        let mut session = Session {
            deframer: Deframer::default(),
            seq: 0,
            in_flight: None,
            sends: 0,
            requests: VecDeque::new(),
            outgoing: Vec::new(),
            identified: false,
            noise_control: None,
            equalizer: None,
        };
        session.request(vec![command::CONNECT_GET_PROTOCOL_INFO, FIXED_VALUE]);
        session
    }

    /// Takes the frames to write to the Device, in order.
    pub fn outgoing(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outgoing)
    }

    /// While a request is waiting for its ACK, a number that changes each
    /// time it is sent again or another request goes out. `None` while
    /// nothing is waiting.
    pub fn awaiting_ack(&self) -> Option<u64> {
        self.in_flight.as_ref().map(|_| self.sends)
    }

    /// Sends the request that is waiting for its ACK again. Returns `false`,
    /// sending nothing, once it has been sent [`MAX_SENDS`] times.
    pub fn resend(&mut self) -> bool {
        let Some(in_flight) = &mut self.in_flight else {
            return true;
        };
        if in_flight.sends >= MAX_SENDS {
            return false;
        }
        in_flight.sends += 1;
        self.sends += 1;
        self.outgoing.push(in_flight.frame.clone());
        true
    }

    /// Asks the Device to switch to `mode`. The Device confirms with a
    /// notification of the new params. Does nothing until the Device has
    /// reported its params, as there is nothing to keep unchanged yet.
    pub fn set_noise_control_mode(&mut self, mode: NoiseControlMode) {
        let Some(mut params) = self.noise_control else {
            return;
        };
        (params.effect, params.mode) = match mode {
            NoiseControlMode::Off => (0x00, params.mode),
            NoiseControlMode::NoiseCancelling => (0x01, 0x00),
            NoiseControlMode::Ambient => (0x01, 0x01),
            NoiseControlMode::Adaptive => return,
        };
        self.set_noise_control(params);
    }

    /// Asks the Device to set the Ambient Level, in the Model's own steps.
    /// Like a mode set, it waits for the Device's params first.
    pub fn set_ambient_level(&mut self, level: u8) {
        let Some(mut params) = self.noise_control else {
            return;
        };
        params.level = level;
        self.set_noise_control(params);
    }

    /// Asks the Device to set its Equalizer. A Preset alone leaves its curve
    /// to the Device. Bands and Clear Bass are sent together, so one given
    /// alone keeps the other as last reported or sent; with none to keep,
    /// it is rejected as not reported and nothing is sent. The Device
    /// confirms with a notification of the new params.
    pub fn set_equalizer(&mut self, equalizer: &Equalizer) -> Result<(), CommandError> {
        let curve = match (&equalizer.bands, equalizer.clear_bass) {
            (None, None) => None,
            (bands, clear_bass) => {
                let kept = self.equalizer.as_ref();
                let bands = bands.clone().or_else(|| kept?.bands.clone());
                let clear_bass = clear_bass.or_else(|| kept?.clear_bass);
                let (Some(bands), Some(clear_bass)) = (bands, clear_bass) else {
                    return Err(CommandError::SettingsNotReported);
                };
                Some((bands, clear_bass))
            }
        };
        let mut payload = vec![command::EQEBB_SET_PARAM, PRESET_EQ, equalizer.preset];
        match &curve {
            Some((bands, clear_bass)) => {
                payload.push(1 + bands.len() as u8);
                payload.extend(
                    std::iter::once(*clear_bass)
                        .chain(bands.iter().copied())
                        .map(|step| (step + EQUALIZER_STEP_OFFSET) as u8),
                );
            }
            None => payload.push(0),
        }
        if let Some((bands, clear_bass)) = curve {
            self.equalizer = Some(Equalizer {
                preset: equalizer.preset,
                bands: Some(bands),
                clear_bass: Some(clear_bass),
            });
        }
        self.request(payload);
        Ok(())
    }

    /// Asks the Device to Power Off, as libmdr and Gadgetbridge do.
    pub fn power_off(&mut self) {
        /// `PowerOffSettingValue::ON`.
        const ON: u8 = 0x01;
        self.request_as(
            vec![command::POWER_SET_STATUS, power::POWER_OFF, ON],
            RequestKind::PowerOff,
        );
    }

    fn set_noise_control(&mut self, params: NoiseControl) {
        self.noise_control = Some(params);
        self.request(vec![
            command::NCASM_SET_PARAM,
            NCASM_TYPE,
            CHANGED,
            params.effect,
            params.mode,
            params.focus_on_voice,
            params.level,
        ]);
    }

    /// Feeds bytes received from the Device, in chunks of any size, and
    /// returns the whole frames they complete.
    pub fn deframe(&mut self, bytes: &[u8]) -> Vec<Frame> {
        self.deframer.push(bytes)
    }

    /// Handles a frame received from the Device. Any frames to send in
    /// response, such as its ACK, are then in [`Session::outgoing`].
    pub fn receive(&mut self, frame: &Frame) -> Vec<Update> {
        match frame.data_type {
            DataType::Ack => return self.acknowledged(frame),
            DataType::Mdr | DataType::MdrNo2 => {
                self.outgoing
                    .push(encode(DataType::Ack, 1 - (frame.seq & 1), &[]));
                if frame.data_type == DataType::Mdr {
                    return self.message(&frame.data);
                }
            }
            DataType::Other(_) => {}
        }
        Vec::new()
    }

    fn acknowledged(&mut self, ack: &Frame) -> Vec<Update> {
        // A stale ACK, for a frame other than the outstanding one, is ignored.
        let Some(in_flight) = self.in_flight.take_if(|_| ack.seq == 1 - self.seq) else {
            return Vec::new();
        };
        self.seq = ack.seq;
        self.send_next();
        match in_flight.kind {
            RequestKind::PowerOff => vec![Update::PowerOffAcknowledged],
            RequestKind::Plain => Vec::new(),
        }
    }

    /// Handles a Table 1 message.
    fn message(&mut self, data: &[u8]) -> Vec<Update> {
        match *data {
            [command::CONNECT_RET_PROTOCOL_INFO, ..] if !self.identified => {
                self.request(vec![command::CONNECT_GET_DEVICE_INFO, MODEL_NAME]);
                Vec::new()
            }
            [command::CONNECT_RET_DEVICE_INFO, MODEL_NAME, len, ref name @ ..]
                if !self.identified =>
            {
                let Some(name) = name.get(..len as usize) else {
                    return Vec::new();
                };
                let Some(model) = model_for_name(name) else {
                    return vec![Update::Unsupported];
                };
                self.identified = true;
                self.request(vec![command::CONNECT_GET_SUPPORT_FUNCTION, FIXED_VALUE]);
                vec![Update::Identified(model)]
            }
            [command::CONNECT_RET_SUPPORT_FUNCTION, _, count, ref functions @ ..]
                if self.identified =>
            {
                let functions: Vec<u8> = functions
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .take(count as usize)
                    .map(|[function, _]| *function)
                    .collect();
                let has = |function| functions.contains(&function);
                let battery = if !has(function::BATTERY_LEVEL_INDICATOR)
                    && has(function::BATTERY_LEVEL_WITH_THRESHOLD)
                {
                    power::BATTERY_WITH_THRESHOLD
                } else {
                    power::BATTERY
                };
                self.request(vec![command::POWER_GET_STATUS, battery]);
                Vec::new()
            }
            [command::POWER_RET_STATUS | command::POWER_NTFY_STATUS, power::BATTERY | power::BATTERY_WITH_THRESHOLD, level, ..]
                if self.identified && level <= 100 =>
            {
                if data[0] == command::POWER_RET_STATUS {
                    self.request(vec![command::NCASM_GET_PARAM, NCASM_TYPE]);
                }
                vec![Update::Battery(Battery::Headphones { level })]
            }
            [command::NCASM_RET_PARAM | command::NCASM_NTFY_PARAM, NCASM_TYPE, _status, effect, mode, focus_on_voice, level]
                if self.identified =>
            {
                self.noise_control = Some(NoiseControl {
                    effect,
                    mode,
                    focus_on_voice,
                    level,
                });
                let mode = match (effect, mode) {
                    (0x00, _) => NoiseControlMode::Off,
                    (_, 0x00) => NoiseControlMode::NoiseCancelling,
                    _ => NoiseControlMode::Ambient,
                };
                if data[0] == command::NCASM_RET_PARAM {
                    self.request(vec![command::EQEBB_GET_STATUS, PRESET_EQ]);
                }
                let mut updates = vec![Update::ChosenMode(mode)];
                let range = Model::Wh1000Xm5.capabilities().ambient_level;
                if range.is_some_and(|range| range.contains(level)) {
                    updates.push(Update::AmbientLevel(level));
                }
                updates
            }
            [command::EQEBB_RET_STATUS | command::EQEBB_NTFY_STATUS, PRESET_EQ, status]
                if self.identified =>
            {
                if data[0] == command::EQEBB_RET_STATUS {
                    self.request(vec![command::EQEBB_GET_PARAM, PRESET_EQ]);
                }
                vec![Update::EqualizerUnavailable(status == EQUALIZER_DISABLE)]
            }
            [command::EQEBB_RET_PARAM | command::EQEBB_NTFY_PARAM, PRESET_EQ, preset, n, ref steps @ ..]
                if self.identified && steps.len() == n as usize =>
            {
                let equalizer = equalizer(preset, steps);
                self.equalizer = Some(equalizer.clone());
                vec![Update::Equalizer(equalizer)]
            }
            _ => Vec::new(),
        }
    }

    fn request(&mut self, payload: Vec<u8>) {
        self.request_as(payload, RequestKind::Plain);
    }

    fn request_as(&mut self, payload: Vec<u8>, kind: RequestKind) {
        self.requests.push_back((payload, kind));
        self.send_next();
    }

    fn send_next(&mut self) {
        if self.in_flight.is_some() {
            return;
        }
        if let Some((payload, kind)) = self.requests.pop_front() {
            let frame = encode(DataType::Mdr, self.seq, &payload);
            self.outgoing.push(frame.clone());
            self.sends += 1;
            self.in_flight = Some(InFlight {
                frame,
                sends: 1,
                kind,
            });
        }
    }
}

/// The Equalizer the Device reports as `preset` and the `steps` that follow
/// it. A whole curve is Clear Bass then each Band. Any other count is kept
/// as the Preset alone.
fn equalizer(preset: u8, steps: &[u8]) -> Equalizer {
    let steps: Option<Vec<i8>> = match steps.len() {
        CURVE_STEPS => steps
            .iter()
            .map(|&step| {
                i8::try_from(step)
                    .ok()
                    .map(|step| step - EQUALIZER_STEP_OFFSET)
            })
            .collect(),
        _ => None,
    };
    match steps {
        Some(steps) => Equalizer {
            preset,
            clear_bass: Some(steps[0]),
            bands: Some(steps[1..].to_vec()),
        },
        None => Equalizer {
            preset,
            bands: None,
            clear_bass: None,
        },
    }
}

fn model_for_name(name: &[u8]) -> Option<Model> {
    (name == b"WH-1000XM5").then_some(Model::Wh1000Xm5)
}
