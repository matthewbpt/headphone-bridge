//! Scripted fake platform: the test double for the platform port.
//!
//! Tests play the OS through [`FakeOs`]: they emit observations, script how
//! opening a control link turns out, and take the device end of every link the
//! core opens so they can play back device frames and read what the core wrote.

#![allow(
    dead_code,
    reason = "each test crate uses a different part of the fake"
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use headphone_bridge_core::platform::{
    ControlLink, DeviceAddress, LinkFailure, Observation, Observations, Platform,
};
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct FakePlatform {
    observations: Mutex<Option<Observations>>,
    failures: Arc<Mutex<HashMap<DeviceAddress, LinkFailure>>>,
    attempted: mpsc::UnboundedSender<DeviceAddress>,
    opened: mpsc::UnboundedSender<DeviceLink>,
}

/// The test's handle on the fake OS.
pub struct FakeOs {
    observations: mpsc::UnboundedSender<Observation>,
    failures: Arc<Mutex<HashMap<DeviceAddress, LinkFailure>>>,
    attempted: mpsc::UnboundedReceiver<DeviceAddress>,
    opened: mpsc::UnboundedReceiver<DeviceLink>,
}

/// The device end of a control link the core opened.
pub struct DeviceLink {
    pub address: DeviceAddress,
    pub service: Uuid,
    /// Frames the core wrote, one item per write.
    pub written: mpsc::UnboundedReceiver<Vec<u8>>,
    /// Bytes to deliver to the core as if the device sent them.
    pub send: mpsc::UnboundedSender<Vec<u8>>,
}

pub fn fake_platform() -> (FakePlatform, FakeOs) {
    let (observations_tx, observations_rx) = mpsc::unbounded_channel();
    let (attempted_tx, attempted_rx) = mpsc::unbounded_channel();
    let (opened_tx, opened_rx) = mpsc::unbounded_channel();
    let failures = Arc::new(Mutex::new(HashMap::new()));
    let platform = FakePlatform {
        observations: Mutex::new(Some(observations_rx)),
        failures: failures.clone(),
        attempted: attempted_tx,
        opened: opened_tx,
    };
    let os = FakeOs {
        observations: observations_tx,
        failures,
        attempted: attempted_rx,
        opened: opened_rx,
    };
    (platform, os)
}

impl FakeOs {
    pub fn observe(&self, observation: Observation) {
        self.observations
            .send(observation)
            .expect("core stopped listening to observations");
    }

    /// Every attempt to open a control link to `address` fails with `failure`
    /// until [`FakeOs::allow_links`] is called.
    pub fn fail_links(&self, address: DeviceAddress, failure: LinkFailure) {
        self.failures.lock().unwrap().insert(address, failure);
    }

    pub fn allow_links(&self, address: DeviceAddress) {
        self.failures.lock().unwrap().remove(&address);
    }

    /// The address of the next Device the core tries to open a control link
    /// to, whether or not that succeeds.
    pub async fn next_attempt(&mut self) -> DeviceAddress {
        tokio::time::timeout(Duration::from_secs(600), self.attempted.recv())
            .await
            .expect("timed out waiting for the core to open a control link")
            .expect("fake platform dropped")
    }

    /// Whether the core tries to open any control link within `within`.
    pub async fn attempts_within(&mut self, within: Duration) -> bool {
        tokio::time::timeout(within, self.attempted.recv())
            .await
            .is_ok()
    }

    /// The next control link the core opens successfully.
    pub async fn next_link(&mut self) -> DeviceLink {
        tokio::time::timeout(Duration::from_secs(600), self.opened.recv())
            .await
            .expect("timed out waiting for the core to open a control link")
            .expect("fake platform dropped")
    }
}

impl Platform for FakePlatform {
    fn observations(&self) -> Observations {
        self.observations
            .lock()
            .unwrap()
            .take()
            .expect("observations taken twice")
    }

    async fn open_control_link(
        &self,
        address: DeviceAddress,
        service: Uuid,
    ) -> Result<ControlLink, LinkFailure> {
        let _ = self.attempted.send(address);
        if let Some(failure) = self.failures.lock().unwrap().get(&address) {
            return Err(failure.clone());
        }
        let (to_core, from_device) = mpsc::unbounded_channel();
        let (to_device, from_core) = mpsc::unbounded_channel();
        let _ = self.opened.send(DeviceLink {
            address,
            service,
            written: from_core,
            send: to_core,
        });
        Ok(ControlLink {
            incoming: from_device,
            outgoing: to_device,
        })
    }
}

