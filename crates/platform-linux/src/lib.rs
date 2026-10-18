//! The Linux platform adapter: BlueZ (through `bluer`) and the PulseAudio or
//! PipeWire default sink behind the core's platform port.
//!
//! Thin and tested by hand, apart from the pure mappings below, which build
//! and are tested on every OS. It knows nothing about vendor protocols, and it
//! never connects or disconnects a Device itself.

mod failure;
mod sink;

pub use failure::connect_profile_failure;
pub use sink::{sink_device, Sink};

#[cfg(target_os = "linux")]
mod audio;
#[cfg(target_os = "linux")]
mod bluez;

#[cfg(target_os = "linux")]
pub use linux::LinuxBluetooth;

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::{Mutex, PoisonError};

    use headphone_bridge_core::platform::{
        ControlLink, DeviceAddress, LinkFailure, Observations, Platform,
    };
    use tokio::sync::mpsc;
    use uuid::Uuid;

    use crate::{audio, bluez};

    /// The BlueZ and sound server adapter. Must be started inside a tokio
    /// runtime; it watches Devices and the default sink for the life of the
    /// process.
    pub struct LinuxBluetooth {
        bluez: bluez::Connection,
        observations: Mutex<Option<Observations>>,
    }

    impl LinuxBluetooth {
        pub fn start() -> LinuxBluetooth {
            let (observations_tx, observations) = mpsc::unbounded_channel();
            let bluez = bluez::Connection::default();
            audio::watch(observations_tx.clone());
            tokio::spawn(bluez::watch(bluez.clone(), observations_tx));
            LinuxBluetooth {
                bluez,
                observations: Mutex::new(Some(observations)),
            }
        }
    }

    impl Platform for LinuxBluetooth {
        fn observations(&self) -> Observations {
            self.observations
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .unwrap_or_else(|| mpsc::unbounded_channel().1)
        }

        async fn open_control_link(
            &self,
            address: DeviceAddress,
            service: Uuid,
        ) -> Result<ControlLink, LinkFailure> {
            let bluez = self.bluez.get().await.map_err(|error| {
                eprintln!("Bluetooth is unavailable: {error}");
                LinkFailure::from(error)
            })?;
            bluez.open(address, service).await
        }
    }
}
