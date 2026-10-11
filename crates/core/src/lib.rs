//! Headphone Bridge's platform-independent core.
//!
//! The public API is [`Core`]: [`Command`]s in, [`DeviceList`] snapshots out.
//! Everything OS-specific comes in through the [`platform::Platform`] port.

mod command;
mod device_list;
pub mod platform;

use tokio::sync::{mpsc, oneshot, watch};

pub use command::{Command, CommandError, NoiseControlMode};
pub use device_list::{Device, DeviceList};
use platform::{Observation, Platform};

/// A running core. Dropping it stops the core.
pub struct Core {
    commands: mpsc::UnboundedSender<(Command, oneshot::Sender<Result<(), CommandError>>)>,
    device_list: watch::Receiver<DeviceList>,
}

impl Core {
    /// Starts the core on the current Tokio runtime.
    pub fn start(platform: impl Platform) -> Core {
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let (device_list_tx, device_list) = watch::channel(DeviceList::default());
        let state = State {
            platform,
            device_list: device_list_tx,
        };
        tokio::spawn(state.run(commands_rx));
        Core {
            commands,
            device_list,
        }
    }

    /// Sends `command` to the core. Resolves once the command has been
    /// accepted or rejected; its effect shows up in a later snapshot, once the
    /// Device confirms it.
    pub async fn command(&self, command: Command) -> Result<(), CommandError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send((command, reply))
            .expect("core task stopped");
        response.await.expect("core task stopped")
    }

    /// The latest Device list snapshot.
    pub fn device_list(&self) -> DeviceList {
        self.device_list.borrow().clone()
    }

    /// Notifies on every new Device list snapshot.
    pub fn subscribe(&self) -> watch::Receiver<DeviceList> {
        self.device_list.clone()
    }
}

struct State<P> {
    platform: P,
    #[allow(dead_code)] // Nothing changes the Device list until Devices are tracked.
    device_list: watch::Sender<DeviceList>,
}

impl<P: Platform> State<P> {
    async fn run(
        mut self,
        mut commands: mpsc::UnboundedReceiver<(Command, oneshot::Sender<Result<(), CommandError>>)>,
    ) {
        let mut observations = self.platform.observations();
        loop {
            tokio::select! {
                Some(observation) = observations.recv() => self.observe(observation),
                command = commands.recv() => match command {
                    Some((command, reply)) => {
                        let _ = reply.send(self.command(command));
                    }
                    None => return,
                },
            }
        }
    }

    fn observe(&mut self, _observation: Observation) {}

    fn command(&mut self, _command: Command) -> Result<(), CommandError> {
        Err(CommandError::UnknownDevice)
    }
}
