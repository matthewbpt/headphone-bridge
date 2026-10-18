//! Models on the allowlist and their Capabilities.

use serde::{Deserialize, Serialize};

use crate::command::{CommandError, Equalizer, NoiseControlMode};

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
                equalizer: Some(EqualizerCapability {
                    presets: [
                        samsung_presets(),
                        // Set from Samsung's app, which keeps its curve: the
                        // app can report it, but not pick it. 6 on the wire
                        // (galaxy-buds4-pro/real-equalizer-custom), not the 7
                        // upstream gave.
                        vec![Preset {
                            selectable: false,
                            ..Preset::fixed(6, "Custom")
                        }],
                    ]
                    .concat(),
                    bands: None,
                    clear_bass: None,
                }),
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
                // From GalaxyBudsClient, Gadgetbridge and Samsung's support
                // pages, and confirmed on a real Buds2
                // (galaxy-buds2/real-equalizer*). Samsung's app offers it no
                // Custom Preset, so neither does the Model.
                equalizer: Some(EqualizerCapability {
                    presets: samsung_presets(),
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
                // From Gadgetbridge and Sony's app, until checked against the
                // XM5's own EQEBB_RET_CAPABILITY.
                equalizer: Some(EqualizerCapability {
                    presets: vec![
                        Preset::fixed(0x00, "Off"),
                        Preset::fixed(0x10, "Bright"),
                        Preset::fixed(0x11, "Excited"),
                        Preset::fixed(0x12, "Mellow"),
                        Preset::fixed(0x13, "Relaxed"),
                        Preset::fixed(0x14, "Vocal"),
                        Preset::fixed(0x15, "Treble Boost"),
                        Preset::fixed(0x16, "Bass Boost"),
                        Preset::fixed(0x17, "Speech"),
                        Preset::editable(0xa0, "Manual"),
                        Preset::editable(0xa1, "Custom 1"),
                        Preset::editable(0xa2, "Custom 2"),
                    ],
                    bands: Some(Bands {
                        frequencies: WH_1000XM5_BANDS.to_vec(),
                        range: StepRange { min: -10, max: 10 },
                    }),
                    clear_bass: Some(StepRange { min: -10, max: 10 }),
                }),
            },
        }
    }
}

/// The fixed Presets the Galaxy Buds4 Pro and the Galaxy Buds2 share, by
/// wire value.
fn samsung_presets() -> Vec<Preset> {
    vec![
        Preset::fixed(0, "Normal"),
        Preset::fixed(1, "Bass boost"),
        Preset::fixed(2, "Soft"),
        Preset::fixed(3, "Dynamic"),
        Preset::fixed(4, "Clear"),
        Preset::fixed(5, "Treble boost"),
    ]
}

/// The WH-1000XM5's Bands, as their centre frequencies, lowest first.
pub(crate) const WH_1000XM5_BANDS: [&str; 5] = ["400 Hz", "1 kHz", "2.5 kHz", "6.3 kHz", "16 kHz"];

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

impl Capabilities {
    /// Rejects `level` unless a Device with these Capabilities has that
    /// Ambient Level.
    pub(crate) fn check_ambient_level(&self, level: u8) -> Result<(), CommandError> {
        Capabilities::check_level(self.ambient_level, level)
    }

    /// Rejects `level` unless a Device with these Capabilities has that
    /// Noise Cancelling Level.
    pub(crate) fn check_noise_cancelling_level(&self, level: u8) -> Result<(), CommandError> {
        Capabilities::check_level(self.noise_cancelling_level, level)
    }

    /// Rejects `equalizer` unless a Device with these Capabilities can be set
    /// to it.
    pub(crate) fn check_equalizer(&self, equalizer: &Equalizer) -> Result<(), CommandError> {
        let capability = self
            .equalizer
            .as_ref()
            .ok_or(CommandError::MissingCapability)?;
        let preset = capability
            .preset(equalizer.preset)
            .filter(|preset| preset.selectable)
            .ok_or(CommandError::UnavailablePreset)?;
        if let Some(bands) = &equalizer.bands {
            let shape = capability
                .bands
                .as_ref()
                .ok_or(CommandError::MissingCapability)?;
            if bands.len() != shape.frequencies.len() {
                return Err(CommandError::OutOfRange);
            }
            Capabilities::check_steps(Some(shape.range), preset, bands)?;
        }
        if let Some(clear_bass) = equalizer.clear_bass {
            Capabilities::check_steps(capability.clear_bass, preset, &[clear_bass])?;
        }
        // A Model with both takes them together.
        if capability.bands.is_some()
            && capability.clear_bass.is_some()
            && equalizer.bands.is_some() != equalizer.clear_bass.is_some()
        {
            return Err(CommandError::IncompleteEqualizer);
        }
        Ok(())
    }

    /// Rejects `level` unless a Device with the level Capability `range` has it.
    fn check_level(range: Option<LevelRange>, level: u8) -> Result<(), CommandError> {
        let range = range.ok_or(CommandError::MissingCapability)?;
        if !range.contains(level) {
            return Err(CommandError::OutOfRange);
        }
        Ok(())
    }

    /// Rejects `steps` unless a Device with the step Capability `range` can set
    /// them on `preset`.
    fn check_steps(
        range: Option<StepRange>,
        preset: &Preset,
        steps: &[i8],
    ) -> Result<(), CommandError> {
        let range = range.ok_or(CommandError::MissingCapability)?;
        if !preset.editable {
            return Err(CommandError::FixedPreset);
        }
        if !steps.iter().all(|&step| range.contains(step)) {
            return Err(CommandError::OutOfRange);
        }
        Ok(())
    }
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
    pub fn preset(&self, id: PresetId) -> Option<&Preset> {
        self.presets.iter().find(|preset| preset.id == id)
    }
}

/// A Preset's id on the wire, as its Model's protocol numbers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PresetId(pub u8);

/// One of a Model's Presets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: PresetId,
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
            id: PresetId(id),
            name,
            editable: false,
            selectable: true,
        }
    }

    /// A Preset the app can pick, whose Bands can be changed.
    fn editable(id: u8, name: &'static str) -> Preset {
        Preset {
            editable: true,
            ..Preset::fixed(id, name)
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

/// A setting's steps in the Model's own units, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Steps<T> {
    pub min: T,
    pub max: T,
}

impl<T: PartialOrd> Steps<T> {
    /// Whether `step` is one of the range's steps.
    pub fn contains(self, step: T) -> bool {
        self.min <= step && step <= self.max
    }
}

/// A level Capability's steps.
pub type LevelRange = Steps<u8>;

/// A signed setting's steps, such as a Band's or Clear Bass.
pub type StepRange = Steps<i8>;

// Every Model on the allowlist has an Equalizer, so a Device can't be asked
// for one without it; the rule is checked here instead.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_equalizer_for_a_model_without_the_capability_is_rejected() {
        let preset = Equalizer {
            preset: PresetId(0),
            bands: None,
            clear_bass: None,
        };
        assert_eq!(
            Capabilities::default().check_equalizer(&preset),
            Err(CommandError::MissingCapability)
        );
    }
}
