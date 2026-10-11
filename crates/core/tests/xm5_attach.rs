mod support;

use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{
    AmbientLevelRange, Battery, Capabilities, Core, DeviceState, Model, NoiseControlMode,
};
use support::{fake_platform, fixture, replay, wait_for, xm5, SONY_SERVICE};

/// `CONNECT_GET_PROTOCOL_INFO` with sequence number 0.
const GET_PROTOCOL_INFO: [u8; 11] = [
    0x3e, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x0e, 0x3c,
];

#[tokio::test]
async fn a_connected_xm5_is_attached_on_the_v2_service_by_asking_for_its_protocol_info() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);

    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;

    assert_eq!(link.address, xm5().address);
    assert_eq!(link.service, SONY_SERVICE);
    assert_eq!(link.written.recv().await.unwrap(), GET_PROTOCOL_INFO);
    let list = core.device_list();
    assert_eq!(list.devices.len(), 1);
    assert_eq!(list.devices[0].name, "WH-1000XM5");
    assert_eq!(list.devices[0].state, DeviceState::Attaching);
}

#[tokio::test]
async fn the_model_name_identifies_the_xm5_and_its_battery_is_read() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;

    // Up to and including the reply to the battery GET: 70 %.
    let records = fixture("wh-1000xm5/attach");
    replay(&mut link, &records[..16]).await;

    let list = wait_for(&core, |list| list.devices[0].battery.is_some()).await;
    let device = &list.devices[0];
    assert_eq!(device.state, DeviceState::Attached);
    assert_eq!(device.model, Some(Model::Wh1000Xm5));
    assert_eq!(
        device.capabilities,
        Capabilities {
            noise_control_modes: vec![
                NoiseControlMode::Off,
                NoiseControlMode::NoiseCancelling,
                NoiseControlMode::Ambient,
            ],
            ambient_level: Some(AmbientLevelRange { min: 1, max: 20 }),
        }
    );
    assert_eq!(device.battery, Some(Battery::Headphones { level: 70 }));
}

#[tokio::test]
async fn battery_follows_notifications() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;

    // The fixture ends with a notification of 69 %, which the core ACKs.
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;

    wait_for(&core, |list| {
        list.devices[0].battery == Some(Battery::Headphones { level: 69 })
    })
    .await;
}
