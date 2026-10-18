//! The Simulated Device, driven through a real core over the wrapper around
//! a scripted adapter that observes nothing and refuses every control link.

#![cfg(test)]

use std::time::Duration;

use headphone_bridge_core::platform::{
    ControlLink, DeviceAddress, DeviceIdentity, LinkFailure, Observation, Platform,
};
use headphone_bridge_core::{
    AttachPolicy, Battery, Command, CommandKind, Core, CoreBuilder, Device, DeviceCommand,
    DeviceList, DeviceState, Equalizer, Model, NoiseControlMode, PresetId,
};
use headphone_bridge_simulated::{wrap, Simulator};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

/// The scripted adapter the Simulated Device wraps: it refuses every control
/// link as not Connected, reporting each address it was asked for.
struct Os {
    asked: mpsc::UnboundedSender<DeviceAddress>,
}

impl Platform for Os {
    async fn open_control_link(
        &self,
        address: DeviceAddress,
        _service: Uuid,
    ) -> Result<ControlLink, LinkFailure> {
        let _ = self.asked.send(address);
        Err(LinkFailure::DeviceNotConnected)
    }
}

/// A core over the wrapper, and the test's handles on it.
struct Started {
    core: Core,
    simulator: Simulator,
    list: watch::Receiver<DeviceList>,
    /// What the scripted OS reports.
    observe: mpsc::UnboundedSender<Observation>,
    /// Every address the scripted OS was asked to open a control link to.
    asked: mpsc::UnboundedReceiver<DeviceAddress>,
}

/// A core with desktop's Always Attach Policy.
fn start() -> Started {
    start_with(Core::builder())
}

fn start_with(core: CoreBuilder) -> Started {
    let (asked_tx, asked) = mpsc::unbounded_channel();
    let (observe, observations) = mpsc::unbounded_channel();
    let (platform, observations, simulator) = wrap(Os { asked: asked_tx }, observations);
    let core = core.start(platform, observations);
    let list = core.subscribe();
    Started {
        core,
        simulator,
        list,
        observe,
        asked,
    }
}

impl Started {
    /// The first Device list in which `condition` holds.
    async fn until(&mut self, condition: impl Fn(&DeviceList) -> bool) -> DeviceList {
        let wait = async {
            loop {
                {
                    let list = self.list.borrow_and_update();
                    if condition(&list) {
                        return list.clone();
                    }
                }
                self.list.changed().await.expect("core stopped");
            }
        };
        tokio::time::timeout(Duration::from_secs(5), wait)
            .await
            .expect("timed out waiting for the Device list")
    }

    /// The Simulated Device, once it's the only Device and `condition` holds
    /// of it.
    async fn simulated(&mut self, condition: impl Fn(&Device) -> bool) -> Device {
        let list = self
            .until(|list| matches!(list.devices.as_slice(), [device] if condition(device)))
            .await;
        list.devices[0].clone()
    }

    /// Switches on the simulated `model` and waits until it's Attached with
    /// its settings reported; returns its address.
    async fn attached(&mut self, model: Model) -> DeviceAddress {
        self.simulator.simulate(Some(model)).unwrap();
        self.simulated(|device| {
            device.state == DeviceState::Attached && device.chosen_mode.is_some()
        })
        .await
        .address
    }

    /// Runs `kind` on the Device at `address`, which the core must accept.
    async fn command(&self, address: DeviceAddress, kind: CommandKind) {
        self.core
            .command(Command::Device(DeviceCommand { address, kind }))
            .await
            .unwrap();
    }

    /// Waits until no Device is Connected.
    async fn none(&mut self) {
        self.until(|list| list.devices.is_empty()).await;
    }
}

/// The Equalizer at `preset` with no curve.
fn preset(preset: u8) -> Equalizer {
    Equalizer {
        preset: PresetId(preset),
        bands: None,
        clear_bass: None,
    }
}

#[tokio::test]
async fn a_simulated_buds4_pro_attaches_at_once_and_reports_its_status() {
    let mut started = start();
    started
        .simulator
        .simulate(Some(Model::GalaxyBuds4Pro))
        .unwrap();

    let device = started
        .simulated(|device| device.state == DeviceState::Attached && device.battery.is_some())
        .await;
    assert_eq!(device.model, Some(Model::GalaxyBuds4Pro));
    assert_eq!(device.name, "Galaxy Buds4 Pro (Simulated)");
    // capture-3's first extended status: L 100 %, R 97 %.
    assert!(matches!(
        device.battery,
        Some(Battery::Buds {
            left: Some(100),
            right: Some(97),
            ..
        })
    ));
    assert!(device.chosen_mode.is_some());
    assert!(device.ambient_level.is_some());
    assert!(device.noise_cancelling_level.is_some());
}

