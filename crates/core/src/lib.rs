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
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

pub use command::{Command, CommandError, NoiseControlMode};
pub use device_list::{
    Battery, Device, DeviceList, DeviceState, EffectiveModeReason, NotAttachedReason,
};
use frame_log::{Direction, FrameLog};
pub use model::{Capabilities, LevelRange, Model};
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
        Core::start_logging(platform, FrameLog::default())
    }

    /// Starts the core with the frame log already on, appending to `path`,
    /// so Devices Attached at startup are logged too. The core starts even if
    /// the file can't be opened, with the frame log off.
    pub fn start_with_frame_log(
        platform: impl Platform,
        path: &Path,
    ) -> (Core, Result<(), CommandError>) {
        let frame_log = FrameLog::default();
        let started = frame_log
            .start(path)
            .map_err(CommandError::frame_log_unavailable);
        (Core::start_logging(platform, frame_log), started)
    }

    fn start_logging(platform: impl Platform, frame_log: FrameLog) -> Core {
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let (device_list_tx, device_list) = watch::channel(DeviceList::default());
        let (link_events, link_events_rx) = mpsc::unbounded_channel();
        let state = State {
            platform: Arc::new(platform),
            device_list: device_list_tx,
            devices: BTreeMap::new(),
            link_events,
            next_attempt: 0,
            next_attach: 0,
            audio_output: None,
            frame_log,
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
    /// Orders Attaches, so the most recently Attached Device can be found.
    next_attach: u64,
    /// The Bluetooth Device the OS is playing audio through, if any. It may
    /// not be Attached, or even Connected yet.
    audio_output: Option<DeviceAddress>,
    frame_log: FrameLog,
}

/// A Connected Device and its control link session, if any.
struct Tracked {
    device: Device,
    vendor: Vendor,
    /// Includes a session waiting to retry.
    session: Option<LinkSession>,
    /// Automatic retries since the Device was last Attached.
    retries: u32,
    /// The Device's place in the order of Attaches, while it is Attached.
    attach_order: Option<u64>,
    /// When a Device that is Powering Off, and acknowledged it, is given up
    /// on if it is still Connected.
    power_off_deadline: Option<Instant>,
}

/// How long a Device has to identify itself once its control link is open.
const IDENTIFY_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a Device that acknowledged a Power Off has to stop being
/// Connected before Powering Off ends and the Power Off is reported as failed.
const POWER_OFF_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a Sony request waits for its ACK before it is sent again.
const ACK_TIMEOUT: Duration = Duration::from_secs(1);

/// How many times Attaching is retried automatically before only the retry
/// command can try again.
const MAX_RETRIES: u32 = 6;

/// The pause before the automatic retry that follows `retries` earlier ones:
/// 1 s, doubling up to 30 s.
fn backoff(retries: u32) -> Duration {
    Duration::from_secs(1u64 << retries.min(5)).min(Duration::from_secs(30))
}

/// Rejects `level` unless a Device with the level Capability `range` has it.
fn check_level(range: Option<LevelRange>, level: u8) -> Result<(), CommandError> {
    let range = range.ok_or(CommandError::MissingCapability)?;
    if !range.contains(level) {
        return Err(CommandError::OutOfRange);
    }
    Ok(())
}

impl NotAttachedReason {
    /// Whether trying again by itself might Attach the Device. Not when only
    /// the listener can fix the cause, e.g. by quitting the app holding the
    /// link, and never for an unsupported Model.
    fn is_transient(&self) -> bool {
        match self {
            NotAttachedReason::UnsupportedModel
            | NotAttachedReason::InUseByAnotherProcess
            | NotAttachedReason::BluetoothPermissionDenied => false,
            NotAttachedReason::ServiceNotFound
            | NotAttachedReason::DeviceNotConnected
            | NotAttachedReason::LinkDropped
            | NotAttachedReason::NoResponse
            | NotAttachedReason::Other(_) => true,
        }
    }
}

/// A running control link session task.
struct LinkSession {
    attempt: u64,
    task: JoinHandle<()>,
    commands: mpsc::UnboundedSender<SessionCommand>,
}

impl LinkSession {
    fn send(&self, command: SessionCommand) -> Result<(), CommandError> {
        self.commands
            .send(command)
            .map_err(|_| CommandError::NotAttached)
    }
}

impl Drop for LinkSession {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A command for the Device, already checked against its Capabilities.
// Named after the `Command`s they come from.
#[allow(clippy::enum_variant_names)]
enum SessionCommand {
    SetNoiseControlMode(NoiseControlMode),
    SetAmbientLevel(u8),
    SetNoiseCancellingLevel(u8),
    PowerOff,
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
    /// A session waiting to retry has started opening the link.
    Attaching,
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
            let power_off_deadline = self.power_off_deadline();
            let power_off_timeout = async {
                match power_off_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                () = power_off_timeout => self.power_off_timed_out(),
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
            Observation::AudioOutputChanged(output) => self.audio_output = output,
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
            device: Device::unidentified(identity.address.clone(), identity.name, state),
            vendor,
            session: None,
            retries: 0,
            attach_order: None,
            power_off_deadline: None,
        };
        if attach {
            tracked.session = Some(self.attach(&identity.address, vendor, Duration::ZERO));
        }
        self.devices.insert(identity.address, tracked);
    }

    /// Starts a session that opens the control link after `delay`.
    fn attach(&mut self, address: &DeviceAddress, vendor: Vendor, delay: Duration) -> LinkSession {
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
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
                send(LinkEventKind::Attaching);
            }
            let service = match vendor {
                Vendor::Samsung => samsung::SERVICE,
                Vendor::Sony => sony::SERVICE,
            };
            let identified = AtomicBool::new(false);
            let on_update = |update| {
                if let Update::Identified(_) = update {
                    identified.store(true, Ordering::Relaxed);
                }
                send(LinkEventKind::Update(update));
            };
            let reason = match platform.open_control_link(&address, service).await {
                Ok(link) => {
                    let session = async {
                        match vendor {
                            Vendor::Samsung => {
                                run_samsung_session(link, commands_rx, frame_log, on_update).await
                            }
                            Vendor::Sony => {
                                run_sony_session(link, commands_rx, frame_log, on_update).await
                            }
                        }
                    };
                    // A Device that never identifies itself, e.g. one that
                    // ACKs but never replies, would otherwise be left
                    // Attaching for good.
                    let unidentified = async {
                        tokio::time::sleep(IDENTIFY_TIMEOUT).await;
                        if identified.load(Ordering::Relaxed) {
                            std::future::pending().await
                        }
                    };
                    tokio::select! {
                        reason = session => reason,
                        () = unidentified => NotAttachedReason::NoResponse,
                    }
                }
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
            LinkEventKind::Attaching => device.state = DeviceState::Attaching,
            LinkEventKind::Update(Update::Identified(model)) => {
                device.model = Some(model);
                device.capabilities = model.capabilities();
                device.state = DeviceState::Attached;
                tracked.retries = 0;
                tracked.attach_order = Some(self.next_attach);
                self.next_attach += 1;
            }
            // The session ends straight after, with that reason.
            LinkEventKind::Update(Update::Unsupported) => {}
            LinkEventKind::Update(Update::Battery(battery)) => {
                device.battery = Some(battery);
            }
            LinkEventKind::Update(Update::ChosenMode(mode)) => {
                device.chosen_mode = Some(mode);
                device.set_effective_mode(mode, None);
            }
            LinkEventKind::Update(Update::EffectiveMode(mode, reason)) => {
                device.set_effective_mode(mode, reason);
            }
            LinkEventKind::Update(Update::AmbientLevel(level)) => {
                device.ambient_level = Some(level);
            }
            LinkEventKind::Update(Update::NoiseCancellingLevel(level)) => {
                device.noise_cancelling_level = Some(level);
            }
            LinkEventKind::Update(Update::PowerOffAcknowledged) => {
                if device.state == DeviceState::PoweringOff {
                    tracked.power_off_deadline = Some(Instant::now() + POWER_OFF_TIMEOUT);
                }
            }
            // The control link closing is part of switching off: the Device
            // stays Powering Off until it stops being Connected.
            LinkEventKind::Ended(_) if device.state == DeviceState::PoweringOff => {
                tracked.session = None;
            }
            LinkEventKind::Ended(reason) => {
                let retry = reason.is_transient() && tracked.retries < MAX_RETRIES;
                *device = Device::unidentified(
                    device.address.clone(),
                    device.name.clone(),
                    DeviceState::NotAttached { reason },
                );
                tracked.session = None;
                tracked.attach_order = None;
                if retry {
                    let delay = backoff(tracked.retries);
                    tracked.retries += 1;
                    let vendor = tracked.vendor;
                    let session = self.attach(&event.address, vendor, delay);
                    let tracked = self.devices.get_mut(&event.address).expect("tracked");
                    tracked.session = Some(session);
                }
            }
        }
    }

    fn command(&mut self, command: Command) -> Result<(), CommandError> {
        match command {
            Command::SetNoiseControlMode(address, mode) => {
                let (device, session) = self.attached(&address)?;
                if !device.capabilities.noise_control_modes.contains(&mode) {
                    return Err(CommandError::MissingCapability);
                }
                device.settings_reported()?;
                session.send(SessionCommand::SetNoiseControlMode(mode))
            }
            Command::SetAmbientLevel(address, level) => {
                let (device, session) = self.attached(&address)?;
                check_level(device.capabilities.ambient_level, level)?;
                device.settings_reported()?;
                session.send(SessionCommand::SetAmbientLevel(level))
            }
            Command::SetNoiseCancellingLevel(address, level) => {
                let (device, session) = self.attached(&address)?;
                check_level(device.capabilities.noise_cancelling_level, level)?;
                device.settings_reported()?;
                session.send(SessionCommand::SetNoiseCancellingLevel(level))
            }
            Command::PowerOff(address) => {
                let (device, session) = self.attached(&address)?;
                if !device.capabilities.power_off {
                    return Err(CommandError::MissingCapability);
                }
                session.send(SessionCommand::PowerOff)?;
                let tracked = self.devices.get_mut(&address).expect("checked above");
                tracked.device.state = DeviceState::PoweringOff;
                tracked.device.power_off_failed = false;
                Ok(())
            }
            Command::RetryAttach(address) => {
                let tracked = self.supported(&address)?;
                if !matches!(tracked.device.state, DeviceState::NotAttached { .. }) {
                    return Ok(());
                }
                // Retrying by hand starts over, straight away.
                let session = self.attach(&address, tracked.vendor, Duration::ZERO);
                let tracked = self.devices.get_mut(&address).expect("checked above");
                tracked.device.state = DeviceState::Attaching;
                tracked.session = Some(session);
                tracked.retries = 0;
                Ok(())
            }
            Command::SetFrameLog(Some(path)) => self
                .frame_log
                .start(&path)
                .map_err(CommandError::frame_log_unavailable),
            Command::SetFrameLog(None) => {
                self.frame_log.stop();
                Ok(())
            }
        }
    }

    /// The earliest moment a Device that is Powering Off is given up on.
    fn power_off_deadline(&self) -> Option<Instant> {
        self.devices
            .values()
            .filter_map(|tracked| tracked.power_off_deadline)
            .min()
    }

    /// Ends Powering Off for every Device still Connected past its deadline,
    /// and reports that it didn't Power Off.
    fn power_off_timed_out(&mut self) {
        let now = Instant::now();
        for tracked in self.devices.values_mut() {
            if tracked
                .power_off_deadline
                .is_none_or(|deadline| deadline > now)
            {
                continue;
            }
            tracked.power_off_deadline = None;
            tracked.device.state = DeviceState::Attached;
            tracked.device.power_off_failed = true;
        }
    }

    /// The Device at `address`, unless it's unknown, an unsupported Model, or
    /// Powering Off.
    fn supported(&self, address: &DeviceAddress) -> Result<&Tracked, CommandError> {
        let tracked = self
            .devices
            .get(address)
            .ok_or(CommandError::UnknownDevice)?;
        match tracked.device.state {
            DeviceState::NotAttached {
                reason: NotAttachedReason::UnsupportedModel,
            } => Err(CommandError::UnsupportedModel),
            DeviceState::PoweringOff => Err(CommandError::PoweringOff),
            _ => Ok(tracked),
        }
    }

    /// The Attached Device at `address` and its control link session.
    fn attached(&self, address: &DeviceAddress) -> Result<(&Device, &LinkSession), CommandError> {
        let tracked = self.supported(address)?;
        match (&tracked.device.state, &tracked.session) {
            (DeviceState::Attached, Some(session)) => Ok((&tracked.device, session)),
            _ => Err(CommandError::NotAttached),
        }
    }

    /// The Attached Device playing audio, or else the most recently Attached
    /// Device.
    fn primary(&self) -> Option<DeviceAddress> {
        let playing = self.audio_output.as_ref().filter(|address| {
            self.devices
                .get(address)
                .is_some_and(|t| t.attach_order.is_some())
        });
        if let Some(address) = playing {
            return Some(address.clone());
        }
        self.devices
            .values()
            .filter_map(|tracked| Some((tracked.attach_order?, &tracked.device.address)))
            .max()
            .map(|(_, address)| address.clone())
    }

    fn publish(&self) {
        let snapshot = DeviceList {
            devices: self
                .devices
                .values()
                .map(|tracked| tracked.device.clone())
                .collect(),
            primary: self.primary(),
        };
        self.device_list.send_if_modified(|list| {
            if *list == snapshot {
                return false;
            }
            *list = snapshot;
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
                    SessionCommand::SetAmbientLevel(level) => session.set_ambient_level(level),
                    SessionCommand::SetNoiseCancellingLevel(level) => {
                        session.set_noise_cancelling_level(level)
                    }
                    // No Samsung Model can Power Off, so the command is
                    // rejected before it gets here.
                    SessionCommand::PowerOff => {
                        debug_assert!(false, "no Samsung Model can Power Off");
                        Vec::new()
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
    mut commands: mpsc::UnboundedReceiver<SessionCommand>,
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
    // A request whose ACK is lost is sent again every second, and a Device
    // that acknowledges none of the tries is given up on, like a dropped link
    // but retried as one that didn't respond.
    let ack_timeout = tokio::time::sleep(ACK_TIMEOUT);
    tokio::pin!(ack_timeout);
    let mut awaiting_ack = None;
    loop {
        if session.awaiting_ack() != awaiting_ack {
            awaiting_ack = session.awaiting_ack();
            ack_timeout
                .as_mut()
                .reset(tokio::time::Instant::now() + ACK_TIMEOUT);
        }
        tokio::select! {
            () = &mut ack_timeout, if awaiting_ack.is_some() => {
                if !session.resend() {
                    return NotAttachedReason::NoResponse;
                }
                if !write(&mut session, slug) {
                    return NotAttachedReason::LinkDropped;
                }
            }
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
            Some(command) = commands.recv() => {
                match command {
                    SessionCommand::SetNoiseControlMode(mode) => {
                        session.set_noise_control_mode(mode)
                    }
                    SessionCommand::SetAmbientLevel(level) => session.set_ambient_level(level),
                    // No Sony Model has a Noise Cancelling Level, so the
                    // command is rejected before it gets here.
                    SessionCommand::SetNoiseCancellingLevel(_) => {
                        debug_assert!(false, "no Sony Model has a Noise Cancelling Level");
                    }
                    SessionCommand::PowerOff => session.power_off(),
                }
                if !write(&mut session, slug) {
                    return NotAttachedReason::LinkDropped;
                }
            }
        }
    }
}
