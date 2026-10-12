//! Models on the allowlist and their Capabilities.

use serde::{Deserialize, Serialize};

use crate::command::NoiseControlMode;

/// A Model the app supports. Anything else is unsupported and never written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Model {
    GalaxyBuds4Pro,
    #[serde(rename = "wh1000xm5")]
    Wh1000Xm5,
}

impl Model {
    /// The Model's slug, as written in the frame log and fixtures.
    pub fn slug(self) -> &'static str {
        match self {
            Model::GalaxyBuds4Pro => "galaxy-buds4-pro",
            Model::Wh1000Xm5 => "wh-1000xm5",
        }
    }

    /// The Model's name, as the app shows it.
    pub fn display_name(self) -> &'static str {
        match self {
            Model::GalaxyBuds4Pro => "Galaxy Buds4 Pro",
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
                equalizer: Some(EqualizerCapability {
                    presets: vec![
                        Preset::fixed(0, "Normal"),
                        Preset::fixed(1, "Bass boost"),
                        Preset::fixed(2, "Soft"),
                        Preset::fixed(3, "Dynamic"),
                        Preset::fixed(4, "Clear"),
                        Preset::fixed(5, "Treble boost"),
                        // Set from Samsung's app, which keeps its curve: the
                        // app can report it, but not pick it.
                        Preset {
                            selectable: false,
                            ..Preset::fixed(7, "Custom")
                        },
                    ],
                    bands: None,
                    clear_bass: None,
                }),
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
                equalizer: None,
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
    /// The Model's Equalizer, if the app can show and change it.
    pub equalizer: Option<EqualizerCapability>,
}

/// What a Model's Equalizer offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EqualizerCapability {
    /// Every Preset the Device can report, in the order the Model lists them.
    pub presets: Vec<Preset>,
    /// `None` if the Model's Bands can't be shown.
    pub bands: Option<Bands>,
    /// Clear Bass's range in the Model's own steps, if it has Clear Bass.
    pub clear_bass: Option<StepRange>,
}

impl EqualizerCapability {
    /// The Preset with wire id `id`, if the Model has one.
    pub fn preset(&self, id: u8) -> Option<&Preset> {
        self.presets.iter().find(|preset| preset.id == id)
    }
}

/// One of a Model's Presets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    /// The Preset's id on the wire.
    pub id: u8,
    /// The Preset's name in the Model's own words.
    pub name: &'static str,
    /// Whether its Bands can be changed while on it.
    pub editable: bool,
    /// Whether the app can pick it. A Preset it can't is only reported.
    pub selectable: bool,
}

impl Preset {
    /// A Preset the app can pick, whose Bands can't be changed.
    fn fixed(id: u8, name: &'static str) -> Preset {
        Preset {
            id,
            name,
            editable: false,
            selectable: true,
        }
    }
}

/// A Model's Bands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bands {
    /// Each Band's centre frequency, as the app labels it, lowest first.
    pub frequencies: Vec<&'static str>,
    /// The range every Band shares, in the Model's own steps.
    pub range: StepRange,
}

/// A signed setting's steps in the Model's own units, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StepRange {
    pub min: i8,
    pub max: i8,
}

impl StepRange {
    /// Whether `step` is one of the range's steps.
    pub fn contains(self, step: i8) -> bool {
        (self.min..=self.max).contains(&step)
    }
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
