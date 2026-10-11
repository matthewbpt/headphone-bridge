//! Headphone Bridge's platform-independent core.
//!
//! The public API is [`Core`]: [`Command`]s in, [`DeviceList`] snapshots out.
//! Everything OS-specific comes in through the [`platform::Platform`] port.

mod command;
mod device_list;
pub mod frame_log;
mod model;
pub mod platform;
mod samsung;

use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

pub use command::{Command, CommandError, NoiseControlMode};
pub use device_list::{Battery, Device, DeviceList, DeviceState, NotAttachedReason};
pub use model::{AmbientLevelRange, Capabilities, Model};
use platform::{ControlLink, DeviceAddress, DeviceIdentity, Observation, Platform};

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
        let (link_events, link_events_rx) = mpsc::unbounded_channel();
        let state = State {
            platform: Arc::new(platform),
            device_list: device_list_tx,
            devices: BTreeMap::new(),
            link_events,
            next_attempt: 0,
        };
        tokio::spawn(state.run(commands_rx, link_events_rx));
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
    platform: Arc<P>,
    device_list: watch::Sender<DeviceList>,
    devices: BTreeMap<DeviceAddress, Tracked>,
    link_events: mpsc::UnboundedSender<LinkEvent>,
    /// Tells apart link sessions, so events from a session that has been
    /// replaced are ignored.
    next_attempt: u64,
}

/// A Connected Device and its control link session, if any.
struct Tracked {
    device: Device,
    session: Option<(u64, JoinHandle<()>)>,
}

impl Drop for Tracked {
    fn drop(&mut self) {
        if let Some((_, task)) = &self.session {
            task.abort();
        }
    }
}

struct LinkEvent {
    address: DeviceAddress,
    attempt: u64,
    kind: LinkEventKind,
}

enum LinkEventKind {
    Update(samsung::Update),
    /// The session ended: the link couldn't be opened, or it closed.
    Ended(NotAttachedReason),
}

impl<P: Platform> State<P> {
    async fn run(
        mut self,
        mut commands: mpsc::UnboundedReceiver<(Command, oneshot::Sender<Result<(), CommandError>>)>,
        mut link_events: mpsc::UnboundedReceiver<LinkEvent>,
    ) {
        let mut observations = self.platform.observations();
        loop {
            tokio::select! {
                Some(observation) = observations.recv() => self.observe(observation),
                Some(event) = link_events.recv() => self.link_event(event),
                command = commands.recv() => match command {
                    Some((command, reply)) => {
                        let _ = reply.send(self.command(command));
                    }
                    None => return,
                },
            }
            self.publish();
        }
    }

    fn observe(&mut self, observation: Observation) {
        match observation {
            Observation::DeviceConnected(identity) => self.connected(identity),
            Observation::DeviceDisconnected(address) => {
                self.devices.remove(&address);
            }
            Observation::AudioOutputChanged(_) => {}
        }
    }

    fn connected(&mut self, identity: DeviceIdentity) {
        if !identity.service_uuids.contains(&samsung::SERVICE) {
            return;
        }
        let candidate = samsung::candidate_model(&identity.name);
        let state = match candidate {
            Some(_) => DeviceState::Attaching,
            None => DeviceState::NotAttached {
                reason: NotAttachedReason::UnsupportedModel,
            },
        };
        let mut tracked = Tracked {
            device: Device {
                address: identity.address.clone(),
                name: identity.name,
                model: None,
                state,
                capabilities: Capabilities::default(),
                battery: None,
            },
            session: None,
        };
        if candidate.is_some() {
            tracked.session = Some(self.attach(&identity.address));
        }
        self.devices.insert(identity.address, tracked);
    }

    fn attach(&mut self, address: &DeviceAddress) -> (u64, JoinHandle<()>) {
        let attempt = self.next_attempt;
        self.next_attempt += 1;
        let platform = self.platform.clone();
        let events = self.link_events.clone();
        let address = address.clone();
        let task = tokio::spawn(async move {
            let send = |kind| {
                let _ = events.send(LinkEvent {
                    address: address.clone(),
                    attempt,
                    kind,
                });
            };
            let reason = match platform.open_control_link(&address, samsung::SERVICE).await {
                Ok(link) => {
                    run_samsung_session(link, |update| send(LinkEventKind::Update(update))).await
                }
                Err(failure) => failure.into(),
            };
            send(LinkEventKind::Ended(reason));
        });
        (attempt, task)
    }

    fn link_event(&mut self, event: LinkEvent) {
        let Some(tracked) = self.devices.get_mut(&event.address) else {
            return;
        };
        if tracked.session.as_ref().map(|(attempt, _)| *attempt) != Some(event.attempt) {
            return;
        }
        let device = &mut tracked.device;
        match event.kind {
            LinkEventKind::Update(samsung::Update::Identified(model)) => {
                device.model = Some(model);
                device.capabilities = model.capabilities();
                device.state = DeviceState::Attached;
            }
            // The session ends straight after, with that reason.
            LinkEventKind::Update(samsung::Update::Unsupported) => {}
            LinkEventKind::Update(samsung::Update::Battery(battery)) => {
                device.battery = Some(battery);
            }
            LinkEventKind::Ended(reason) => {
                *device = Device {
                    address: device.address.clone(),
                    name: device.name.clone(),
                    model: None,
                    state: DeviceState::NotAttached { reason },
                    capabilities: Capabilities::default(),
                    battery: None,
                };
                tracked.session = None;
            }
        }
    }

    fn command(&mut self, _command: Command) -> Result<(), CommandError> {
        Err(CommandError::UnknownDevice)
    }

    fn publish(&self) {
        let devices = self
            .devices
            .values()
            .map(|tracked| tracked.device.clone())
            .collect();
        self.device_list.send_if_modified(|list| {
            if list.devices == devices {
                return false;
            }
            list.devices = devices;
            true
        });
    }
}

/// Runs a Samsung control link session until the link closes, or until the
/// Device turns out to be an unsupported Model. Returns why it ended.
async fn run_samsung_session(
    mut link: ControlLink,
    mut on_update: impl FnMut(samsung::Update),
) -> NotAttachedReason {
    if link.outgoing.send(samsung::Session::handshake()).is_err() {
        return NotAttachedReason::LinkDropped;
    }
    let mut session = samsung::Session::default();
    while let Some(bytes) = link.incoming.recv().await {
        for update in session.receive(&bytes) {
            let unsupported = update == samsung::Update::Unsupported;
            on_update(update);
            if unsupported {
                // Dropping the link Detaches without writing anything more.
                return NotAttachedReason::UnsupportedModel;
            }
        }
    }
    NotAttachedReason::LinkDropped
}
