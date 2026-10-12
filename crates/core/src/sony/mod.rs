//! The Sony MDR v2 protocol (pure, no I/O), for the WH-1000XM5. Ported from
//! libmdr (MIT).
//!
//! Every data frame carries a 1-bit sequence number and is acknowledged by the
//! other side with an ACK whose sequence number is the opposite bit. The app
//! keeps one request outstanding at a time and sends the next only once the
//! Device has acknowledged it. A request that isn't acknowledged is resent,
//! up to [`MAX_SENDS`] times in all, as libmdr does.

mod frame;

use std::collections::VecDeque;

use uuid::{uuid, Uuid};

use crate::command::NoiseControlMode;
use crate::device_list::Battery;
use crate::model::Model;
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
    pub const POWER_NTFY_STATUS: u8 = 0x25;
    pub const NCASM_GET_PARAM: u8 = 0x66;
    pub const NCASM_RET_PARAM: u8 = 0x67;
    pub const NCASM_SET_PARAM: u8 = 0x68;
    pub const NCASM_NTFY_PARAM: u8 = 0x69;
}

/// `ConnectInquiredType::FIXED_VALUE`.
const FIXED_VALUE: u8 = 0x00;
/// `DeviceInfoType::MODEL_NAME`.
const MODEL_NAME: u8 = 0x01;

/// `PowerInquiredType`s for a single battery.
mod power {
    pub const BATTERY: u8 = 0x00;
    pub const BATTERY_WITH_THRESHOLD: u8 = 0x08;
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

/// A request waiting for its ACK.
#[derive(Debug)]
struct InFlight {
    frame: Vec<u8>,
    /// How many times it has been sent.
    sends: u32,
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
    requests: VecDeque<Vec<u8>>,
    outgoing: Vec<Vec<u8>>,
    identified: bool,
    /// The NCASM params last reported, or since sent. Every set changes one
    /// field of these and sends the rest back as they were. `None` until the
    /// Device first reports them.
    noise_control: Option<NoiseControl>,
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
            DataType::Ack => self.acknowledged(frame),
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

    fn acknowledged(&mut self, ack: &Frame) {
        // A stale ACK, for a frame other than the outstanding one, is ignored.
        if self.in_flight.is_none() || ack.seq != 1 - self.seq {
            return;
        }
        self.seq = ack.seq;
        self.in_flight = None;
        self.send_next();
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
                let mut updates = vec![Update::ChosenMode(mode)];
                let range = Model::Wh1000Xm5.capabilities().ambient_level;
                if range.is_some_and(|range| range.contains(level)) {
                    updates.push(Update::AmbientLevel(level));
                }
                updates
            }
            _ => Vec::new(),
        }
    }

    fn request(&mut self, payload: Vec<u8>) {
        self.requests.push_back(payload);
        self.send_next();
    }

    fn send_next(&mut self) {
        if self.in_flight.is_some() {
            return;
        }
        if let Some(payload) = self.requests.pop_front() {
            let frame = encode(DataType::Mdr, self.seq, &payload);
            self.outgoing.push(frame.clone());
            self.sends += 1;
            self.in_flight = Some(InFlight { frame, sends: 1 });
        }
    }
}

fn model_for_name(name: &[u8]) -> Option<Model> {
    (name == b"WH-1000XM5").then_some(Model::Wh1000Xm5)
}
