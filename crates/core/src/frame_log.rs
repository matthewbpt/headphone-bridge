//! The frame record format shared by the frame log and the test fixtures.
//!
//! One record per line, fields separated by single spaces:
//!
//! ```text
//! 2026-09-13T13:26:47.128Z galaxy-buds4-pro >> fd 06 00 88 01 01 22 da 58 dd
//! ```
//!
//! 1. timestamp: RFC 3339, UTC, millisecond precision;
//! 2. Device Model slug (see [`crate::Model::slug`]), or the vendor's slug
//!    (`samsung`, `sony`) while the Model isn't identified yet;
//! 3. direction: `>>` sent to the Device, `<<` received from it;
//! 4. the whole frame as raw wire bytes, lowercase hex, one space between bytes.
//!
//! Lines starting with `#` are comments. `crates/core/tests/fixtures/README.md`
//! describes the fixture files.
//!
//! The frame log itself is a developer setting, off by default, switched with
//! [`crate::Command::SetFrameLog`]. While it's on, every control link session
//! records each frame it sends or receives, and a background thread appends
//! the records to the file, so sessions never wait on disk.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::str::FromStr;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The frame log setting. Clones share it, so switching it on or off reaches
/// every session, including ones already running.
#[derive(Debug, Clone, Default)]
pub(crate) struct FrameLog {
    writer: Arc<Mutex<Option<Writer>>>,
}

#[derive(Debug)]
struct Writer {
    records: mpsc::Sender<FrameRecord>,
    thread: JoinHandle<()>,
}

impl FrameLog {
    /// Switches the log on, appending to `path`, or switches it to `path` if
    /// it was on already.
    pub(crate) fn start(&self, path: &Path) -> io::Result<()> {
        self.stop();
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "# Headphone Bridge frame log")?;
        let (records, queue) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("frame-log".into())
            .spawn(move || write_records(file, queue))?;
        *self.writer.lock().unwrap() = Some(Writer { records, thread });
        Ok(())
    }

    /// Switches the log off, once every record so far is written.
    pub(crate) fn stop(&self) {
        let writer = self.writer.lock().unwrap().take();
        if let Some(Writer { records, thread }) = writer {
            drop(records);
            let _ = thread.join();
        }
    }

    /// Records a frame, timestamped now, if the log is on.
    pub(crate) fn record(&self, model: &str, direction: Direction, bytes: &[u8]) {
        if let Some(writer) = &*self.writer.lock().unwrap() {
            let _ = writer.records.send(FrameRecord {
                timestamp: SystemTime::now(),
                model: model.to_owned(),
                direction,
                bytes: bytes.to_vec(),
            });
        }
    }
}

/// Appends records until the log is switched off, flushing whenever the queue
/// runs dry. A failed write loses records, never stalls a session.
fn write_records(file: File, queue: mpsc::Receiver<FrameRecord>) {
    let mut out = BufWriter::new(file);
    while let Ok(record) = queue.recv() {
        let _ = writeln!(out, "{record}");
        for record in queue.try_iter() {
            let _ = writeln!(out, "{record}");
        }
        let _ = out.flush();
    }
}

/// One frame sent or received on a control link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameRecord {
    /// When the frame was sent or received, to millisecond precision.
    pub timestamp: SystemTime,
    pub model: String,
    pub direction: Direction,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Sent by the app to the Device: `>>`.
    Sent,
    /// Received by the app from the Device: `<<`.
    Received,
}

impl FrameRecord {
    /// Parses a whole frame log or fixture file, skipping comments and blank
    /// lines.
    pub fn parse_all(text: &str) -> Result<Vec<FrameRecord>, ParseError> {
        text.lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'))
            .map(|(index, line)| {
                line.parse().map_err(|ParseError(message)| {
                    ParseError(format!("line {}: {message}", index + 1))
                })
            })
            .collect()
    }
}

impl fmt::Display for FrameRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_timestamp(f, self.timestamp)?;
        let direction = match self.direction {
            Direction::Sent => ">>",
            Direction::Received => "<<",
        };
        write!(f, " {} {direction}", self.model)?;
        for byte in &self.bytes {
            write!(f, " {byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

impl FromStr for FrameRecord {
    type Err = ParseError;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let error = |message: &str| ParseError(format!("{message}: {line:?}"));
        let mut fields = line.split(' ');
        let timestamp = fields
            .next()
            .and_then(parse_timestamp)
            .ok_or_else(|| error("bad timestamp"))?;
        let model = fields
            .next()
            .filter(|model| !model.is_empty())
            .ok_or_else(|| error("missing model"))?
            .to_owned();
        let direction = match fields.next() {
            Some(">>") => Direction::Sent,
            Some("<<") => Direction::Received,
            _ => return Err(error("bad direction")),
        };
        let bytes = fields
            .map(|byte| {
                (byte.len() == 2)
                    .then(|| u8::from_str_radix(byte, 16).ok())
                    .flatten()
                    .ok_or_else(|| error("bad hex byte"))
            })
            .collect::<Result<Vec<u8>, _>>()?;
        if bytes.is_empty() {
            return Err(error("no bytes"));
        }
        Ok(FrameRecord {
            timestamp,
            model,
            direction,
            bytes,
        })
    }
}

const SECONDS_PER_DAY: u64 = 86_400;

fn write_timestamp(f: &mut fmt::Formatter<'_>, timestamp: SystemTime) -> fmt::Result {
    let since_epoch = timestamp.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since_epoch.as_secs();
    let (year, month, day) = civil_from_days((seconds / SECONDS_PER_DAY) as i64);
    let seconds_of_day = seconds % SECONDS_PER_DAY;
    write!(
        f,
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        seconds_of_day / 3600,
        seconds_of_day / 60 % 60,
        seconds_of_day % 60,
        since_epoch.subsec_millis()
    )
}

/// Parses `YYYY-MM-DDTHH:MM:SS.mmmZ`.
fn parse_timestamp(text: &str) -> Option<SystemTime> {
    let b = text.as_bytes();
    if b.len() != 24 || [b[4], b[7], b[10], b[13], b[16], b[19], b[23]] != *b"--T::.Z" {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<u64> {
        let digits = &text[range];
        digits
            .bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| digits.parse().ok())
            .flatten()
    };
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    let millis = number(20..23)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let days = u64::try_from(days_from_civil(year as i64, month as u32, day as u32)).ok()?;
    let seconds = days * SECONDS_PER_DAY + hour * 3600 + minute * 60 + second;
    Some(UNIX_EPOCH + Duration::from_secs(seconds) + Duration::from_millis(millis))
}

// Howard Hinnant's civil calendar algorithms.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}
