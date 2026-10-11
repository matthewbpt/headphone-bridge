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
mod sony;
mod update;

use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

pub use command::{Command, CommandError, NoiseControlMode};
pub use device_list::{Battery, Device, DeviceList, DeviceState, NotAttachedReason};
use frame_log::{Direction, FrameLog};
pub use model::{AmbientLevelRange, Capabilities, Model};
use platform::{ControlLink, DeviceAddress, DeviceIdentity, Observation, Platform};
use update::Update;

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
            frame_log: FrameLog::default(),
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
    frame_log: FrameLog,
}

/// A Connected Device and its control link session, if any.
struct Tracked {
    device: Device,
    session: Option<LinkSession>,
}

/// A running control link session task.
struct LinkSession {
    attempt: u64,
    task: JoinHandle<()>,
    commands: mpsc::UnboundedSender<SessionCommand>,
}

impl Drop for LinkSession {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A command for the Device, already checked against its Capabilities.
enum SessionCommand {
    SetNoiseControlMode(NoiseControlMode),
}

#[derive(Debug, Clone, Copy)]
enum Vendor {
    Samsung,
    Sony,
}

struct LinkEvent {
    address: DeviceAddress,
    attempt: u64,
    kind: LinkEventKind,
}

enum LinkEventKind {
    Update(Update),
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
        let has_service = |service| identity.service_uuids.contains(&service);
        let vendor = if has_service(samsung::SERVICE) {
            Vendor::Samsung
        } else if has_service(sony::SERVICE) {
            Vendor::Sony
        } else {
            return;
        };
        // A Sony Device reveals its Model in reply to reads alone, so every one
        // is Attached to identify it. A Samsung Device needs a write first, so
        // only those whose name points at the allowlist are (ADR-0003).
        let attach = match vendor {
            Vendor::Samsung => samsung::candidate_model(&identity.name).is_some(),
            Vendor::Sony => true,
        };
        let state = if attach {
            DeviceState::Attaching
        } else {
            DeviceState::NotAttached {
                reason: NotAttachedReason::UnsupportedModel,
            }
        };
        let mut tracked = Tracked {
            device: Device {
                address: identity.address.clone(),
                name: identity.name,
                model: None,
                state,
                capabilities: Capabilities::default(),
                chosen_mode: None,
                battery: None,
            },
            session: None,
        };
        if attach {
            tracked.session = Some(self.attach(&identity.address, vendor));
        }
        self.devices.insert(identity.address, tracked);
    }

    fn attach(&mut self, address: &DeviceAddress, vendor: Vendor) -> LinkSession {
        let attempt = self.next_attempt;
        self.next_attempt += 1;
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let platform = self.platform.clone();
        let events = self.link_events.clone();
        let frame_log = self.frame_log.clone();
        let address = address.clone();
        let task = tokio::spawn(async move {
            let send = |kind| {
                let _ = events.send(LinkEvent {
                    address: address.clone(),
                    attempt,
                    kind,
                });
            };
            let service = match vendor {
                Vendor::Samsung => samsung::SERVICE,
                Vendor::Sony => sony::SERVICE,
            };
            let on_update = |update| send(LinkEventKind::Update(update));
            let reason = match platform.open_control_link(&address, service).await {
                Ok(link) => match vendor {
                    Vendor::Samsung => {
                        run_samsung_session(link, commands_rx, frame_log, on_update).await
                    }
                    Vendor::Sony => run_sony_session(link, frame_log, on_update).await,
                },
                Err(failure) => failure.into(),
            };
            send(LinkEventKind::Ended(reason));
        });
        LinkSession {
            attempt,
            task,
            commands,
        }
    }

    fn link_event(&mut self, event: LinkEvent) {
        let Some(tracked) = self.devices.get_mut(&event.address) else {
            return;
        };
        if tracked.session.as_ref().map(|session| session.attempt) != Some(event.attempt) {
            return;
        }
        let device = &mut tracked.device;
        match event.kind {
            LinkEventKind::Update(Update::Identified(model)) => {
                device.model = Some(model);
                device.capabilities = model.capabilities();
                device.state = DeviceState::Attached;
            }
            // The session ends straight after, with that reason.
            LinkEventKind::Update(Update::Unsupported) => {}
            LinkEventKind::Update(Update::Battery(battery)) => {
                device.battery = Some(battery);
            }
            LinkEventKind::Update(Update::ChosenMode(mode)) => {
                device.chosen_mode = Some(mode);
            }
            LinkEventKind::Ended(reason) => {
                *device = Device {
                    address: device.address.clone(),
                    name: device.name.clone(),
                    model: None,
                    state: DeviceState::NotAttached { reason },
                    capabilities: Capabilities::default(),
                    chosen_mode: None,
                    battery: None,
                };
                tracked.session = None;
            }
        }
    }

