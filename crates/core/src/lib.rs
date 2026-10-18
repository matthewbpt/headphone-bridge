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
mod session;
mod sony;
mod state;
mod update;
#[doc(hidden)]
pub mod wire;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{mpsc, oneshot, watch};

pub use command::{Command, CommandError, Equalizer, NoiseControlMode};
pub use device_list::{
    Battery, Device, DeviceList, DeviceState, EffectiveModeReason, NotAttachedReason,
};
use frame_log::FrameLog;
pub use model::{
    Bands, Capabilities, EqualizerCapability, LevelRange, Model, Preset, PresetId, StepRange, Steps,
};
use platform::{DeviceAddress, Platform};
use state::Request;

/// A running core. Dropping it stops the core.
pub struct Core {
    requests: mpsc::UnboundedSender<Request>,
    device_list: watch::Receiver<DeviceList>,
    next_hold: AtomicU64,
}

/// A hold on every Connected Device, or on one: while any is in place on a
/// Device, the core Attaches it, whatever its [`AttachPolicy`]. Dropping it
/// ends the hold.
#[must_use = "the hold ends when it's dropped"]
pub struct Hold {
    id: u64,
    device: Option<DeviceAddress>,
    requests: mpsc::UnboundedSender<Request>,
}

impl Hold {
    /// The Device held, for a hold on the Primary Device; `None` for a hold on
    /// every Connected Device.
    pub fn device(&self) -> Option<&DeviceAddress> {
        self.device.as_ref()
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        // A stopped core holds nothing.
        let _ = self.requests.send(Request::EndHold(self.id));
    }
}

impl Core {
    /// Starts the core on the current Tokio runtime, Attaching every
    /// Connected Device.
    pub fn start(platform: impl Platform) -> Core {
        Core::start_with(platform, AttachPolicy::Always)
    }

    /// Starts the core on the current Tokio runtime with `policy`.
    pub fn start_with(platform: impl Platform, policy: AttachPolicy) -> Core {
        Core::start_logging(platform, policy, FrameLog::default())
    }

    /// Starts the core like [`Core::start`], with the frame log already on, appending to `path`,
    /// so Devices Attached at startup are logged too. The core starts even if
    /// the file can't be opened, with the frame log off.
    pub fn start_with_frame_log(
        platform: impl Platform,
        path: &Path,
    ) -> (Core, Result<(), CommandError>) {
        Core::start_with_policy_and_frame_log(platform, AttachPolicy::Always, path)
    }

    /// Starts the core like [`Core::start_with`], with the frame log already
    /// on, like [`Core::start_with_frame_log`].
    pub fn start_with_policy_and_frame_log(
        platform: impl Platform,
        policy: AttachPolicy,
        path: &Path,
    ) -> (Core, Result<(), CommandError>) {
        let frame_log = FrameLog::default();
        let started = frame_log
            .start(path)
            .map_err(CommandError::frame_log_unavailable);
        (Core::start_logging(platform, policy, frame_log), started)
    }

    fn start_logging(platform: impl Platform, policy: AttachPolicy, frame_log: FrameLog) -> Core {
        let (requests, requests_rx) = mpsc::unbounded_channel();
        let (device_list_tx, device_list) = watch::channel(DeviceList::default());
        state::spawn(platform, policy, device_list_tx, frame_log, requests_rx);
        Core {
            requests,
            device_list,
            next_hold: AtomicU64::new(0),
        }
    }

    /// Sends `command` to the core. Resolves once the command has been
    /// accepted or rejected; its effect shows up in a later snapshot, once the
    /// Device confirms it.
    pub async fn command(&self, command: Command) -> Result<(), CommandError> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(Request::Command(command, reply))
            .map_err(|_| CommandError::CoreStopped)?;
        response.await.unwrap_or(Err(CommandError::CoreStopped))
    }

    /// Holds every Connected Device, now and as they Connect, until the
    /// returned [`Hold`] is dropped.
    pub fn hold_all(&self) -> Hold {
        self.hold(None)
    }

    /// Holds the Primary Device of the latest Device list snapshot until the
    /// returned [`Hold`] is dropped, and only that Device, even if another
    /// becomes Primary meanwhile. `None` if there is no Primary Device.
    pub fn hold_primary(&self) -> Option<Hold> {
        let primary = self.device_list.borrow().primary.clone()?;
        Some(self.hold(Some(primary)))
    }

    fn hold(&self, device: Option<DeviceAddress>) -> Hold {
        let id = self.next_hold.fetch_add(1, Ordering::Relaxed);
        // A stopped core holds nothing.
        let _ = self.requests.send(Request::Hold(id, device.clone()));
        Hold {
            id,
            device,
            requests: self.requests.clone(),
        }
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

/// When the core Attaches a Connected Device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachPolicy {
    /// For as long as it's Connected.
    Always,
    /// Only while a hold asks for it; otherwise it stays Detached, so another
    /// app or device can have its Control Link.
    OnRequest,
}
