#![cfg(test)]

mod support;

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{CommandError, CommandKind, Core, NoiseControlMode};
use support::{device_command, fake_platform};

#[tokio::test]
async fn a_command_for_a_device_that_is_not_connected_is_rejected() {
    let (platform, _os) = fake_platform();
    let core = Core::builder().start(platform);
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