use std::time::Duration;

use headphone_bridge_core::frame_log::{Direction, FrameRecord};
use headphone_bridge_core::platform::DeviceIdentity;
use headphone_bridge_core::wire::samsung::id;
use headphone_bridge_core::{
    Command, CommandKind, Core, DeviceCommand, DeviceList, Equalizer, PresetId,
};
use uuid::uuid;

pub const SAMSUNG_SERVICE: Uuid = uuid!("2e73a4ad-332d-41fc-90e2-16bef06523f2");
pub const SONY_SERVICE: Uuid = uuid!("956c7b26-d49a-4ba8-b03f-b17d393cb6e2");

/// A command for the Device at `address`.
pub fn device_command(address: DeviceAddress, kind: CommandKind) -> Command {
    Command::Device(DeviceCommand { address, kind })
}

/// A Galaxy Buds4 Pro as the OS reports it.
pub fn buds4pro() -> DeviceIdentity {
    DeviceIdentity {
        address: DeviceAddress::parse("A1:B2:C3:D4:E5:F6").unwrap(),
        name: "Galaxy Buds4 Pro (A1B2)".into(),
        service_uuids: vec![SAMSUNG_SERVICE],
    }
}

/// A Galaxy Buds2 as the OS reports it: the owner's, as macOS names it.
pub fn buds2() -> DeviceIdentity {
    DeviceIdentity {
        address: DeviceAddress::parse("B1:BD:C3:D4:E5:F6").unwrap(),
        name: "Galaxy Buds2 (B1BD)".into(),
        service_uuids: vec![SAMSUNG_SERVICE],
    }
}

/// A WH-1000XM5 as the OS reports it.
pub fn xm5() -> DeviceIdentity {
    DeviceIdentity {
        address: DeviceAddress::parse("AC:80:0A:11:22:33").unwrap(),
        name: "WH-1000XM5".into(),
        service_uuids: vec![SONY_SERVICE],
    }
}

