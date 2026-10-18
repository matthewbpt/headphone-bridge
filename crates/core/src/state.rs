//! The State actor: tracks Connected Devices, Attaches and Detaches them
//! as the Attach Policy and holds ask, and publishes the Device list.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::command::{Command, CommandError};
use crate::device_list::{Device, DeviceList, DeviceState, NotAttachedReason};
use crate::frame_log::FrameLog;
use crate::platform::{DeviceAddress, DeviceIdentity, Observation, Platform};
use crate::session::{run_samsung_session, run_sony_session, SessionCommand};
use crate::update::Update;
use crate::{samsung, sony, AttachPolicy, Model};

/// What the core is asked for, in the order it's asked.
pub(crate) enum Request {
    Command(Command, Reply),
    /// A hold on the Device at the address, or on every Connected Device.
    Hold(u64, Option<DeviceAddress>),
    EndHold(u64),
}

/// Where a command's acceptance or rejection is sent.
pub(crate) type Reply = oneshot::Sender<Result<(), CommandError>>;

/// Starts the State actor on the current Tokio runtime, serving `requests`
/// and publishing to `device_list`.
pub(crate) fn spawn(
    platform: impl Platform,
    policy: AttachPolicy,
    device_list: watch::Sender<DeviceList>,
    frame_log: FrameLog,
    requests: mpsc::UnboundedReceiver<Request>,
) {
    let (link_events, link_events_rx) = mpsc::unbounded_channel();
    let state = State {
        platform: Arc::new(platform),
        policy,
        device_list,
        devices: BTreeMap::new(),
        link_events,
        next_attempt: 0,
        next_recency: 0,
        audio_output: None,
        frame_log,
        holds: BTreeMap::new(),
    };
    tokio::spawn(state.run(requests, link_events_rx));
}

struct State<P> {
    platform: Arc<P>,
    policy: AttachPolicy,
    device_list: watch::Sender<DeviceList>,
    devices: BTreeMap<DeviceAddress, Tracked>,
    link_events: mpsc::UnboundedSender<LinkEvent>,
    /// Tells apart link sessions, so events from a session that has been
    /// replaced are ignored.
    next_attempt: u64,
    /// Orders Connects and Attaches, so the most recently Attached or shown
    /// Device can be found.
    next_recency: u64,
    /// The Bluetooth Device the OS is playing audio through, if any. It may
    /// not be Attached, or even Connected yet.
    audio_output: Option<DeviceAddress>,
    frame_log: FrameLog,
    /// The holds in place, by id: each on one Device, or (`None`) on every
    /// Connected Device.
    holds: BTreeMap<u64, Option<DeviceAddress>>,
}

/// A Connected Device and its control link session, if any.
struct Tracked {
    device: Device,
    /// `None` for an unsupported Model, which no session runs with.
    vendor: Option<Vendor>,
    /// Includes a session waiting to retry.
    session: Option<LinkSession>,
    /// Automatic retries since the Device was last Attached.
    retries: u32,
    /// When the Device was last shown or Attached, in the order of those.
    recency: u64,
    /// When the Device last reported something, in milliseconds since the
    /// Unix epoch. `None` once what it reported is dropped.
    last_report: Option<u64>,
    /// When a Device that is Powering Off, and acknowledged it or closed its
    /// control link, is given up on if it is still Connected.
    power_off_deadline: Option<Instant>,
}

impl Tracked {
    /// Detaches the Device on purpose, closing its control link. It keeps
    /// what it last reported, for the rest of its Connected spell, marked
    /// with when it reported it.
    fn detach(&mut self) {
        self.device.state = DeviceState::Detached;
        self.device.reported_at = self.last_report;
        self.end_session();
    }

    /// Closes the Device's control link, or forgets it after it dropped, and
    /// puts the Device in `state`, dropping what it reported.
    fn lose_link(&mut self, state: DeviceState) {
        self.device = self.device.detached(state);
        self.last_report = None;
        self.end_session();
    }

    fn end_session(&mut self) {
        self.session = None;
        self.power_off_deadline = None;
    }
}

/// The wall-clock time now, in milliseconds since the Unix epoch.
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// How long a Device has to identify itself once its control link is open.
const IDENTIFY_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a Device that acknowledged a Power Off, or closed its control
/// link first, has to stop being Connected before Powering Off ends and the Power Off is reported as failed.
const POWER_OFF_TIMEOUT: Duration = Duration::from_secs(10);

/// How many times Attaching is retried automatically before only the retry
/// command can try again.
const MAX_RETRIES: u32 = 6;

