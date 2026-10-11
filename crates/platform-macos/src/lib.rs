//! The macOS platform adapter: IOBluetooth behind the core's platform port.
//!
//! Thin and tested by hand. It knows nothing about vendor protocols: it reports
//! Devices becoming Connected and disconnected, and opens RFCOMM links by
//! service UUID. It never connects or disconnects a Device itself.
//!
//! IOBluetooth delivers callbacks on the run loop of the thread that registered
//! them and isn't thread-safe, so a single thread owns every IOBluetooth object
//! and pumps its own run loop. The async side talks to it through commands.
//!
//! Bluetooth access only works from the bundled, signed app (see README).

#![cfg(target_os = "macos")]

mod bluetooth_thread;
mod delegates;

use std::sync::mpsc as std_mpsc;
use std::sync::Mutex;

use headphone_bridge_core::platform::{
    ControlLink, DeviceAddress, LinkFailure, Observations, Platform,
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use bluetooth_thread::{Command, Waker};

/// The IOBluetooth adapter. Starting it starts the Bluetooth thread, which
/// runs for the life of the process.
pub struct MacBluetooth {
    commands: std_mpsc::Sender<Command>,
    waker: Waker,
    observations: Mutex<Option<Observations>>,
}

impl MacBluetooth {
    pub fn start() -> MacBluetooth {
        let (observations_tx, observations) = mpsc::unbounded_channel();
        let (commands, waker) = bluetooth_thread::spawn(observations_tx);
        MacBluetooth {
            commands,
            waker,
            observations: Mutex::new(Some(observations)),
        }
    }

    fn send(&self, command: Command) -> bool {
        let sent = self.commands.send(command).is_ok();
        self.waker.wake();
        sent
    }
}

impl Platform for MacBluetooth {
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
        let stopped = || LinkFailure::Other("the Bluetooth thread stopped".into());
        let (reply, opened) = oneshot::channel();
        if !self.send(Command::Open {
            address: address.clone(),
            service,
            reply,
        }) {
            return Err(stopped());
        }
        let (link, incoming) = opened.await.map_err(|_| stopped())??;

        // Forward writes to the Bluetooth thread, and close the link once the
        // core drops its end.
        let (outgoing, mut writes) = mpsc::unbounded_channel::<Vec<u8>>();
        let commands = self.commands.clone();
        let waker = self.waker.clone();
        tokio::spawn(async move {
            while let Some(bytes) = writes.recv().await {
                if commands.send(Command::Write { link, bytes }).is_err() {
                    return;
                }
                waker.wake();
            }
            let _ = commands.send(Command::Close { link });
            waker.wake();
        });
        Ok(ControlLink { incoming, outgoing })
    }
}

/// Converts an IOBluetooth address (`ac-80-0a-11-22-33`) to the port's form
/// (`AC:80:0A:11:22:33`).
fn port_address(iobluetooth: &str) -> DeviceAddress {
    DeviceAddress(iobluetooth.replace('-', ":").to_ascii_uppercase())
}
