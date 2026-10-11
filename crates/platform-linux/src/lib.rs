//! The Linux platform adapter: BlueZ (through `bluer`) and the PulseAudio or
//! PipeWire default sink behind the core's platform port.

mod failure;
mod sink;

pub use failure::{connect_profile_failure, ConnectProfileFailure};
pub use sink::{sink_device, Sink};
