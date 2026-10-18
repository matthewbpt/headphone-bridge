//! The Simulated Device's commands, compiled into debug builds alone.

use headphone_bridge_core::Model;
use headphone_bridge_simulated::{Simulator, StartError};
use tauri::State;

/// Why the Simulated Device wasn't switched, as the UI names it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SimulateError {
    /// The Model couldn't start from its fixtures. The developer text why
    /// goes to the log.
    SimulatedDeviceUnavailable,
}

impl From<StartError> for SimulateError {
    fn from(error: StartError) -> SimulateError {
        let message = format!("couldn't start the Simulated Device: {error}");
        #[cfg(target_os = "android")]
        headphone_bridge_platform_android::android::log(&message);
        #[cfg(not(target_os = "android"))]
        eprintln!("{message}");
        SimulateError::SimulatedDeviceUnavailable
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
            serde_json::to_string(&SimulateError::SimulatedDeviceUnavailable).unwrap(),
            r#""simulatedDeviceUnavailable""#
        );
    }
}
