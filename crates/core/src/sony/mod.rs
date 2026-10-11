//! The Sony MDR v2 protocol (pure, no I/O), for the WH-1000XM5. Ported from
//! libmdr (MIT).
//!
//! Every data frame carries a 1-bit sequence number and is acknowledged by the
//! other side with an ACK whose sequence number is the opposite bit. The app
//! keeps one request outstanding at a time and sends the next only once the
//! Device has acknowledged it.

mod frame;

use std::collections::VecDeque;

use uuid::{uuid, Uuid};

use crate::device_list::Battery;
use crate::model::Model;
pub use crate::update::Update;
use frame::{encode, DataType, Deframer, Frame};

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
    awaiting_ack: bool,
    /// Data frame payloads waiting for the outstanding one to be acknowledged.
    requests: VecDeque<Vec<u8>>,
    outgoing: Vec<Vec<u8>>,
    identified: bool,
}

impl Session {
    pub fn new() -> Session {
        let mut session = Session {
            deframer: Deframer::default(),
            seq: 0,
            awaiting_ack: false,
            requests: VecDeque::new(),
            outgoing: Vec::new(),
            identified: false,
        };
        session.request(vec![command::CONNECT_GET_PROTOCOL_INFO, FIXED_VALUE]);
        session
    }

    /// Takes the frames to write to the Device, in order.
    pub fn outgoing(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outgoing)
    }

    /// Feeds bytes received from the Device, in chunks of any size.
    pub fn receive(&mut self, bytes: &[u8]) -> Vec<Update> {
        let mut updates = Vec::new();
        for frame in self.deframer.push(bytes) {
            match frame.data_type {
                DataType::Ack => self.acknowledged(&frame),
                DataType::Mdr | DataType::MdrNo2 => {
                    self.outgoing
                        .push(encode(DataType::Ack, 1 - (frame.seq & 1), &[]));
                    if frame.data_type == DataType::Mdr {
                        if let Some(update) = self.message(&frame.data) {
                            if update == Update::Unsupported {
                                return vec![update];
                            }
                            updates.push(update);
                        }
                    }
                }
                DataType::Other(_) => {}
            }
        }
        updates
    }

    fn acknowledged(&mut self, ack: &Frame) {
        // A stale ACK, for a frame other than the outstanding one, is ignored.
        if !self.awaiting_ack || ack.seq != 1 - self.seq {
            return;
        }
        self.seq = ack.seq;
        self.awaiting_ack = false;
        self.send_next();
    }

    /// Handles a Table 1 message.
    fn message(&mut self, data: &[u8]) -> Option<Update> {
        match *data {
            [command::CONNECT_RET_PROTOCOL_INFO, ..] if !self.identified => {
                self.request(vec![command::CONNECT_GET_DEVICE_INFO, MODEL_NAME]);
                None
            }
            [command::CONNECT_RET_DEVICE_INFO, MODEL_NAME, len, ref name @ ..]
                if !self.identified =>
            {
                let name = name.get(..len as usize)?;
                let Some(model) = model_for_name(name) else {
                    return Some(Update::Unsupported);
                };
                self.identified = true;
                self.request(vec![command::CONNECT_GET_SUPPORT_FUNCTION, FIXED_VALUE]);
                Some(Update::Identified(model))
            }
            [command::CONNECT_RET_SUPPORT_FUNCTION, _, count, ref functions @ ..]
                if self.identified =>
            {
                let functions: Vec<u8> = functions
                    .chunks_exact(2)
                    .take(count as usize)
                    .map(|function| function[0])
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
                None
            }
            [command::POWER_RET_STATUS | command::POWER_NTFY_STATUS, power::BATTERY | power::BATTERY_WITH_THRESHOLD, level, ..]
                if self.identified && level <= 100 =>
            {
                Some(Update::Battery(Battery::Headphones { level }))
            }
            _ => None,
        }
    }

    fn request(&mut self, payload: Vec<u8>) {
        self.requests.push_back(payload);
        self.send_next();
    }

    fn send_next(&mut self) {
        if self.awaiting_ack {
            return;
        }
        if let Some(payload) = self.requests.pop_front() {
            self.outgoing
                .push(encode(DataType::Mdr, self.seq, &payload));
            self.awaiting_ack = true;
        }
    }
}

fn model_for_name(name: &[u8]) -> Option<Model> {
    (name == b"WH-1000XM5").then_some(Model::Wh1000Xm5)
}
