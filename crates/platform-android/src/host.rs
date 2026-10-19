//! The core's host: the core with the On request Attach Policy, and the platform
//! port it talks to, which is Kotlin at the other end of the JSON messages.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use headphone_bridge_core::platform::{
    ControlLink, DeviceAddress, LinkFailure, Observation, Platform,
};
use headphone_bridge_core::{AttachPolicy, Core, CoreBuilder, Hold};
#[cfg(debug_assertions)]
use headphone_bridge_simulated::Simulator;
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
    /// Switches the Simulated Device in front of Kotlin's Bluetooth.
    #[cfg(debug_assertions)]
    simulator: Simulator,
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
}

impl Shared {
    fn send(&self, message: Message) -> bool {
        (self.kotlin)(message)
    }
}

impl Host {
    /// Starts the core from `core` on the current Tokio runtime, Detached
    /// until a hold, whatever Attach Policy `core` has, sending every message
    /// for Kotlin to `kotlin`. In debug builds Kotlin's Bluetooth is wrapped
    /// in the Simulated Device.
    pub fn start(
        kotlin: impl Fn(Message) -> bool + Send + Sync + 'static,
        core: CoreBuilder,
    ) -> Host {
        let (observations, observations_rx) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            kotlin: Box::new(kotlin),
            observations,
            opening: Mutex::default(),
            next_request: AtomicU64::new(1),
            links: Mutex::default(),
        });
        let platform = KotlinBluetooth {
            shared: shared.clone(),
        };
        #[cfg(debug_assertions)]
        let (platform, observations_rx, simulator) =
            headphone_bridge_simulated::wrap(platform, observations_rx);
        let core = core
            .attach_policy(AttachPolicy::OnRequest)
            .start(platform, observations_rx);
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
            #[cfg(debug_assertions)]
            simulator,
        }
    }

    /// The core, for the App Screen.
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

    /// Switches the Simulated Device, for the shell to manage.
    #[cfg(debug_assertions)]
    pub fn simulator(&self) -> &Simulator {
        &self.simulator
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
                shared.observations.send(observation).ok();
            }
            Event::Opened { request, link } => {
                let (incoming_tx, incoming) = mpsc::unbounded_channel();
                shared
                    .links
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(link, incoming_tx);
                let waiter = shared
                    .opening
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&request);
                let delivered =
                    waiter.is_some_and(|waiter| waiter.send(Ok((link, incoming))).is_ok());
                if !delivered {
                    // The core stopped waiting for it, e.g. its hold ended.
                    shared
                        .links
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .remove(&link);
                    shared.send(Message::Close { link });
                }
            }
            Event::OpenFailed { request, failure } => {
                if let Some(waiter) = shared
                    .opening
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&request)
                {
                    waiter.send(Err(failure)).ok();
                }
            }
            Event::Data { link, bytes } => {
                if let Some(incoming) = shared
                    .links
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(&link)
                {
                    incoming.send(bytes).ok();
                }
            }
            Event::Closed { link } => {
                // Dropping the sender tells the core the link dropped.
                shared
                    .links
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&link);
            }
            Event::Hold(kind) => {
                // A second hold of a kind already in place changes nothing.
                if let Entry::Vacant(entry) = self
                    .holds
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .entry(kind)
                {
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
                self.holds
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&kind);
            }
        }
        Ok(())
    }
}

/// The core's platform port, with Kotlin's `Hub` behind it.
struct KotlinBluetooth {
    shared: Arc<Shared>,
}

impl Platform for KotlinBluetooth {
    async fn open_control_link(
        &self,
        address: DeviceAddress,
        service: Uuid,
    ) -> Result<ControlLink, LinkFailure> {
        let shared = &self.shared;
        let request = shared.next_request.fetch_add(1, Ordering::Relaxed);
        let (opened_tx, opened) = oneshot::channel();
        shared
            .opening
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(request, opened_tx);
        let asked = shared.send(Message::Open {
            request,
            address,
            service,
        });
        if !asked {
            shared
                .opening
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&request);
            // Kotlin hasn't connected yet.
            return Err(LinkFailure::BluetoothUnavailable);
        }
        let (link, incoming) = opened.await.map_err(|_| {
            log(&format!("the open of {address} was abandoned"));
            LinkFailure::BluetoothHandlingStopped
        })??;
        let (outgoing, mut outgoing_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let shared = shared.clone();
        tokio::spawn(async move {
            while let Some(bytes) = outgoing_rx.recv().await {
                shared.send(Message::Write { link, bytes });
            }
            // The core is done with the link.
            shared
                .links
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&link);
            shared.send(Message::Close { link });
        });
        Ok(ControlLink { incoming, outgoing })
    }
}

/// Logs to logcat on Android, and to stderr where the host is tested.
fn log(message: &str) {
    #[cfg(target_os = "android")]
    crate::android::log(message);
    #[cfg(not(target_os = "android"))]
    eprintln!("{message}");
}
