//! The vendors' framing, for the Simulated Device to speak the Device's end
//! of a control link. Not part of the core's API: only debug builds of the
//! Android app use it.

pub mod samsung {
    pub use crate::samsung::frame::{encode, Deframer, Frame};
    pub use crate::samsung::SERVICE;
}

pub mod sony {
    pub use crate::sony::frame::{encode, DataType, Deframer, Frame};
    pub use crate::sony::SERVICE;
}
