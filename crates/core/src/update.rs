//! What a control link session reports to the core, whatever the vendor.

use crate::command::NoiseControlMode;
use crate::device_list::Battery;
use crate::model::Model;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    Identified(Model),
    /// The Device isn't a supported Model: the session must end without
    /// writing anything else.
    Unsupported,
    Battery(Battery),
    ChosenMode(NoiseControlMode),
}
