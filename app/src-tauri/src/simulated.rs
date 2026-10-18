//! The Simulated Device's commands, compiled into debug builds alone.

use headphone_bridge_core::Model;
use headphone_bridge_simulated::{Simulator, StartError};
use tauri::State;

/// Why the Simulated Device wasn't switched, as the UI names it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SimulateError {
    /// The Model couldn't start from its fixtures, with the developer text
    /// why.
    SimulatedDeviceUnavailable(String),
}

impl From<StartError> for SimulateError {
    fn from(error: StartError) -> SimulateError {
        SimulateError::SimulatedDeviceUnavailable(error.to_string())
    }
}

/// The Simulated Device's Model while it's switched on.
#[tauri::command]
pub fn simulated_device(simulator: State<Simulator>) -> Option<Model> {
    simulator.model()
}

/// Switches the Simulated Device on as `model`, or off with `None`.
#[tauri::command]
pub fn set_simulated_device(
    simulator: State<Simulator>,
    model: Option<Model>,
) -> Result<(), SimulateError> {
    Ok(simulator.simulate(model)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_a_simulated_device_that_cant_start_as_the_ui_does() {
        assert_eq!(
            serde_json::to_string(&SimulateError::SimulatedDeviceUnavailable(
                "the Galaxy Buds fixture has no extended status".into()
            ))
            .unwrap(),
            r#"{"simulatedDeviceUnavailable":"the Galaxy Buds fixture has no extended status"}"#
        );
    }
}
