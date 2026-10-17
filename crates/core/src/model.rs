//! Models on the allowlist and their Capabilities.

use serde::{Deserialize, Serialize};

use crate::command::NoiseControlMode;

/// A Model the app supports. Anything else is unsupported and never written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Model {
    GalaxyBuds4Pro,
    GalaxyBuds2,
    #[serde(rename = "wh1000xm5")]
    Wh1000Xm5,
}

impl Model {
    /// The Model's slug, as written in the frame log and fixtures.
    pub fn slug(self) -> &'static str {
        match self {
            Model::GalaxyBuds4Pro => "galaxy-buds4-pro",
            Model::GalaxyBuds2 => "galaxy-buds2",
            Model::Wh1000Xm5 => "wh-1000xm5",
        }
    }

    /// The Model's name, as the app shows it.
    pub fn display_name(self) -> &'static str {
        match self {
            Model::GalaxyBuds4Pro => "Galaxy Buds4 Pro",
            Model::GalaxyBuds2 => "Galaxy Buds2",
            Model::Wh1000Xm5 => "WH-1000XM5",
        }
    }

    pub fn capabilities(self) -> Capabilities {
        match self {
            Model::GalaxyBuds4Pro => Capabilities {
                noise_control_modes: vec![
                    NoiseControlMode::Off,
                    NoiseControlMode::NoiseCancelling,
                    NoiseControlMode::Ambient,
                    NoiseControlMode::Adaptive,
                ],
                ambient_level: Some(LevelRange { min: 0, max: 4 }),
                noise_cancelling_level: Some(LevelRange { min: 0, max: 4 }),
                power_off: false,
            },
            Model::GalaxyBuds2 => Capabilities {
                noise_control_modes: vec![
                    NoiseControlMode::Off,
                    NoiseControlMode::NoiseCancelling,
                    NoiseControlMode::Ambient,
                ],
                // The lowest step is 0: a real Buds2 ACKs a set to 0
                // (galaxy-buds2/real-ambient-level).
                ambient_level: Some(LevelRange { min: 0, max: 2 }),
                noise_cancelling_level: None,
                power_off: false,
            },
            Model::Wh1000Xm5 => Capabilities {
                noise_control_modes: vec![
                    NoiseControlMode::Off,
                    NoiseControlMode::NoiseCancelling,
                    NoiseControlMode::Ambient,
                ],
                ambient_level: Some(LevelRange { min: 1, max: 20 }),
                noise_cancelling_level: None,
                power_off: true,
            },
        }
    }
}

/// What a Device can be asked to do. Empty until its Model is identified.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub noise_control_modes: Vec<NoiseControlMode>,
    /// The Ambient Level's range in the Model's own steps, if it has one.
    pub ambient_level: Option<LevelRange>,
    /// The Noise Cancelling Level's range in the Model's own steps, if it has
    /// one.
    pub noise_cancelling_level: Option<LevelRange>,
    /// Whether the app can Power Off the Device.
    pub power_off: bool,
}

/// A level Capability's steps in the Model's own units, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LevelRange {
    pub min: u8,
    pub max: u8,
}

impl LevelRange {
    /// Whether `level` is one of the range's steps.
    pub fn contains(self, level: u8) -> bool {
        (self.min..=self.max).contains(&level)
    }
}
