//! The vendors' framing, message ids and the protocol values the Simulated
//! Device re-derives, so that it speaks the Device's end of a control link in
//! the core's own terms. Not part of the core's API.

pub mod samsung {
    pub use crate::samsung::frame::{encode, Deframer, Frame};
    pub use crate::samsung::{extended_status, id, BOTH_WORN, SERVICE};
}

pub mod sony {
    pub use crate::sony::frame::{encode, DataType, Deframer, Frame, Sequence};
    pub use crate::sony::{
        command, FLAT_CURVE, NOISE_CONTROL_SET_LEN, POWER_OFF, PRESET_EQ, SERVICE,
    };
}
