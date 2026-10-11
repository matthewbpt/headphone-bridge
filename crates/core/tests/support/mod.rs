//! Scripted fake platform: the test double for the platform port.
//!
//! Tests play the OS through [`FakeOs`]: they emit observations, script how
//! opening a control link turns out, and take the device end of every link the
//! core opens so they can play back device frames and read what the core wrote.

#![allow(dead_code)]

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
    opened: mpsc::UnboundedSender<DeviceLink>,
}

/// The test's handle on the fake OS.
pub struct FakeOs {
    observations: mpsc::UnboundedSender<Observation>,
    failures: Arc<Mutex<HashMap<DeviceAddress, LinkFailure>>>,
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
    let (opened_tx, opened_rx) = mpsc::unbounded_channel();
    let failures = Arc::new(Mutex::new(HashMap::new()));
    let platform = FakePlatform {
        observations: Mutex::new(Some(observations_rx)),
        failures: failures.clone(),
        opened: opened_tx,
    };
    let os = FakeOs {
        observations: observations_tx,
        failures,
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
    pub fn fail_links(&self, address: &DeviceAddress, failure: LinkFailure) {
        self.failures
            .lock()
            .unwrap()
            .insert(address.clone(), failure);
    }

    pub fn allow_links(&self, address: &DeviceAddress) {
        self.failures.lock().unwrap().remove(address);
    }

    /// The next control link the core opens successfully.
    pub async fn next_link(&mut self) -> DeviceLink {
        self.opened.recv().await.expect("fake platform dropped")
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
        address: &DeviceAddress,
        service: Uuid,
    ) -> Result<ControlLink, LinkFailure> {
        if let Some(failure) = self.failures.lock().unwrap().get(address) {
            return Err(failure.clone());
        }
        let (to_core, from_device) = mpsc::unbounded_channel();
        let (to_device, from_core) = mpsc::unbounded_channel();
        let _ = self.opened.send(DeviceLink {
            address: address.clone(),
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
use headphone_bridge_core::{Core, DeviceList};
use uuid::uuid;

pub const SAMSUNG_SERVICE: Uuid = uuid!("2e73a4ad-332d-41fc-90e2-16bef06523f2");

/// A Galaxy Buds4 Pro as the OS reports it.
pub fn buds4pro() -> DeviceIdentity {
    DeviceIdentity {
        address: DeviceAddress("A1:B2:C3:D4:E5:F6".into()),
        name: "Galaxy Buds4 Pro (A1B2)".into(),
        service_uuids: vec![SAMSUNG_SERVICE],
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
