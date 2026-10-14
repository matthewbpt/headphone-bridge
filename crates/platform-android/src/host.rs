//! The core's host: the core with the On request policy, and the platform
//! port it talks to, which is Kotlin at the other end of the JSON messages.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use headphone_bridge_core::platform::{
    ControlLink, DeviceAddress, LinkFailure, Observation, Observations, Platform,
};
#[cfg(debug_assertions)]
use headphone_bridge_core::Model;

#[cfg(debug_assertions)]
use crate::simulated::SimulatedDevice;
use headphone_bridge_core::{AttachPolicy, Core, Hold};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::messages::{
    parse_command, parse_event, BadMessage, CommandFailure, Event, HoldKind, Message,
};

/// Sends a message to Kotlin. False if it couldn't be delivered, e.g. before
/// Kotlin has connected.
type ToKotlin = Box<dyn Fn(Message) -> bool + Send + Sync>;

/// A control link Kotlin opened: its id and the bytes it reads.
type Opened = Result<(i32, mpsc::UnboundedReceiver<Vec<u8>>), LinkFailure>;

/// The core and its side of the JSON boundary.
pub struct Host {
    core: Arc<Core>,
    runtime: Handle,
    shared: Arc<Shared>,
    /// The holds in place, at most one of each kind.
    holds: Mutex<HashMap<HoldKind, Hold>>,
}

/// What the host and its platform port share.
struct Shared {
    kotlin: ToKotlin,
    observations: mpsc::UnboundedSender<Observation>,
    /// Opens Kotlin hasn't answered yet, by request.
    opening: Mutex<HashMap<u64, oneshot::Sender<Opened>>>,
    next_request: AtomicU64,
    /// Where each open link's bytes go, by Kotlin's link id.
    links: Mutex<HashMap<i32, mpsc::UnboundedSender<Vec<u8>>>>,
    /// The Simulated Device's Model while it's switched on.
    #[cfg(debug_assertions)]
    simulated: Mutex<Option<SimulatedDevice>>,
}

impl Shared {
    fn send(&self, message: Message) -> bool {
        (self.kotlin)(message)
    }

    /// Switches the Simulated Device to `model` if `from` approves of the
    /// one simulated now, Disconnecting that one and Connecting `model`.
    #[cfg(debug_assertions)]
    fn simulate(&self, model: Option<Model>, from: impl FnOnce(Option<Model>) -> bool) {
        let mut simulated = self.simulated.lock().unwrap();
        let now = simulated.as_ref().map(SimulatedDevice::model);
        if now == model || !from(now) {
            return;
        }
        if let Some(previous) = simulated.take() {
            let address = previous.identity().address;
            let _ = self
                .observations
                .send(Observation::DeviceDisconnected(address));
        }
        if let Some(model) = model {
            let device = SimulatedDevice::new(model);
            let _ = self
                .observations
                .send(Observation::DeviceConnected(device.identity()));
            *simulated = Some(device);
        }
    }
}

