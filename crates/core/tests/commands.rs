#![cfg(test)]

mod support;

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{CommandError, CommandKind, Core, NoiseControlMode};
use support::{device_command, fake_platform};
use tokio::sync::mpsc;

#[tokio::test]
async fn a_command_for_a_device_that_is_not_connected_is_rejected() {
    let (platform, observations, _os) = fake_platform();
    let core = Core::builder().start(platform, observations);
    let unknown = DeviceAddress::parse("AA:BB:CC:DD:EE:FF").unwrap();

    for command in [
        device_command(unknown, CommandKind::RetryAttach),
        device_command(
            unknown,
            CommandKind::SetNoiseControlMode {
                mode: NoiseControlMode::NoiseCancelling,
            },
        ),
        device_command(unknown, CommandKind::SetAmbientLevel { level: 2 }),
        device_command(unknown, CommandKind::SetNoiseCancellingLevel { level: 2 }),
        device_command(unknown, CommandKind::PowerOff),
    ] {
        assert_eq!(
            core.command(command.clone()).await,
            Err(CommandError::UnknownDevice),
            "{command:?}"
        );
    }
}

#[tokio::test]
async fn a_core_with_nothing_to_observe_still_serves_holds_and_commands() {
    let (platform, _observations, _os) = fake_platform();
    let (_, ended) = mpsc::unbounded_channel();
    let core = Core::builder().start(platform, ended);
    let unknown = DeviceAddress::parse("AA:BB:CC:DD:EE:FF").unwrap();

    // The core serves requests in order, so an answer to the command shows
    // it took and ended the Hold without stopping.
    drop(core.hold_all());

    assert_eq!(
        core.command(device_command(unknown, CommandKind::PowerOff))
            .await,
        Err(CommandError::UnknownDevice)
    );
}
