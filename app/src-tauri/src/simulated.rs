//! The Simulated Device's commands, compiled into debug builds alone.

use headphone_bridge_core::Model;
#[cfg(not(target_os = "android"))]
use headphone_bridge_simulated::{Simulator, StartError};
#[cfg(not(target_os = "android"))]
use tauri::State;

/// Why the Simulated Device wasn't switched, as the UI names it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    all(target_os = "android", not(test)),
    expect(dead_code, reason = "Android's own simulator can't fail yet")
)]
pub enum SimulateError {
    /// The Model couldn't start from its fixtures, with the developer text
    /// why.
    SimulatedDeviceUnavailable(String),
}

#[cfg(not(target_os = "android"))]
impl From<StartError> for SimulateError {
    fn from(error: StartError) -> SimulateError {
        SimulateError::SimulatedDeviceUnavailable(error.to_string())
    }
}

/// The Simulated Device's Model while it's switched on.
#[cfg(not(target_os = "android"))]
#[tauri::command]
pub fn simulated_device(simulator: State<Simulator>) -> Option<Model> {
    simulator.model()
}

/// Switches the Simulated Device on as `model`, or off with `None`.
#[cfg(not(target_os = "android"))]
#[tauri::command]
pub fn set_simulated_device(
    simulator: State<Simulator>,
    model: Option<Model>,
) -> Result<(), SimulateError> {
    Ok(simulator.simulate(model)?)
}

/// The Simulated Device's Model while it's switched on. Android's adapter
/// has its own simulator until it wraps its adapter too.
#[cfg(target_os = "android")]
#[tauri::command]
pub fn simulated_device() -> Option<Model> {
    headphone_bridge_platform_android::android::simulated()
}

/// Switches the Simulated Device on as `model`, or off with `None`.
#[cfg(target_os = "android")]
#[tauri::command]
pub fn set_simulated_device(model: Option<Model>) -> Result<(), SimulateError> {
    headphone_bridge_platform_android::android::simulate(model);
    Ok(())
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
