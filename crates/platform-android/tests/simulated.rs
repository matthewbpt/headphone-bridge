//! The Simulated Device in front of the host, in debug builds, for the
//! emulator, which has no headphones. The Simulated Device's own behavior is
//! the shared crate's to test.

#![cfg(test)]
#![cfg(debug_assertions)]

use std::time::Duration;

use headphone_bridge_core::{Core, Device, DeviceList, DeviceState, Model};
use headphone_bridge_platform_android::messages::Message;
use headphone_bridge_platform_android::Host;
use tokio::sync::mpsc;

/// Kotlin's side of the boundary: every message the host sends it.
struct Kotlin {
    messages: mpsc::UnboundedReceiver<Message>,
}

fn start() -> (Host, Kotlin) {
    let (sink, messages) = mpsc::unbounded_channel();
    let host = Host::start(move |message| sink.send(message).is_ok(), Core::builder());
    (host, Kotlin { messages })
}

impl Kotlin {
    /// The first Device list after which `condition` holds of the Simulated
    /// Device, which is the only Device. Fails if Kotlin is asked to open a
    /// control link: the Simulated Device needs no Bluetooth.
    async fn simulated(&mut self, condition: impl Fn(&Device) -> bool) -> Device {
        let wait = async {
            loop {
                match self.messages.recv().await.expect("host stopped") {
                    Message::DeviceList { list } => {
                        if let Some(device) = only(&list).filter(|device| condition(device)) {
                            return device.clone();
                        }
                    }
                    Message::Open { .. } => panic!("Kotlin was asked to open a control link"),
                    _ => {}
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(5), wait)
            .await
            .expect("timed out waiting for the Simulated Device")
    }
}

fn only(list: &DeviceList) -> Option<&Device> {
    match list.devices.as_slice() {
        [device] => Some(device),
        _ => None,
    }
}

#[tokio::test]
async fn the_hosts_simulator_connects_a_simulated_device_that_attaches_when_held() {
    let (host, mut kotlin) = start();
    host.simulator()
        .simulate(Some(Model::GalaxyBuds4Pro))
        .unwrap();
    kotlin
        .simulated(|device| device.state == DeviceState::Detached)
        .await;

    host.event(r#"{"type":"holdAll"}"#).unwrap();
    let device = kotlin
        .simulated(|device| device.state == DeviceState::Attached)
        .await;
    assert_eq!(device.model, Some(Model::GalaxyBuds4Pro));
}
