mod support;

use headphone_bridge_core::platform::{DeviceIdentity, Observation};
use headphone_bridge_core::{Command, Core, DeviceState, NoiseControlMode};
use support::{
    buds4pro, fake_platform, fixture, received, replay, wait_for, xm5, DeviceLink, FakeOs,
};

/// `NOISE_CONTROLS` (0x78) with Noise Cancelling (1).
const SET_NOISE_CANCELLING: [u8; 8] = [0xfd, 0x04, 0x00, 0x78, 0x01, 0xd1, 0x91, 0xdd];

/// Connects the Buds4 Pro and plays its Device up to Attached. The link stays
/// open while the returned end is held.
async fn attach_buds4pro(core: &Core, os: &mut FakeOs) -> DeviceLink {
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;
    let frames = received(&fixture("galaxy-buds4-pro/capture-3"));
    link.send.send(frames[0].clone()).unwrap();
    wait_until_attached(core, &buds4pro()).await;
    link
}

/// Connects the WH-1000XM5 and plays its Device up to Attached.
async fn attach_xm5(core: &Core, os: &mut FakeOs) -> DeviceLink {
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")[..16]).await;
    wait_until_attached(core, &xm5()).await;
    link
}

async fn wait_until_attached(core: &Core, identity: &DeviceIdentity) {
    wait_for(core, |list| {
        list.devices.iter().any(|device| {
            device.address == identity.address && device.state == DeviceState::Attached
        })
    })
    .await;
}

#[tokio::test]
async fn with_nothing_attached_there_is_no_primary_device() {
    let (platform, os) = fake_platform();
    let core = Core::start(platform);

    os.observe(Observation::AudioOutputChanged(Some(xm5().address)));

    assert_eq!(core.device_list().primary, None);
}

#[tokio::test]
async fn the_only_attached_device_is_the_primary_device() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);

    let _xm5 = attach_xm5(&core, &mut os).await;

    assert_eq!(core.device_list().primary, Some(xm5().address));
}

#[tokio::test]
async fn with_two_devices_attached_the_primary_device_is_the_one_playing_audio() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let _buds = attach_buds4pro(&core, &mut os).await;
    let _xm5 = attach_xm5(&core, &mut os).await;

    os.observe(Observation::AudioOutputChanged(Some(buds4pro().address)));

    let list = wait_for(&core, |list| list.primary == Some(buds4pro().address)).await;
    assert_eq!(list.devices.len(), 2);
}

#[tokio::test]
async fn the_primary_device_follows_the_audio_output_when_it_switches() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let _buds = attach_buds4pro(&core, &mut os).await;
    let _xm5 = attach_xm5(&core, &mut os).await;
    os.observe(Observation::AudioOutputChanged(Some(buds4pro().address)));
    wait_for(&core, |list| list.primary == Some(buds4pro().address)).await;

    os.observe(Observation::AudioOutputChanged(Some(xm5().address)));

    wait_for(&core, |list| list.primary == Some(xm5().address)).await;
}

#[tokio::test]
async fn when_the_audio_output_is_not_bluetooth_the_most_recently_attached_device_is_primary() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let _buds = attach_buds4pro(&core, &mut os).await;
    let _xm5 = attach_xm5(&core, &mut os).await;
    os.observe(Observation::AudioOutputChanged(Some(buds4pro().address)));
    wait_for(&core, |list| list.primary == Some(buds4pro().address)).await;

    // The built-in speakers, say.
    os.observe(Observation::AudioOutputChanged(None));

    wait_for(&core, |list| list.primary == Some(xm5().address)).await;
}

#[tokio::test]
async fn when_the_audio_output_is_a_device_that_is_not_attached_the_most_recently_attached_device_is_primary(
) {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let _xm5 = attach_xm5(&core, &mut os).await;
    // The Buds4 Pro are Connected, but still Attaching.
    os.observe(Observation::DeviceConnected(buds4pro()));
    let _buds = os.next_link().await;

    os.observe(Observation::AudioOutputChanged(Some(buds4pro().address)));

    let list = wait_for(&core, |list| list.devices.len() == 2).await;
    assert_eq!(list.primary, Some(xm5().address));
}

#[tokio::test]
async fn a_device_playing_audio_becomes_primary_once_it_is_attached() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::AudioOutputChanged(Some(buds4pro().address)));

    let _buds = attach_buds4pro(&core, &mut os).await;
    let _xm5 = attach_xm5(&core, &mut os).await;

    assert_eq!(core.device_list().primary, Some(buds4pro().address));
}

#[tokio::test]
async fn when_the_primary_device_disconnects_the_other_attached_device_becomes_primary() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let _buds = attach_buds4pro(&core, &mut os).await;
    let _xm5 = attach_xm5(&core, &mut os).await;
    os.observe(Observation::AudioOutputChanged(Some(xm5().address)));
    wait_for(&core, |list| list.primary == Some(xm5().address)).await;

    os.observe(Observation::DeviceDisconnected(xm5().address));

    let list = wait_for(&core, |list| list.devices.len() == 1).await;
    assert_eq!(list.primary, Some(buds4pro().address));
}

#[tokio::test]
async fn when_the_only_attached_device_disconnects_there_is_no_primary_device() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let _xm5 = attach_xm5(&core, &mut os).await;
    os.observe(Observation::AudioOutputChanged(Some(xm5().address)));

    os.observe(Observation::DeviceDisconnected(xm5().address));

    let list = wait_for(&core, |list| list.devices.is_empty()).await;
    assert_eq!(list.primary, None);
}

#[tokio::test]
async fn when_the_primary_devices_control_link_drops_the_other_attached_device_becomes_primary() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let _buds = attach_buds4pro(&core, &mut os).await;
    let xm5_link = attach_xm5(&core, &mut os).await;
    os.observe(Observation::AudioOutputChanged(Some(xm5().address)));
    wait_for(&core, |list| list.primary == Some(xm5().address)).await;

    drop(xm5_link);

    wait_for(&core, |list| list.primary == Some(buds4pro().address)).await;
}

#[tokio::test]
async fn a_device_that_is_not_primary_can_still_be_controlled() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let mut buds = attach_buds4pro(&core, &mut os).await;
    let _xm5 = attach_xm5(&core, &mut os).await;
    os.observe(Observation::AudioOutputChanged(Some(xm5().address)));
    wait_for(&core, |list| list.primary == Some(xm5().address)).await;
    buds.written.recv().await.expect("MANAGER_INFO");

    core.command(Command::SetNoiseControlMode(
        buds4pro().address,
        NoiseControlMode::NoiseCancelling,
    ))
    .await
    .unwrap();

    assert_eq!(buds.written.recv().await.unwrap(), SET_NOISE_CANCELLING);
}