    fn command(&mut self, command: Command) -> Result<(), CommandError> {
        match command {
            Command::SetNoiseControlMode(address, mode) => {
                let tracked = self
                    .devices
                    .get(&address)
                    .ok_or(CommandError::UnknownDevice)?;
                let session = match (&tracked.device.state, &tracked.session) {
                    (DeviceState::Attached, Some(session)) => session,
                    _ => return Err(CommandError::NotAttached),
                };
                if !tracked
                    .device
                    .capabilities
                    .noise_control_modes
                    .contains(&mode)
                {
                    return Err(CommandError::MissingCapability);
                }
                session
                    .commands
                    .send(SessionCommand::SetNoiseControlMode(mode))
                    .map_err(|_| CommandError::NotAttached)
            }
            Command::SetFrameLog(Some(path)) => self
                .frame_log
                .start(&path)
                .map_err(|error| CommandError::FrameLogUnavailable(error.to_string())),
            Command::SetFrameLog(None) => {
                self.frame_log.stop();
                Ok(())
            }
            _ => Err(CommandError::UnknownDevice),
        }
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
    mut commands: mpsc::UnboundedReceiver<SessionCommand>,
    frame_log: FrameLog,
    mut on_update: impl FnMut(Update),
) -> NotAttachedReason {
    // Frames are logged under the vendor's slug until the Model is identified.
    let mut slug = "samsung";
    // Every frame written goes through here, so all of them are logged.
    let send = |slug: &str, frame: Vec<u8>| {
        frame_log.record(slug, Direction::Sent, &frame);
        link.outgoing.send(frame).is_ok()
    };
    if !send(slug, samsung::Session::handshake()) {
        return NotAttachedReason::LinkDropped;
    }
    let mut session = samsung::Session::default();
    loop {
        tokio::select! {
            bytes = link.incoming.recv() => {
                let Some(bytes) = bytes else {
                    return NotAttachedReason::LinkDropped;
                };
                for frame in session.deframe(&bytes) {
                    let updates = session.receive(&frame);
                    for update in &updates {
                        if let Update::Identified(model) = update {
                            slug = model.slug();
                        }
                    }
                    frame_log.record(slug, Direction::Received, &frame.raw);
                    for update in updates {
                        let unsupported = update == Update::Unsupported;
                        on_update(update);
                        if unsupported {
                            // Dropping the link Detaches without writing anything more.
                            return NotAttachedReason::UnsupportedModel;
                        }
                    }
                }
            }
            Some(command) = commands.recv() => {
                let frames = match command {
                    SessionCommand::SetNoiseControlMode(mode) => {
                        session.set_noise_control_mode(mode)
                    }
                };
                for frame in frames {
                    if !send(slug, frame) {
                        return NotAttachedReason::LinkDropped;
                    }
                }
            }
        }
    }
}

/// Runs a Sony control link session until the link closes, or until the
/// Device turns out to be an unsupported Model. Returns why it ended.
async fn run_sony_session(
    mut link: ControlLink,
    frame_log: FrameLog,
    mut on_update: impl FnMut(Update),
) -> NotAttachedReason {
    // Frames are logged under the vendor's slug until the Model is identified.
    let mut slug = "sony";
    let mut session = sony::Session::new();
    // Every frame the session sends goes out, and into the log, through here.
    let write = |session: &mut sony::Session, slug: &str| {
        session.outgoing().into_iter().all(|frame| {
            frame_log.record(slug, Direction::Sent, &frame);
            link.outgoing.send(frame).is_ok()
        })
    };
    if !write(&mut session, slug) {
        return NotAttachedReason::LinkDropped;
    }
    while let Some(bytes) = link.incoming.recv().await {
        for frame in session.deframe(&bytes) {
            let updates = session.receive(&frame);
            for update in &updates {
                if let Update::Identified(model) = update {
                    slug = model.slug();
                }
            }
            frame_log.record(slug, Direction::Received, &frame.raw);
            if updates.contains(&Update::Unsupported) {
                on_update(Update::Unsupported);
                // Dropping the link Detaches without writing anything more.
                return NotAttachedReason::UnsupportedModel;
            }
            // Write first, so ACKs are out before the update is reported.
            if !write(&mut session, slug) {
                return NotAttachedReason::LinkDropped;
            }
            updates.into_iter().for_each(&mut on_update);
        }
    }
    NotAttachedReason::LinkDropped
}