/// Loads `tests/fixtures/<name>.frames`.
pub fn fixture(name: &str) -> Vec<FrameRecord> {
    let path = format!(
        "{}/tests/fixtures/{name}.frames",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    FrameRecord::parse_all(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The frames the Device sent, in order.
pub fn received(records: &[FrameRecord]) -> Vec<Vec<u8>> {
    records
        .iter()
        .filter(|record| record.direction == Direction::Received)
        .map(|record| record.bytes.clone())
        .collect()
}

/// The frames the app sent, in order.
pub fn sent(records: &[FrameRecord]) -> Vec<Vec<u8>> {
    records
        .iter()
        .filter(|record| record.direction == Direction::Sent)
        .map(|record| record.bytes.clone())
        .collect()
}

/// Plays a recorded session on `link`: delivers each received frame to the
/// core, and checks each sent frame is the next one the core writes.
pub async fn replay(link: &mut DeviceLink, records: &[FrameRecord]) {
    for record in records {
        match record.direction {
            Direction::Received => link.send.send(record.bytes.clone()).unwrap(),
            Direction::Sent => {
                let written = tokio::time::timeout(Duration::from_secs(5), link.written.recv())
                    .await
                    .unwrap_or_else(|_| panic!("timed out waiting for the core to write {record}"));
                assert_eq!(written.as_ref(), Some(&record.bytes), "expected {record}");
            }
        }
    }
}

/// Starts a core, reports `device` Connected, and takes the control link the
/// core opens to it.
pub async fn linked_core(device: DeviceIdentity) -> (Core, FakeOs, DeviceLink) {
    let (platform, mut os) = fake_platform();
    let core = Core::builder().start(platform);
    os.observe(Observation::DeviceConnected(device));
    let link = os.next_link().await;
    (core, os, link)
}

/// Plays a recorded Galaxy Buds session of `device` from its Attach up to the
/// first `EQUALIZER` the app sent, checking it reports Preset `attached_on`,
/// then picks `picks` in turn, replaying each set and the Device's answer, and
/// checks each Preset waits for its ACK.
pub async fn real_preset_picks_are_confirmed_by_their_acks(
    device: DeviceIdentity,
    name: &str,
    attached_on: u8,
    picks: &[u8],
) {
    let preset = |preset| Equalizer {
        preset: PresetId(preset),
        bands: None,
        clear_bass: None,
    };
    let (core, _os, mut link) = linked_core(device.clone()).await;
    let records = fixture(name);
    let set_at: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.direction == Direction::Sent && r.bytes.get(3) == Some(&id::EQUALIZER))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(set_at.len(), picks.len());

    replay(&mut link, &records[..set_at[0]]).await;
    wait_for(&core, |list| {
        list.devices[0].equalizer == Some(preset(attached_on))
    })
    .await;
    let mut confirmed = attached_on;
    for (i, &pick) in picks.iter().enumerate() {
        core.command(device_command(
            device.address,
            CommandKind::SetEqualizer {
                equalizer: preset(pick),
            },
        ))
        .await
        .unwrap();
        assert_eq!(
            core.device_list().devices[0].equalizer,
            Some(preset(confirmed)),
            "Preset {pick} before its ACK"
        );
        let end = set_at.get(i + 1).copied().unwrap_or(records.len());
        replay(&mut link, &records[set_at[i]..end]).await;
        wait_for(&core, |list| {
            list.devices[0].equalizer == Some(preset(pick))
        })
        .await;
        confirmed = pick;
    }
}

/// Waits for a Device list snapshot that satisfies `condition`, and returns it.
pub async fn wait_for(core: &Core, condition: impl Fn(&DeviceList) -> bool) -> DeviceList {
    let mut snapshots = core.subscribe();
    let wait = async {
        snapshots
            .wait_for(|list| condition(list))
            .await
            .map(|list| list.clone())
    };
    match tokio::time::timeout(Duration::from_secs(5), wait).await {
        Ok(Ok(list)) => list,
        _ => panic!("timed out; last snapshot: {:#?}", core.device_list()),
    }
}

/// Encodes a Samsung frame the way a Device would, with all header flags
/// clear. Only for frames no fixture has; the CRC is CRC-16/XMODEM.
pub fn samsung_frame(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut body = vec![id];
    body.extend_from_slice(payload);
    let crc = body.iter().fold(0u16, |crc, &byte| {
        (0..8).fold(crc ^ (u16::from(byte) << 8), |crc, _| {
            if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            }
        })
    });
    let mut frame = vec![0xfd];
    frame.extend_from_slice(&((body.len() + 2) as u16).to_le_bytes());
    frame.extend(body);
    frame.extend_from_slice(&crc.to_le_bytes());
    frame.push(0xdd);
    frame
}

/// Encodes a Sony MDR v2 Table 1 (`0x0C`) frame the way a Device would. Only
/// for frames no fixture has; see `tests/fixtures/wh-1000xm5/gen.py`.
pub fn sony_frame(seq: u8, data: &[u8]) -> Vec<u8> {
    sony_encode(0x0c, seq, data)
}

/// A Sony ACK with sequence number `seq`.
pub fn sony_ack(seq: u8) -> Vec<u8> {
    sony_encode(0x01, seq, &[])
}

fn sony_encode(data_type: u8, seq: u8, data: &[u8]) -> Vec<u8> {
    let mut body = vec![data_type, seq];
    body.extend_from_slice(&(data.len() as u32).to_be_bytes());
    body.extend_from_slice(data);
    body.push(body.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte)));
    let mut frame = vec![0x3e];
    for byte in body {
        match byte {
            0x3c..=0x3e => frame.extend([0x3d, byte - 0x10]),
            _ => frame.push(byte),
        }
    }
    frame.push(0x3c);
    frame
}

/// A Sony frame's type, sequence number and data.
pub fn sony_decode(frame: &[u8]) -> (u8, u8, Vec<u8>) {
    let mut body = Vec::new();
    let mut escaped = frame[1..frame.len() - 1].iter();
    while let Some(&byte) = escaped.next() {
        match byte {
            0x3d => body.push(escaped.next().unwrap() + 0x10),
            _ => body.push(byte),
        }
    }
    (body[0], body[1], body[6..body.len() - 1].to_vec())
}

