//! Samsung SPP framing.
//!
//! ```text
//! 0xFD | header u16 LE | id u8 | payload | CRC16 u16 LE | 0xDD
//! ```
//!
//! Header bits 0–9 are `size` = 1 (id) + payload length + 2 (CRC). Bit 12 is
//! the response flag and bit 13 the fragment flag; the Device also sets bits
//! 10–11 and a rolling sequence counter in bits 14–15. Only the end byte and
//! the CRC decide whether a frame is valid, never the flag bits. Ported from
//! omarchy-buds (MIT).

const SOM: u8 = 0xFD;
const EOM: u8 = 0xDD;
const SIZE_MASK: u16 = 0x3FF;
/// Start byte, header and end byte: the bytes a frame has beyond `size`.
const OVERHEAD: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub id: u8,
    pub payload: Vec<u8>,
    /// The whole frame as it appeared on the wire.
    pub raw: Vec<u8>,
}

/// Encodes a frame from the app, with all header flags clear.
pub fn encode(id: u8, payload: &[u8]) -> Vec<u8> {
    let size = (1 + payload.len() + 2) as u16;
    let mut frame = Vec::with_capacity(size as usize + OVERHEAD);
    frame.push(SOM);
    frame.extend_from_slice(&(size & SIZE_MASK).to_le_bytes());
    frame.push(id);
    frame.extend_from_slice(payload);
    frame.extend_from_slice(&crc16(&frame[3..]).to_le_bytes());
    frame.push(EOM);
    frame
}

/// CRC-16/XMODEM (poly 0x1021, init 0, unreflected), over id and payload.
fn crc16(data: &[u8]) -> u16 {
    data.iter().fold(0u16, |crc, &byte| {
        (0..8).fold(crc ^ (u16::from(byte) << 8), |crc, _| {
            if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            }
        })
    })
}

/// Reassembles frames from a byte stream that arrives in arbitrary chunks,
/// skipping garbage and frames that fail their checks.
#[derive(Debug, Default)]
pub struct Deframer {
    buffer: Vec<u8>,
}

enum Candidate {
    Valid(Frame),
    Invalid,
    Incomplete,
}

impl Deframer {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Frame> {
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();
        let mut start = 0;
        loop {
            let Some(offset) = self.buffer[start..].iter().position(|&b| b == SOM) else {
                start = self.buffer.len();
                break;
            };
            start += offset;
            match candidate_at(&self.buffer[start..]) {
                Candidate::Valid(frame) => {
                    start += frame.raw.len();
                    frames.push(frame);
                }
                Candidate::Invalid => start += 1,
                Candidate::Incomplete => {
                    // A start byte in garbage can claim a size that swallows a
                    // real frame; if a valid frame follows, resync onto it.
                    match (start + 1..self.buffer.len()).find(|&next| {
                        self.buffer[next] == SOM
                            && matches!(candidate_at(&self.buffer[next..]), Candidate::Valid(_))
                    }) {
                        Some(next) => start = next,
                        None => break,
                    }
                }
            }
        }
        self.buffer.drain(..start);
        frames
    }
}

fn candidate_at(bytes: &[u8]) -> Candidate {
    if bytes.len() < 3 {
        return Candidate::Incomplete;
    }
    let size = (u16::from_le_bytes([bytes[1], bytes[2]]) & SIZE_MASK) as usize;
    if size < 3 {
        return Candidate::Invalid;
    }
    let total = size + OVERHEAD;
    if bytes.len() < total {
        return Candidate::Incomplete;
    }
    let body = &bytes[3..3 + size - 2];
    let crc = u16::from_le_bytes([bytes[total - 3], bytes[total - 2]]);
    if bytes[total - 1] != EOM || crc16(body) != crc {
        return Candidate::Invalid;
    }
    Candidate::Valid(Frame {
        id: body[0],
        payload: body[1..].to_vec(),
        raw: bytes[..total].to_vec(),
    })
}
