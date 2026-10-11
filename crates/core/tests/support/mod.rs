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