impl Host {
    /// Starts the core on the current Tokio runtime, Detached until a hold,
    /// sending every message for Kotlin to `kotlin`.
    pub fn start(kotlin: impl Fn(Message) -> bool + Send + Sync + 'static) -> Host {
        let (observations, observations_rx) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            kotlin: Box::new(kotlin),
            observations,
            opening: Mutex::default(),
            next_request: AtomicU64::new(1),
            links: Mutex::default(),
            #[cfg(debug_assertions)]
            simulated: Mutex::default(),
        });
        let core = Core::start_with(
            KotlinBluetooth {
                shared: shared.clone(),
                observations: Mutex::new(Some(observations_rx)),
            },
            AttachPolicy::OnRequest,
        );
        let mut snapshots = core.subscribe();
        let forwarding = shared.clone();
        tokio::spawn(async move {
            while snapshots.changed().await.is_ok() {
                let list = snapshots.borrow_and_update().clone();
                forwarding.send(Message::DeviceList { list });
            }
        });
        Host {
            core: Arc::new(core),
            runtime: Handle::current(),
            shared,
            holds: Mutex::default(),
        }
    }

    /// The core, for the app screen.
    pub fn core(&self) -> &Core {
        &self.core
    }

    /// Sends Kotlin the latest Device list, e.g. once it connects, since
    /// earlier ones went nowhere.
    pub fn resend_device_list(&self) {
        self.shared.send(Message::DeviceList {
            list: self.core.device_list(),
        });
    }

    /// Switches the Simulated Device on as `model`, or off with `None`. It's
    /// Connected while on, and Attached and Detached like a real Device.
    #[cfg(debug_assertions)]
    pub fn simulate(&self, model: Option<Model>) {
        self.shared.simulate(model, |_| true);
    }

    /// The Simulated Device's Model while it's switched on.
    #[cfg(debug_assertions)]
    pub fn simulated(&self) -> Option<Model> {
        let simulated = self.shared.simulated.lock().unwrap();
        simulated.as_ref().map(SimulatedDevice::model)
    }

    /// Runs a command Kotlin numbered `request`, and answers with a
    /// `commandResult` once the core accepts or rejects it. Never blocks.
    pub fn command(&self, request: i64, json: &str) {
        let command = parse_command(json);
        let core = self.core.clone();
        let shared = self.shared.clone();
        self.runtime.spawn(async move {
            let result = match command {
                Ok(command) => core
                    .command(command)
                    .await
                    .map_err(CommandFailure::Rejected),
                Err(bad) => Err(CommandFailure::BadRequest(bad)),
            };
            shared.send(Message::CommandResult { request, result });
        });
    }

    /// Handles an event from Kotlin. Never blocks.
    pub fn event(&self, json: &str) -> Result<(), BadMessage> {
        let shared = &self.shared;
        match parse_event(json)? {
            Event::Observed(observation) => {
                let _ = shared.observations.send(observation);
            }
            Event::Opened { request, link } => {
                let (incoming_tx, incoming) = mpsc::unbounded_channel();
                shared.links.lock().unwrap().insert(link, incoming_tx);
                let waiter = shared.opening.lock().unwrap().remove(&request);
                let delivered =
                    waiter.is_some_and(|waiter| waiter.send(Ok((link, incoming))).is_ok());
                if !delivered {
                    // The core stopped waiting for it, e.g. its hold ended.
                    shared.links.lock().unwrap().remove(&link);
                    shared.send(Message::Close { link });
                }
            }
            Event::OpenFailed { request, failure } => {
                if let Some(waiter) = shared.opening.lock().unwrap().remove(&request) {
                    let _ = waiter.send(Err(failure));
                }
            }
            Event::Data { link, bytes } => {
                if let Some(incoming) = shared.links.lock().unwrap().get(&link) {
                    let _ = incoming.send(bytes);
                }
            }
            Event::Closed { link } => {
                // Dropping the sender tells the core the link dropped.
                shared.links.lock().unwrap().remove(&link);
            }
            Event::Hold(kind) => {
                // A second hold of a kind already in place changes nothing.
                if let Entry::Vacant(entry) = self.holds.lock().unwrap().entry(kind) {
                    let hold = match kind {
                        HoldKind::All => Some(self.core.hold_all()),
                        // No Primary Device, nothing to hold.
                        HoldKind::Primary => self.core.hold_primary(),
                    };
                    if let Some(hold) = hold {
                        entry.insert(hold);
                    }
                }
            }
            Event::EndHold(kind) => {
                self.holds.lock().unwrap().remove(&kind);
            }
        }
        Ok(())
    }
}

/// The core's platform port, with Kotlin's `Hub` behind it.
struct KotlinBluetooth {
    shared: Arc<Shared>,
    observations: Mutex<Option<Observations>>,
}

impl Platform for KotlinBluetooth {
    fn observations(&self) -> Observations {
        self.observations
            .lock()
            .unwrap()
            .take()
            .expect("the core takes its observations once")
    }

    async fn open_control_link(
        &self,
        address: &DeviceAddress,
        service: Uuid,
    ) -> Result<ControlLink, LinkFailure> {
        let shared = &self.shared;
        #[cfg(debug_assertions)]
        if let Some(device) = &*shared.simulated.lock().unwrap() {
            if device.is_at(address) {
                // Powering Off switches it off, as it stops being Connected.
                let model = device.model();
                let shared = shared.clone();
                let power_off = move || shared.simulate(None, |now| now == Some(model));
                return Ok(device.open(power_off));
            }
        }
        let request = shared.next_request.fetch_add(1, Ordering::Relaxed);
        let (opened_tx, opened) = oneshot::channel();
        shared.opening.lock().unwrap().insert(request, opened_tx);
        let asked = shared.send(Message::Open {
            request,
            address: address.clone(),
            service,
        });
        if !asked {
            shared.opening.lock().unwrap().remove(&request);
            return Err(LinkFailure::Other("Bluetooth isn't ready yet".into()));
        }
        let (link, incoming) = opened
            .await
            .map_err(|_| LinkFailure::Other("the open was abandoned".into()))??;
        let (outgoing, mut outgoing_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let shared = shared.clone();
        tokio::spawn(async move {
            while let Some(bytes) = outgoing_rx.recv().await {
                shared.send(Message::Write { link, bytes });
            }
            // The core is done with the link.
            shared.links.lock().unwrap().remove(&link);
            shared.send(Message::Close { link });
        });
        Ok(ControlLink { incoming, outgoing })
    }
}