#[tokio::test]
async fn the_simulated_buds4_pro_confirms_a_mode_levels_and_a_preset_like_a_real_one() {
    let mut started = start();
    let address = started.attached(Model::GalaxyBuds4Pro).await;

    started
        .command(
            address,
            CommandKind::SetNoiseControlMode {
                mode: NoiseControlMode::NoiseCancelling,
            },
        )
        .await;
    started
        .simulated(|device| device.chosen_mode == Some(NoiseControlMode::NoiseCancelling))
        .await;

    started
        .command(address, CommandKind::SetNoiseCancellingLevel { level: 3 })
        .await;
    started
        .simulated(|device| device.noise_cancelling_level == Some(3))
        .await;

    started
        .command(address, CommandKind::SetAmbientLevel { level: 1 })
        .await;
    started
        .simulated(|device| device.ambient_level == Some(1))
        .await;

    // capture-3: Dynamic; then Soft.
    assert_eq!(
        started.core.device_list().devices[0].equalizer,
        Some(preset(3))
    );
    started
        .command(
            address,
            CommandKind::SetEqualizer {
                equalizer: preset(2),
            },
        )
        .await;
    started
        .simulated(|device| device.equalizer == Some(preset(2)))
        .await;
}

#[tokio::test]
async fn a_simulated_buds2_is_one_and_confirms_a_mode_a_level_and_a_preset_within_its_range() {
    let mut started = start();
    let address = started.attached(Model::GalaxyBuds2).await;
    let device = started.core.device_list().devices[0].clone();
    assert_eq!(device.model, Some(Model::GalaxyBuds2));
    assert_eq!(device.name, "Galaxy Buds2 (Simulated)");
    assert_eq!(device.capabilities, Model::GalaxyBuds2.capabilities());
    // GalaxyBudsClient's Buds2 extended status: L and R 100 %, Dynamic.
    assert!(matches!(
        device.battery,
        Some(Battery::Buds {
            left: Some(100),
            right: Some(100),
            ..
        })
    ));
    assert_eq!(device.equalizer, Some(preset(3)));

    for mode in [NoiseControlMode::Ambient, NoiseControlMode::Off] {
        started
            .command(address, CommandKind::SetNoiseControlMode { mode })
            .await;
        started
            .simulated(|device| device.chosen_mode == Some(mode))
            .await;
    }
    for level in [2, 0] {
        started
            .command(address, CommandKind::SetAmbientLevel { level })
            .await;
        started
            .simulated(|device| device.ambient_level == Some(level))
            .await;
    }
    // Treble boost.
    started
        .command(
            address,
            CommandKind::SetEqualizer {
                equalizer: preset(5),
            },
        )
        .await;
    started
        .simulated(|device| device.equalizer == Some(preset(5)))
        .await;
}

#[tokio::test]
async fn the_simulated_xm5_attaches_reports_its_status_and_confirms_a_mode_and_level() {
    let mut started = start();
    let address = started.attached(Model::Wh1000Xm5).await;
    let device = started.simulated(|device| device.battery.is_some()).await;
    assert_eq!(device.model, Some(Model::Wh1000Xm5));
    assert_eq!(device.name, "WH-1000XM5 (Simulated)");
    assert!(matches!(device.battery, Some(Battery::Headphones { .. })));
    // attach.frames: Ambient.
    assert_eq!(device.chosen_mode, Some(NoiseControlMode::Ambient));

    started
        .command(address, CommandKind::SetAmbientLevel { level: 15 })
        .await;
    started
        .simulated(|device| device.ambient_level == Some(15))
        .await;

    started
        .command(
            address,
            CommandKind::SetNoiseControlMode {
                mode: NoiseControlMode::Off,
            },
        )
        .await;
    started
        .simulated(|device| device.chosen_mode == Some(NoiseControlMode::Off))
        .await;
}

#[tokio::test]
async fn the_simulated_xm5_reports_its_equalizer_and_confirms_presets_and_curves_like_a_real_one() {
    let mut started = start();
    let address = started.attached(Model::Wh1000Xm5).await;
    let device = started.simulated(|device| device.equalizer.is_some()).await;
    // attach.frames: Bright, Clear Bass +2 and Bands −1, 0, +3, +5, +10.
    assert_eq!(
        device.equalizer,
        Some(Equalizer {
            preset: PresetId(0x10),
            bands: Some(vec![-1, 0, 3, 5, 10]),
            clear_bass: Some(2),
        })
    );
    assert!(!device.equalizer_unavailable);

    // Bass Boost, with the curve a real XM5 reports for it.
    started
        .command(
            address,
            CommandKind::SetEqualizer {
                equalizer: preset(0x16),
            },
        )
        .await;
    let bass_boost = Equalizer {
        preset: PresetId(0x16),
        bands: Some(vec![0, 0, 0, 0, 0]),
        clear_bass: Some(7),
    };
    started
        .simulated(|device| device.equalizer == Some(bass_boost.clone()))
        .await;

    // Onto Manual from Bass Boost's curve, then edited on Manual: a real XM5
    // only ACKs that, so the core reads it back.
    for clear_bass in [7, 3] {
        let manual = Equalizer {
            preset: PresetId(0xa0),
            bands: Some(vec![0, 4, 0, 0, 0]),
            clear_bass: Some(clear_bass),
        };
        started
            .command(
                address,
                CommandKind::SetEqualizer {
                    equalizer: manual.clone(),
                },
            )
            .await;
        started
            .simulated(|device| device.equalizer == Some(manual.clone()))
            .await;
    }
}

