//! Sony MDR v2 framing.
//!
//! ```text
//! 0x3E | escaped(type u8 | seq u8 | size u32 BE | data | checksum u8) | 0x3C
//! ```
//!
//! The checksum is the byte sum of type, seq, size and data, before escaping.
//! Escaping replaces any `0x3C`, `0x3D` or `0x3E` inside the frame with `0x3D`
//! followed by the byte minus `0x10`, so the end byte never appears inside a
//! frame. Ported from libmdr (MIT).

const START: u8 = 0x3E;
const END: u8 = 0x3C;
const ESCAPE: u8 = 0x3D;
/// Type, seq and size: the bytes before the data.
const HEADER: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Ack,
    /// Table 1 messages, which carry everything v1 needs.
    Mdr,
    /// Table 2 messages.
    MdrNo2,
    Other(u8),
}

impl DataType {
    fn byte(self) -> u8 {
        match self {
            DataType::Ack => 0x01,
            DataType::Mdr => 0x0C,
            DataType::MdrNo2 => 0x0E,
            DataType::Other(byte) => byte,
        }
    }

    fn from_byte(byte: u8) -> DataType {
        match byte {
            0x01 => DataType::Ack,
            0x0C => DataType::Mdr,
            0x0E => DataType::MdrNo2,
            other => DataType::Other(other),
        }
    }
}

/// A data frame's 1-bit sequence number. Each side flips its own after every
/// data frame the other acknowledges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sequence(u8);

impl Sequence {
    /// A control link's first data frame's.
    pub const FIRST: Sequence = Sequence(0);

    /// The sequence number a frame's seq byte carries: its low bit.
    pub const fn from_byte(byte: u8) -> Sequence {
        Sequence(byte & 1)
    }

    pub const fn byte(self) -> u8 {
        self.0
    }

    /// The other sequence number: the next data frame's, after this one.
    pub const fn flipped(self) -> Sequence {
        Sequence(1 - self.0)
    }

    /// The sequence number of the ACK that answers a data frame with this one.
    pub const fn ack(self) -> Sequence {
        self.flipped()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub data_type: DataType,
    /// The seq byte as it was on the wire, 0 or 1 from a well-behaved
    /// sender: [`Sequence::from_byte`] reads it.
    pub seq: u8,
    pub data: Vec<u8>,
    /// The whole frame as it appeared on the wire.
    pub raw: Vec<u8>,
}

pub fn encode(data_type: DataType, seq: Sequence, data: &[u8]) -> Vec<u8> {
    let mut body = vec![data_type.byte(), seq.byte()];
    body.extend_from_slice(&(data.len() as u32).to_be_bytes());
    body.extend_from_slice(data);
    body.push(checksum(&body));
    let mut frame = vec![START];
    for byte in body {
        if matches!(byte, START | END | ESCAPE) {
            frame.extend_from_slice(&[ESCAPE, byte - 0x10]);
        } else {
            frame.push(byte);
        }
    }
    frame.push(END);
    frame
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte))
}

/// Reassembles frames from a byte stream that arrives in arbitrary chunks,
/// skipping garbage and frames that fail their checks.
#[derive(Debug, Default)]
pub struct Deframer {
    buffer: Vec<u8>,
}

impl Deframer {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Frame> {
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();
        let mut consumed = 0;
        while let Some(start) = self.buffer[consumed..].iter().position(|&b| b == START) {
            let start = consumed + start;
            let Some(end) = self.buffer[start..].iter().position(|&b| b == END) else {
                // Keep the incomplete frame until its end arrives.
                consumed = start;
                break;
            };
            let end = start + end;
            // A start byte inside the candidate means the earlier one was
            // garbage: decode from the last one.
            let last_start = self.buffer[start..end]
                .iter()
                .rposition(|&b| b == START)
                .map_or(start, |offset| start + offset);
            frames.extend(decode(&self.buffer[last_start..=end]));
            consumed = end + 1;
        }
        if self.buffer[consumed..].iter().all(|&b| b != START) {
            consumed = self.buffer.len();
        }
        self.buffer.drain(..consumed);
        frames
    }
}

/// Decodes a frame from its start byte to its end byte.
fn decode(raw: &[u8]) -> Option<Frame> {
    let escaped = &raw[1..raw.len() - 1];
    let mut body = Vec::with_capacity(escaped.len());
    let mut bytes = escaped.iter();
    while let Some(&byte) = bytes.next() {
        if byte == ESCAPE {
            match bytes.next()?.wrapping_add(0x10) {
                byte @ (START | END | ESCAPE) => body.push(byte),
                _ => return None,
            }
        } else {
            body.push(byte);
        }
    }
    let (&sum, body) = body.split_last()?;
    if body.len() < HEADER || checksum(body) != sum {
        return None;
    }
    let size = u32::from_be_bytes([body[2], body[3], body[4], body[5]]) as usize;
    let data = &body[HEADER..];
    if data.len() != size {
        return None;
    }
    Some(Frame {
        data_type: DataType::from_byte(body[0]),
        seq: body[1],
        data: data.to_vec(),
        raw: raw.to_vec(),
    })
}
