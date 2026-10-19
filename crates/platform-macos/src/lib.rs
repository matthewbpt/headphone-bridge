//! The macOS platform adapter: IOBluetooth behind the core's platform port.
//!
//! Thin and tested by hand. It knows nothing about vendor protocols: it reports
//! Devices becoming Connected and disconnected and the default audio output
//! (CoreAudio), and opens RFCOMM links by service UUID. It never connects or disconnects a Device itself.
//!
//! IOBluetooth delivers callbacks on the run loop of the thread that registered
//! them and isn't thread-safe, so a single thread owns every IOBluetooth object
//! and pumps its own run loop. The async side talks to it through commands.
//!
//! Bluetooth access only works from the bundled, signed app (see README).

#![cfg(target_os = "macos")]

mod audio_output;
mod bluetooth_thread;
mod delegates;

use std::io;
use std::sync::mpsc as std_mpsc;
use std::sync::{Mutex, PoisonError};

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
}

impl MacBluetooth {
    /// Starts the adapter, with the observations it reports.
    pub fn start() -> io::Result<(MacBluetooth, Observations)> {
        let (observations_tx, observations) = mpsc::unbounded_channel();
        let (commands, waker) = bluetooth_thread::spawn(observations_tx)?;
        let audio_changes = Mutex::new(commands.clone());
        let audio_waker = waker.clone();
        audio_output::watch(move || {
            audio_changes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .send(Command::CheckAudioOutput)
                .ok();
            audio_waker.wake();
        });
        Ok((MacBluetooth { commands, waker }, observations))
    }

    fn send(&self, command: Command) -> bool {
        let sent = self.commands.send(command).is_ok();
        self.waker.wake();
        sent
    }
}

impl Platform for MacBluetooth {
    async fn open_control_link(
        &self,
        address: DeviceAddress,
        service: Uuid,
    ) -> Result<ControlLink, LinkFailure> {
        let stopped = || {
            eprintln!("can't open a control link: the Bluetooth thread stopped");
            LinkFailure::BluetoothHandlingStopped
        };
        let (reply, opened) = oneshot::channel();
        if !self.send(Command::Open {
            address,
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
            commands.send(Command::Close { link }).ok();
            waker.wake();
        });
        Ok(ControlLink { incoming, outgoing })
    }
}