#[tokio::test]
async fn under_on_request_it_waits_for_a_hold_and_keeps_its_settings_across_a_detach() {
    let mut started = start_with(Core::builder().attach_policy(AttachPolicy::OnRequest));
    started
        .simulator
        .simulate(Some(Model::GalaxyBuds4Pro))
        .unwrap();
    started
        .simulated(|device| device.state == DeviceState::Detached)
        .await;

    let hold = started.core.hold_all();
    let address = started
        .simulated(|device| device.state == DeviceState::Attached && device.chosen_mode.is_some())
        .await
        .address;
    started
        .command(
            address,
            CommandKind::SetNoiseControlMode {
                mode: NoiseControlMode::Off,
            },
        )
        .await;
    started
        .simulated(|device| device.chosen_mode == Some(NoiseControlMode::Off))
        .await;

    drop(hold);
    started
        .simulated(|device| device.state == DeviceState::Detached)
        .await;
    let _hold = started.core.hold_all();
    // Attached again over a new link, once it has reported again.
    let device = started
        .simulated(|device| {
            device.state == DeviceState::Attached
                && device.reported_at.is_none()
                && device.battery.is_some()
        })
        .await;
    assert_eq!(device.chosen_mode, Some(NoiseControlMode::Off));
}

#[tokio::test]
async fn switching_to_another_model_or_off_disconnects_the_one_before() {
    let mut started = start();
    started.attached(Model::GalaxyBuds4Pro).await;

    started.simulator.simulate(Some(Model::Wh1000Xm5)).unwrap();
    started
        .simulated(|device| {
            device.model == Some(Model::Wh1000Xm5) && device.state == DeviceState::Attached
        })
        .await;
    assert_eq!(started.simulator.model(), Some(Model::Wh1000Xm5));

    started.simulator.simulate(None).unwrap();
    started.none().await;
    assert_eq!(started.simulator.model(), None);
}

#[tokio::test]
async fn the_simulated_xm5_powers_off_by_disconnecting_and_switching_off() {
    let mut started = start();
    let address = started.attached(Model::Wh1000Xm5).await;

    started.command(address, CommandKind::PowerOff).await;
    started.none().await;
    assert_eq!(started.simulator.model(), None);
}

/// A real Galaxy Buds4 Pro, as the OS reports it.
fn real_buds() -> DeviceIdentity {
    DeviceIdentity {
        address: DeviceAddress::parse("A1:B2:C3:D4:E5:F6").unwrap(),
        name: "Galaxy Buds4 Pro (A1B2)".into(),
        service_uuids: vec![headphone_bridge_core::wire::samsung::SERVICE],
    }
}

#[tokio::test]
async fn a_real_device_beside_it_reaches_the_core_and_the_wrapped_adapter() {
    let mut started = start();
    started.attached(Model::GalaxyBuds4Pro).await;

    started
        .observe
        .send(Observation::DeviceConnected(real_buds()))
        .unwrap();
    started
        .until(|list| {
            list.devices.len() == 2
                && list
                    .devices
                    .iter()
                    .any(|device| device.address == real_buds().address)
        })
        .await;
    let asked = tokio::time::timeout(Duration::from_secs(5), started.asked.recv())
        .await
        .expect("the wrapped adapter wasn't asked for a control link")
        .unwrap();
    assert_eq!(asked, real_buds().address);
}

#[tokio::test]
async fn it_works_over_an_adapter_whose_observations_have_already_ended() {
    let mut started = start();
    let (closed, _) = mpsc::unbounded_channel();
    drop(std::mem::replace(&mut started.observe, closed));
    tokio::task::yield_now().await;

    started.attached(Model::Wh1000Xm5).await;
}

#[tokio::test]
async fn every_model_starts() {
    let started = start();
    for model in [Model::GalaxyBuds4Pro, Model::GalaxyBuds2, Model::Wh1000Xm5] {
        assert_eq!(started.simulator.simulate(Some(model)), Ok(()));
        assert_eq!(started.simulator.model(), Some(model));
    }
}
