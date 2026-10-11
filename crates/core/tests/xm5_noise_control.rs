mod support;

use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{Core, NoiseControlMode};
use support::{fake_platform, fixture, replay, wait_for, xm5, DeviceLink, FakeOs};

/// Starts a core with a WH-1000XM5 Attached through the whole attach
/// fixture, which reports Ambient, Focus on Voice on and Ambient Level 10.
async fn attached_xm5() -> (Core, FakeOs, DeviceLink) {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    wait_for(&core, |list| list.devices[0].ambient_level.is_some()).await;
    (core, os, link)
}

#[tokio::test]
async fn attaching_reads_the_chosen_mode_and_ambient_level() {
    let (core, _os, _link) = attached_xm5().await;

    let device = &core.device_list().devices[0];
    assert_eq!(device.chosen_mode, Some(NoiseControlMode::Ambient));
    assert_eq!(device.ambient_level, Some(10));
}