/// A real WH-1000XM5 capture recorded before the app read the Equalizer on
/// Attach, re-based onto an Attach that does. After the app ACKs the NCASM
/// reply, it inserts the app's `EQEBB_GET_STATUS`, the Device's ACK and an
/// ENABLE reply, then the app's `EQEBB_GET_PARAM`, the Device's ACK and
/// `equalizer_reply`, each reply ACKed by the app. The two reads leave every
/// later sequence number as recorded.
pub fn with_equalizer_read(records: &[FrameRecord], equalizer_reply: &[u8]) -> Vec<FrameRecord> {
    let ncasm_reply = records
        .iter()
        .position(|r| {
            r.direction == Direction::Received && sony_decode(&r.bytes).2.first() == Some(&0x67)
        })
        .expect("an NCASM reply");
    let (_, device_seq, _) = sony_decode(&records[ncasm_reply].bytes);
    let (_, ncasm_get_seq, _) = records[..ncasm_reply]
        .iter()
        .rev()
        .filter(|r| r.direction == Direction::Sent)
        .map(|r| sony_decode(&r.bytes))
        .find(|(data_type, _, _)| *data_type == 0x0c)
        .expect("the NCASM GET");
    let at = ncasm_reply + 2;
    let record = |direction, bytes| FrameRecord {
        direction,
        bytes,
        ..records[at - 1].clone()
    };
    let read = |seq: u8, device_seq: u8, request: &[u8], reply: &[u8]| {
        [
            record(Direction::Sent, sony_frame(seq, request)),
            record(Direction::Received, sony_ack(1 - seq)),
            record(Direction::Received, sony_frame(device_seq, reply)),
            record(Direction::Sent, sony_ack(1 - device_seq)),
        ]
    };
    let mut rebased = records[..at].to_vec();
    let (seq, device_seq) = (1 - ncasm_get_seq, 1 - device_seq);
    rebased.extend(read(seq, device_seq, &[0x52, 0x00], &[0x53, 0x00, 0x00]));
    rebased.extend(read(
        1 - seq,
        1 - device_seq,
        &[0x56, 0x00],
        equalizer_reply,
    ));
    rebased.extend_from_slice(&records[at..]);
    rebased
}

/// Sony's request for what an Equalizer offers. The app as it ships never
/// sends it; only a development build's probe did.
pub const EQEBB_GET_CAPABILITY: u8 = 0x50;
/// The WH-1000XM5's reply to [`EQEBB_GET_CAPABILITY`].
pub const EQEBB_RET_CAPABILITY: u8 = 0x51;

/// A real WH-1000XM5 capture with the temporary `EQEBB_GET_CAPABILITY`
/// probe its development build sent after Attach, re-based onto the app as
/// it ships. It drops the probe, the Device's ACK and `EQEBB_RET_CAPABILITY`
/// reply, and the app's ACK of that, then flips the sequence number of every
/// later frame the app sent and of the Device's ACKs of them, as the probe no
/// longer takes one. The Device's own frames, and the app's ACKs of them,
/// keep theirs.
pub fn without_capability_probe(records: &[FrameRecord]) -> Vec<FrameRecord> {
    let decoded = |r: &FrameRecord| sony_decode(&r.bytes);
    let probe = records
        .iter()
        .position(|r| {
            r.direction == Direction::Sent && decoded(r).2.first() == Some(&EQEBB_GET_CAPABILITY)
        })
        .expect("the EQEBB_GET_CAPABILITY probe");
    let probe_ack = probe
        + records[probe..]
            .iter()
            .position(|r| r.direction == Direction::Received && decoded(r).0 == 0x01)
            .expect("the Device's ACK of the probe");
    let reply = probe
        + records[probe..]
            .iter()
            .position(|r| {
                r.direction == Direction::Received
                    && decoded(r).2.first() == Some(&EQEBB_RET_CAPABILITY)
            })
            .expect("the EQEBB_RET_CAPABILITY reply");
    let reply_ack = reply + 1;
    assert_eq!(records[reply_ack].direction, Direction::Sent);
    assert_eq!(decoded(&records[reply_ack]).0, 0x01);
    records
        .iter()
        .enumerate()
        .filter(|(i, _)| ![probe, probe_ack, reply, reply_ack].contains(i))
        .map(|(i, r)| {
            let (data_type, seq, data) = decoded(r);
            let bytes = match (r.direction, data_type) {
                _ if i < probe => return r.clone(),
                (Direction::Sent, 0x0c) => sony_frame(1 - seq, &data),
                (Direction::Received, 0x01) => sony_ack(1 - seq),
                _ => return r.clone(),
            };
            FrameRecord { bytes, ..r.clone() }
        })
        .collect()
}