/// The pause before the automatic retry that follows `retries` earlier ones:
/// 1 s, doubling up to 30 s.
fn backoff(retries: u32) -> Duration {
    Duration::from_secs(1u64 << retries.min(5)).min(Duration::from_secs(30))
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

#[derive(Debug, Clone, Copy)]
enum Vendor {
    /// With the Model its OS name points to (ADR-0003).
    Samsung(Model),
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
        mut requests: mpsc::UnboundedReceiver<Request>,
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
                request = requests.recv() => match request {
                    Some(Request::Command(command, reply)) => {
                        let _ = reply.send(self.command(command));
                    }
                    Some(Request::Hold(id, device)) => {
                        self.holds.insert(id, device);
                        self.follow_holds();
                    }
                    Some(Request::EndHold(id)) => {
                        self.holds.remove(&id);
                        self.follow_holds();
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
        // A Sony Device reveals its Model in reply to reads alone, so every one
        // is Attached to identify it. A Samsung Device needs a write first, so
        // only those whose name points at the allowlist are (ADR-0003).
        let vendor = if has_service(samsung::SERVICE) {
            samsung::candidate_model(&identity.name).map(Vendor::Samsung)
        } else if has_service(sony::SERVICE) {
            Some(Vendor::Sony)
        } else {
            return;
        };
        let state = if vendor.is_none() {
            DeviceState::NotAttached {
                reason: NotAttachedReason::UnsupportedModel,
            }
        } else if self.wants_attached(&identity.address) {
            DeviceState::Attaching
        } else {
            DeviceState::Detached
        };
        let state_is_attaching = state == DeviceState::Attaching;
        let address = identity.address.clone();
        self.next_recency += 1;
        let tracked = Tracked {
            device: Device::unidentified(identity.address, identity.name, state),
            vendor,
            session: None,
            retries: 0,
            recency: self.next_recency,
            last_report: None,
            power_off_deadline: None,
        };
        self.devices.insert(address.clone(), tracked);
        if state_is_attaching {
            self.reattach(&address, Duration::ZERO);
        }
    }

    /// Starts a new session for the tracked Device at `address` that opens
    /// the control link after `delay`, in place of any session it had.
    fn reattach(&mut self, address: &DeviceAddress, delay: Duration) {
        let Some(vendor) = self.devices.get(address).and_then(|tracked| tracked.vendor) else {
            return;
        };
        let session = self.attach(address, vendor, delay);
        if let Some(tracked) = self.devices.get_mut(address) {
            tracked.session = Some(session);
        }
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
                Vendor::Samsung(_) => samsung::SERVICE,
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
                            Vendor::Samsung(candidate) => {
                                run_samsung_session(
                                    link,
                                    candidate,
                                    commands_rx,
                                    frame_log,
                                    on_update,
                                )
                                .await
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
        if let LinkEventKind::Update(_) = event.kind {
            tracked.last_report = Some(now_millis());
        }
        let device = &mut tracked.device;
        match event.kind {
            LinkEventKind::Attaching => device.state = DeviceState::Attaching,
            LinkEventKind::Update(Update::Identified(model)) => {
                device.forget_reported();
                device.model = Some(model);
                device.capabilities = model.capabilities();
                device.state = DeviceState::Attached;
                tracked.retries = 0;
                tracked.recency = self.next_recency;
                self.next_recency += 1;
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
            LinkEventKind::Update(Update::Equalizer(equalizer)) => {
                device.equalizer = Some(equalizer);
            }
            LinkEventKind::Update(Update::EqualizerUnavailable(unavailable)) => {
                device.equalizer_unavailable = unavailable;
            }
            LinkEventKind::Update(Update::PowerOffAcknowledged) => {
                if device.state == DeviceState::PoweringOff {
                    tracked.power_off_deadline = Some(Instant::now() + POWER_OFF_TIMEOUT);
                }
            }
            // The control link closing is part of switching off: the Device
            // stays Powering Off until it stops being Connected, or until the
            // deadline: a link that closes before the ACK stands in for it.
            // A Power Off the Device never acknowledged fails like any other
            // request.
            LinkEventKind::Ended(ref reason)
                if device.state == DeviceState::PoweringOff
                    && *reason != NotAttachedReason::NoResponse =>
            {
                tracked.session = None;
                tracked
                    .power_off_deadline
                    .get_or_insert(Instant::now() + POWER_OFF_TIMEOUT);
            }
            LinkEventKind::Ended(reason) => {
                let retry = reason.is_transient() && tracked.retries < MAX_RETRIES;
                tracked.lose_link(DeviceState::NotAttached { reason });
                if retry {
                    let delay = backoff(tracked.retries);
                    tracked.retries += 1;
                    self.reattach(&event.address, delay);
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
                device.capabilities.check_ambient_level(level)?;
                device.settings_reported()?;
                session.send(SessionCommand::SetAmbientLevel(level))
            }
            Command::SetNoiseCancellingLevel(address, level) => {
                let (device, session) = self.attached(&address)?;
                device.capabilities.check_noise_cancelling_level(level)?;
                device.settings_reported()?;
                session.send(SessionCommand::SetNoiseCancellingLevel(level))
            }
            Command::SetEqualizer(address, equalizer) => {
                let (device, session) = self.attached(&address)?;
                // Before anything else: the Device takes no Equalizer change.
                if device.equalizer_unavailable {
                    return Err(CommandError::EqualizerUnavailable);
                }
                device.capabilities.check_equalizer(&equalizer)?;
                if device.equalizer.is_none() {
                    return Err(CommandError::SettingsNotReported);
                }
                session.send(SessionCommand::SetEqualizer(equalizer))
            }
            Command::PowerOff(address) => {
                let (device, session) = self.attached(&address)?;
                if !device.capabilities.power_off {
                    return Err(CommandError::MissingCapability);
                }
                session.send(SessionCommand::PowerOff)?;
                let tracked = self
                    .devices
                    .get_mut(&address)
                    .ok_or(CommandError::UnknownDevice)?;
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
                let tracked = self
                    .devices
                    .get_mut(&address)
                    .ok_or(CommandError::UnknownDevice)?;
                tracked.device.state = DeviceState::Attaching;
                tracked.retries = 0;
                self.reattach(&address, Duration::ZERO);
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

    /// Whether the policy, or a hold, asks for the Device at `address` to be
    /// Attached.
    fn wants_attached(&self, address: &DeviceAddress) -> bool {
        self.policy == AttachPolicy::Always
            || self
                .holds
                .values()
                .any(|held| held.as_ref().is_none_or(|held| held == address))
    }

    /// Attaches every Detached Device the policy or a hold now asks for, and
    /// Detaches every other Device.
    fn follow_holds(&mut self) {
        let wanted: BTreeSet<DeviceAddress> = self
            .devices
            .keys()
            .filter(|address| self.wants_attached(address))
            .cloned()
            .collect();
        let mut to_attach = Vec::new();
        for (address, tracked) in &mut self.devices {
            let wanted = wanted.contains(address);
            match tracked.device.state {
                DeviceState::Detached if wanted => {
                    tracked.device.state = DeviceState::Attaching;
                    tracked.retries = 0;
                    to_attach.push(address.clone());
                }
                // A Device Powering Off is left to finish, or to fail and
                // be Detached then.
                DeviceState::Detached | DeviceState::PoweringOff => {}
                ref state if state.is_unsupported_model() => {}
                _ if !wanted => tracked.detach(),
                _ => {}
            }
        }
        for address in to_attach {
            self.reattach(&address, Duration::ZERO);
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
    /// A Device whose control link is still open is Attached again; one
    /// whose link closed is Attached afresh, as a newly Connected Device is.
    /// Either is Detached instead if nothing asks for it to be Attached now.
    fn power_off_timed_out(&mut self) {
        let now = Instant::now();
        let timed_out: Vec<DeviceAddress> = self
            .devices
            .iter()
            .filter(|(_, tracked)| tracked.power_off_deadline.is_some_and(|d| d <= now))
            .map(|(address, _)| address.clone())
            .collect();
        for address in timed_out {
            let wanted = self.wants_attached(&address);
            let Some(tracked) = self.devices.get_mut(&address) else {
                continue;
            };
            let link_open = tracked.session.is_some();
            let reattach = wanted && !link_open;
            if !wanted {
                tracked.detach();
            } else if link_open {
                tracked.device.state = DeviceState::Attached;
                tracked.power_off_deadline = None;
            } else {
                tracked.lose_link(DeviceState::Attaching);
                tracked.retries = 0;
            }
            tracked.device.power_off_failed = true;
            if reattach {
                self.reattach(&address, Duration::ZERO);
            }
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

    /// The Connected Device playing audio, or else the most recently
    /// Attached or shown one, whether or not it's Attached now. Never an
    /// unsupported Model.
    fn primary(&self) -> Option<DeviceAddress> {
        let candidates = || {
            self.devices
                .values()
                .filter(|tracked| !tracked.device.state.is_unsupported_model())
        };
        let playing = candidates()
            .find(|tracked| Some(&tracked.device.address) == self.audio_output.as_ref());
        playing
            .or_else(|| candidates().max_by_key(|tracked| tracked.recency))
            .map(|tracked| tracked.device.address.clone())
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
