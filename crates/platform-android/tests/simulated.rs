//! The Simulated Device, in debug builds: a Buds4 Pro or an XM5 answering the
//! core in-process, for the emulator, which has no headphones.
#![cfg(debug_assertions)]

use std::collections::VecDeque;
use std::time::Duration;

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{
    Battery, Command, Device, DeviceList, DeviceState, Equalizer, Model, NoiseControlMode,
};
use headphone_bridge_platform_android::messages::Message;
use headphone_bridge_platform_android::Host;
use tokio::sync::mpsc;

/// Kotlin's side of the boundary: every message the host sends it.
struct Kotlin {
    messages: mpsc::UnboundedReceiver<Message>,
    /// Messages passed over while waiting for another, oldest first.
    skipped: VecDeque<Message>,
}

fn start() -> (Host, Kotlin) {
    let (sink, messages) = mpsc::unbounded_channel();
    let host = Host::start(move |message| sink.send(message).is_ok());
    let skipped = VecDeque::new();
    (host, Kotlin { messages, skipped })
}

impl Kotlin {
    async fn recv(&mut self) -> Message {
        match self.skipped.pop_front() {
            Some(message) => message,
            None => self.messages.recv().await.expect("host stopped"),
        }
    }
}

impl Kotlin {
    /// The first Device list after which `condition` holds of the Simulated
    /// Device, which is the only Device. Fails if Kotlin is asked to open a
    /// control link: the Simulated Device needs no Bluetooth.
    async fn simulated(&mut self, condition: impl Fn(&Device) -> bool) -> Device {
        let wait = async {
            loop {
                match self.recv().await {
                    Message::DeviceList { list } => {
                        if let Some(device) = only(&list).filter(|device| condition(device)) {
                            return device.clone();
                        }
                    }
                    Message::Open { .. } => panic!("Kotlin was asked to open a control link"),
                    _ => {}
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(5), wait)
            .await
            .expect("timed out waiting for the Simulated Device")
    }
}

fn only(list: &DeviceList) -> Option<&Device> {
    match list.devices.as_slice() {
        [device] => Some(device),
        _ => None,
    }
}

#[tokio::test]
async fn a_simulated_buds4_pro_is_detached_until_held_then_attaches_and_reports_its_status() {
    let (host, mut kotlin) = start();
    host.simulate(Some(Model::GalaxyBuds4Pro));
    kotlin
        .simulated(|device| device.state == DeviceState::Detached)
        .await;

    host.event(r#"{"type":"holdAll"}"#).unwrap();
    let device = kotlin
        .simulated(|device| device.state == DeviceState::Attached && device.battery.is_some())
        .await;
    assert_eq!(device.model, Some(Model::GalaxyBuds4Pro));
    assert!(matches!(
        device.battery,
        Some(Battery::Buds {
            left: Some(_),
            right: Some(_),
            ..
        })
    ));
    assert!(device.chosen_mode.is_some());
    assert!(device.ambient_level.is_some());
    assert!(device.noise_cancelling_level.is_some());
}

impl Kotlin {
    /// Waits for the core's answer to command `request`, which must accept it.
    async fn accepted(&mut self, request: i64) {
        let wait = async {
            loop {
                match self.messages.recv().await.expect("host stopped") {
                    Message::CommandResult {
                        request: of,
                        result,
                    } if of == request => return result,
                    other => self.skipped.push_back(other),
                }
            }
        };
        let result = tokio::time::timeout(Duration::from_secs(5), wait)
            .await
            .expect("timed out waiting for the command's result");
        assert_eq!(result, Ok(()), "command {request}");
    }
}

/// Switches on the simulated `model` and holds it until it's Attached with
/// its settings reported; returns its address.
async fn attached(host: &Host, kotlin: &mut Kotlin, model: Model) -> String {
    host.simulate(Some(model));
    host.event(r#"{"type":"holdAll"}"#).unwrap();
    kotlin
        .simulated(|device| device.state == DeviceState::Attached && device.chosen_mode.is_some())
        .await
        .address
        .0
}

#[tokio::test]
async fn the_simulated_buds4_pro_confirms_a_mode_and_levels_like_a_real_one() {
    let (host, mut kotlin) = start();
    let address = attached(&host, &mut kotlin, Model::GalaxyBuds4Pro).await;

    host.command(
        1,
        &format!(
            r#"{{"kind":"setNoiseControlMode","address":"{address}","mode":"noiseCancelling"}}"#
        ),
    );
    kotlin.accepted(1).await;
    kotlin
        .simulated(|device| device.chosen_mode == Some(NoiseControlMode::NoiseCancelling))
        .await;

    host.command(
        2,
        &format!(r#"{{"kind":"setNoiseCancellingLevel","address":"{address}","level":3}}"#),
    );
    kotlin.accepted(2).await;
    kotlin
        .simulated(|device| device.noise_cancelling_level == Some(3))
        .await;

    host.command(
        3,
        &format!(r#"{{"kind":"setAmbientLevel","address":"{address}","level":1}}"#),
    );
    kotlin.accepted(3).await;
    kotlin
        .simulated(|device| device.ambient_level == Some(1))
        .await;
}

#[tokio::test]
async fn the_simulated_xm5_attaches_reports_its_battery_and_confirms_a_mode_and_level() {
    let (host, mut kotlin) = start();
    let address = attached(&host, &mut kotlin, Model::Wh1000Xm5).await;
    let device = host.core().device_list().devices[0].clone();
    assert_eq!(device.model, Some(Model::Wh1000Xm5));
    assert!(matches!(device.battery, Some(Battery::Headphones { .. })));
    // attach.frames: Ambient, Ambient Level 10.
    assert_eq!(device.chosen_mode, Some(NoiseControlMode::Ambient));

    host.command(
        1,
        &format!(r#"{{"kind":"setAmbientLevel","address":"{address}","level":15}}"#),
    );
    kotlin.accepted(1).await;
    kotlin
        .simulated(|device| device.ambient_level == Some(15))
        .await;

    host.command(
        2,
        &format!(r#"{{"kind":"setNoiseControlMode","address":"{address}","mode":"off"}}"#),
    );
    kotlin.accepted(2).await;
    kotlin
        .simulated(|device| device.chosen_mode == Some(NoiseControlMode::Off))
        .await;
}

#[tokio::test]
async fn ending_the_hold_detaches_the_simulated_device_and_holding_it_again_reattaches_it() {
    let (host, mut kotlin) = start();
    attached(&host, &mut kotlin, Model::Wh1000Xm5).await;

    host.event(r#"{"type":"endHoldAll"}"#).unwrap();
    kotlin
        .simulated(|device| device.state == DeviceState::Detached)
        .await;

    host.event(r#"{"type":"holdAll"}"#).unwrap();
    kotlin
        .simulated(|device| device.state == DeviceState::Attached)
        .await;
}

#[tokio::test]
async fn switching_the_simulated_device_off_or_to_another_model_disconnects_it() {
    let (host, mut kotlin) = start();
    attached(&host, &mut kotlin, Model::GalaxyBuds4Pro).await;

    host.simulate(Some(Model::Wh1000Xm5));
    kotlin
        .simulated(|device| device.name == "WH-1000XM5 (Simulated)")
        .await;
    assert_eq!(host.simulated(), Some(Model::Wh1000Xm5));

    host.simulate(None);
    let wait = async {
        loop {
            if let Message::DeviceList { list } = kotlin.recv().await {
                if list.devices.is_empty() {
                    return;
                }
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), wait)
        .await
        .expect("the Simulated Device stayed Connected");
    assert_eq!(host.simulated(), None);
}

#[tokio::test]
async fn the_simulated_xm5_powers_off_by_disconnecting_and_switching_off() {
    let (host, mut kotlin) = start();
    let address = attached(&host, &mut kotlin, Model::Wh1000Xm5).await;

    host.command(
        1,
        &format!(r#"{{"kind":"powerOff","address":"{address}"}}"#),
    );
    kotlin.accepted(1).await;
    let wait = async {
        loop {
            if let Message::DeviceList { list } = kotlin.recv().await {
                if list.devices.is_empty() {
                    return;
                }
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), wait)
        .await
        .expect("the Simulated Device stayed Connected");
    assert_eq!(host.simulated(), None);
}

#[tokio::test]
async fn the_simulated_device_keeps_its_settings_across_a_detach_like_a_real_one() {
    let (host, mut kotlin) = start();
    let address = attached(&host, &mut kotlin, Model::GalaxyBuds4Pro).await;
    host.command(
        1,
        &format!(r#"{{"kind":"setNoiseControlMode","address":"{address}","mode":"off"}}"#),
    );
    kotlin.accepted(1).await;
    kotlin
        .simulated(|device| device.chosen_mode == Some(NoiseControlMode::Off))
        .await;

    host.event(r#"{"type":"endHoldAll"}"#).unwrap();
    kotlin
        .simulated(|device| device.state == DeviceState::Detached)
        .await;
    host.event(r#"{"type":"holdAll"}"#).unwrap();
    // Attached again over a new link, before and after it reports again.
    kotlin
        .simulated(|device| device.state == DeviceState::Attached && device.reported_at.is_none())
        .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let device = host.core().device_list().devices[0].clone();
    assert_eq!(device.chosen_mode, Some(NoiseControlMode::Off));
}

/// The Equalizer at `preset` with no curve.
fn preset(preset: u8) -> Equalizer {
    Equalizer {
        preset,
        bands: None,
        clear_bass: None,
    }
}

#[tokio::test]
async fn the_simulated_buds4_pro_confirms_an_equalizer_preset_like_a_real_one() {
    let (host, mut kotlin) = start();
    let address = attached(&host, &mut kotlin, Model::GalaxyBuds4Pro).await;

    // capture-3: Dynamic.
    assert_eq!(
        host.core().device_list().devices[0].equalizer,
        Some(preset(3))
    );

    // Soft, set from the App Screen, which reaches the core through Tauri.
    host.core()
        .command(Command::SetEqualizer(DeviceAddress(address), preset(2)))
        .await
        .unwrap();
    kotlin
        .simulated(|device| device.equalizer == Some(preset(2)))
        .await;
}

#[tokio::test]
async fn the_simulated_xm5_reports_its_equalizer_and_confirms_presets_and_curves_like_a_real_one() {
    let (host, mut kotlin) = start();
    let address = DeviceAddress(attached(&host, &mut kotlin, Model::Wh1000Xm5).await);
    let device = kotlin.simulated(|device| device.equalizer.is_some()).await;
    // attach.frames: Bright, Clear Bass +2 and Bands −1, 0, +3, +5, +10.
    assert_eq!(
        device.equalizer,
        Some(Equalizer {
            preset: 0x10,
            bands: Some(vec![-1, 0, 3, 5, 10]),
            clear_bass: Some(2),
        })
    );
    assert!(!device.equalizer_unavailable);

    // Bass Boost, with the curve a real XM5 reports for it.
    let core = host.core();
    core.command(Command::SetEqualizer(address.clone(), preset(0x16)))
        .await
        .unwrap();
    let bass_boost = Equalizer {
        preset: 0x16,
        bands: Some(vec![0, 0, 0, 0, 0]),
        clear_bass: Some(7),
    };
    kotlin
        .simulated(|device| device.equalizer == Some(bass_boost.clone()))
        .await;

    // Onto Manual from Bass Boost's curve, then edited on Manual: a real XM5
    // only ACKs that, so the core reads it back.
    for clear_bass in [7, 3] {
        let manual = Equalizer {
            preset: 0xa0,
            bands: Some(vec![0, 4, 0, 0, 0]),
            clear_bass: Some(clear_bass),
        };
        core.command(Command::SetEqualizer(address.clone(), manual.clone()))
            .await
            .unwrap();
        kotlin
            .simulated(|device| device.equalizer == Some(manual.clone()))
            .await;
    }
}
