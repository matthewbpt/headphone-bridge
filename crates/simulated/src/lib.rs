//! The Simulated Device: a Galaxy Buds4 Pro, a Galaxy Buds2 or a WH-1000XM5
//! that answers the core in-process, for trying the app without the real
//! headphones. It's a platform adapter that wraps another (ADR-0006): it
//! answers the core's control links for the simulated address, adds the
//! Simulated Device's observations to the OS's, and passes every other
//! address through. Only debug builds wrap their adapter in it.

mod device;

use std::sync::{Arc, Mutex, PoisonError};

use headphone_bridge_core::frame_log::ParseError;
use headphone_bridge_core::platform::{
    ControlLink, DeviceAddress, LinkFailure, Observation, Observations, Platform,
};
use headphone_bridge_core::Model;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::device::SimulatedDevice;

/// Why a Simulated Device couldn't start: its fixtures don't give it a
/// starting state. Every Model's tests rule these out.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StartError {
    /// A fixture's frames don't parse.
    #[error("the Simulated Device's fixture doesn't parse: {0}")]
    Frames(#[from] ParseError),
    /// The Galaxy Buds fixture has no extended status to start from.
    #[error("the Galaxy Buds fixture has no extended status")]
    NoExtendedStatus,
    /// The WH-1000XM5's Attach fixture has no answer to its noise control
    /// request.
    #[error("the WH-1000XM5 fixture has no noise control answer")]
    NoNoiseControl,
    /// The WH-1000XM5's Attach fixture has no answer to its Equalizer
    /// request.
    #[error("the WH-1000XM5 fixture has no Equalizer answer")]
    NoEqualizer,
}

/// Wraps `inner` and the `observations` it reports in the Simulated Device.
/// Returns the wrapped adapter, the observations to start the core with, and
/// the [`Simulator`] that switches the Simulated Device. The observations end
/// once `observations` has ended and every [`Simulator`] is dropped. Must run
/// inside a Tokio runtime.
pub fn wrap<P: Platform>(
    inner: P,
    observations: Observations,
) -> (WithSimulated<P>, Observations, Simulator) {
    let (merged, merged_rx) = mpsc::unbounded_channel();
    let shared = Arc::new(Shared {
        device: Mutex::default(),
        observations: merged.downgrade(),
    });
    let forwarding = merged.clone();
    let mut observations = observations;
    tokio::spawn(async move {
        while let Some(observation) = observations.recv().await {
            if forwarding.send(observation).is_err() {
                return;
            }
        }
    });
    let wrapped = WithSimulated {
        inner,
        shared: shared.clone(),
    };
    let simulator = Simulator {
        shared,
        observations: merged,
    };
    (wrapped, merged_rx, simulator)
}

/// A platform adapter with the Simulated Device in front of it.
pub struct WithSimulated<P> {
    inner: P,
    shared: Arc<Shared>,
}

/// Switches the Simulated Device. Its clones switch the same one.
#[derive(Clone)]
pub struct Simulator {
    shared: Arc<Shared>,
    /// Keeps the merged observations open while any clone is held.
    observations: mpsc::UnboundedSender<Observation>,
}

impl Simulator {
    /// Switches the Simulated Device on as `model`, or off with `None`. It's
    /// Connected while on, and Attached and Detached like a real Device.
    /// Switching to another Model Disconnects the one before; switching to
    /// the Model already on does nothing. If `model` can't start, the one on
    /// stays on.
    pub fn simulate(&self, model: Option<Model>) -> Result<(), StartError> {
        self.shared.switch(model, Some(&self.observations))
    }

    /// The Simulated Device's Model while it's switched on.
    pub fn model(&self) -> Option<Model> {
        self.shared
            .device
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(SimulatedDevice::model)
    }
}

/// What the wrapper and its [`Simulator`]s share.
struct Shared {
    /// The Simulated Device while it's switched on.
    device: Mutex<Option<SimulatedDevice>>,
    /// The merged observations, for a Power Off to report itself, without
    /// keeping them open.
    observations: mpsc::WeakUnboundedSender<Observation>,
}

impl Shared {
    /// Switches the Simulated Device to `model`, reporting the change to
    /// `observations` if given. Switching to the Model already on does
    /// nothing; if `model` can't start, the one on stays on.
    fn switch(
        &self,
        model: Option<Model>,
        observations: Option<&mpsc::UnboundedSender<Observation>>,
    ) -> Result<(), StartError> {
        let mut device = self.device.lock().unwrap_or_else(PoisonError::into_inner);
        if device.as_ref().map(SimulatedDevice::model) == model {
            return Ok(());
        }
        let next = model.map(SimulatedDevice::new).transpose()?;
        replace(&mut device, next, observations);
        Ok(())
    }

    /// Switches the Simulated Device off if it's still `model`, as its Power
    /// Off does, reporting the change to `observations` if given.
    fn switch_off_if(
        &self,
        model: Model,
        observations: Option<&mpsc::UnboundedSender<Observation>>,
    ) {
        let mut device = self.device.lock().unwrap_or_else(PoisonError::into_inner);
        if device.as_ref().map(SimulatedDevice::model) == Some(model) {
            replace(&mut device, None, observations);
        }
    }
}

/// Puts `next` in place of `device`, Disconnecting the one before and
/// Connecting `next`, as reported to `observations` if given.
fn replace(
    device: &mut Option<SimulatedDevice>,
    next: Option<SimulatedDevice>,
    observations: Option<&mpsc::UnboundedSender<Observation>>,
) {
    let report = |observation| {
        if let Some(observations) = observations {
            observations.send(observation).ok();
        }
    };
    if let Some(previous) = device.take() {
        report(Observation::DeviceDisconnected(previous.identity().address));
    }
    if let Some(next) = next {
        report(Observation::DeviceConnected(next.identity()));
        *device = Some(next);
    }
}

impl<P: Platform> Platform for WithSimulated<P> {
    async fn open_control_link(
        &self,
        address: DeviceAddress,
        service: Uuid,
    ) -> Result<ControlLink, LinkFailure> {
        let link = {
            let device = self
                .shared
                .device
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            device
                .as_ref()
                .filter(|device| device.is_at(address))
                .map(|device| {
                    // Powering Off switches it off, as it stops being Connected,
                    // unless it's been switched to another Model meanwhile.
                    let model = device.model();
                    let shared = self.shared.clone();
                    device.open(move || {
                        let observations = shared.observations.upgrade();
                        shared.switch_off_if(model, observations.as_ref());
                    })
                })
        };
        match link {
            Some(link) => Ok(link),
            None => self.inner.open_control_link(address, service).await,
        }
    }
}
