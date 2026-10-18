#![cfg(test)]

mod support;

use headphone_bridge_core::platform::DeviceAddress;
use headphone_bridge_core::{Command, CommandError, Core, NoiseControlMode};
use support::fake_platform;

#[tokio::test]
async fn a_command_for_a_device_that_is_not_connected_is_rejected() {
    let (platform, _os) = fake_platform();
    let core = Core::builder().start(platform);
    let unknown = DeviceAddress::parse("AA:BB:CC:DD:EE:FF").unwrap();

    for command in [
        Command::RetryAttach(unknown),
        Command::SetNoiseControlMode(unknown, NoiseControlMode::NoiseCancelling),
        Command::SetAmbientLevel(unknown, 2),
        Command::SetNoiseCancellingLevel(unknown, 2),
        Command::PowerOff(unknown),
    ] {
        assert_eq!(
            core.command(command.clone()).await,
            Err(CommandError::UnknownDevice),
            "{command:?}"
        );
    }
}
