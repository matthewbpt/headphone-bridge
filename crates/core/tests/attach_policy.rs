#![cfg(test)]

mod support;

use std::time::Duration;

use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{
    AttachPolicy, Command, CommandError, Core, DeviceList, DeviceState, NoiseControlMode,
    NotAttachedReason,
};
use support::{buds4pro, fake_platform, fixture, received, replay, wait_for, xm5, DeviceLink};

/// Whether the only Device in `list` is in `state`.
fn is(list: &DeviceList, state: DeviceState) -> bool {
    list.devices.first().map(|device| &device.state) == Some(&state)
}

/// Whether every Device in `list` is in `state`.
fn all(list: &DeviceList, state: DeviceState) -> bool {
    !list.devices.is_empty() && list.devices.iter().all(|device| device.state == state)
}

/// Plays the identifying frames of whichever Device `link` goes to, which
/// Attaches it.
async fn identify(link: &mut DeviceLink) {
    if link.address == buds4pro().address {
        let status = received(&fixture("galaxy-buds4-pro/capture-3"))[0].clone();
        link.send.send(status).unwrap();
    } else {
        replay(link, &fixture("wh-1000xm5/attach")).await;
    }
}

/// Whether the core has closed `link`, once it has read what's left on it.
async fn closed(link: &mut DeviceLink) -> bool {
    let drained = async { while link.written.recv().await.is_some() {} };
    tokio::time::timeout(Duration::from_secs(5), drained)
        .await
        .is_ok()
}

#[tokio::test(start_paused = true)]
async fn on_request_a_connected_device_stays_detached_with_no_control_link() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);

    os.observe(Observation::DeviceConnected(buds4pro()));

    wait_for(&core, |list| is(list, DeviceState::Detached)).await;
    assert!(!os.attempts_within(Duration::from_secs(600)).await);
}

#[tokio::test(start_paused = true)]
async fn holding_all_attaches_every_connected_device_until_the_hold_ends() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    os.observe(Observation::DeviceConnected(xm5()));
    wait_for(&core, |list| {
        list.devices.len() == 2 && all(list, DeviceState::Detached)
    })
    .await;

    let hold = core.hold_all();
    let mut links = vec![os.next_link().await, os.next_link().await];
    for link in &mut links {
        identify(link).await;
    }
    wait_for(&core, |list| all(list, DeviceState::Attached)).await;

    drop(hold);
    wait_for(&core, |list| all(list, DeviceState::Detached)).await;
    for link in &mut links {
        assert!(closed(link).await, "{} is still open", link.address);
    }
}

#[tokio::test(start_paused = true)]
async fn overlapping_holds_keep_devices_attached_until_the_last_ends() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let first = core.hold_all();
    os.next_attempt().await;
    let mut link = os.next_link().await;
    identify(&mut link).await;
    wait_for(&core, |list| is(list, DeviceState::Attached)).await;

    let second = core.hold_all();
    drop(first);
    assert!(!os.attempts_within(Duration::from_secs(60)).await);
    assert!(is(&core.device_list(), DeviceState::Attached));

    drop(second);
    wait_for(&core, |list| is(list, DeviceState::Detached)).await;
    assert!(closed(&mut link).await);
}

#[tokio::test(start_paused = true)]
async fn a_device_that_connects_during_a_hold_is_attached() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    let _hold = core.hold_all();

    os.observe(Observation::DeviceConnected(xm5()));

    let mut link = os.next_link().await;
    identify(&mut link).await;
    wait_for(&core, |list| is(list, DeviceState::Attached)).await;
}

#[tokio::test(start_paused = true)]
async fn a_device_detached_after_its_link_dropped_is_never_retried() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let hold = core.hold_all();
    os.next_attempt().await;
    let mut link = os.next_link().await;
    identify(&mut link).await;
    wait_for(&core, |list| is(list, DeviceState::Attached)).await;

    drop(link);
    wait_for(&core, |list| {
        is(
            list,
            DeviceState::NotAttached {
                reason: NotAttachedReason::LinkDropped,
            },
        )
    })
    .await;
    drop(hold);

    wait_for(&core, |list| is(list, DeviceState::Detached)).await;
    assert!(!os.attempts_within(Duration::from_secs(600)).await);
    assert!(is(&core.device_list(), DeviceState::Detached));
}

#[tokio::test(start_paused = true)]
async fn commands_to_a_detached_device_are_refused() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let hold = core.hold_all();
    let mut link = os.next_link().await;
    identify(&mut link).await;
    wait_for(&core, |list| is(list, DeviceState::Attached)).await;
    drop(hold);
    wait_for(&core, |list| is(list, DeviceState::Detached)).await;

    let address = buds4pro().address;
    assert_eq!(
        core.command(Command::SetNoiseControlMode(
            address,
            NoiseControlMode::Ambient
        ))
        .await,
        Err(CommandError::NotAttached)
    );
    assert_eq!(
        core.command(Command::SetAmbientLevel(address, 1)).await,
        Err(CommandError::NotAttached)
    );
}

#[tokio::test(start_paused = true)]
async fn ending_the_hold_leaves_a_power_off_to_finish_then_detaches_a_device_that_stayed() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let hold = core.hold_all();
    os.next_attempt().await;
    let mut link = os.next_link().await;
    identify(&mut link).await;
    wait_for(&core, |list| list.devices[0].ambient_level.is_some()).await;
    core.command(Command::PowerOff(xm5().address))
        .await
        .unwrap();
    link.written.recv().await;
    // The Device ACKs the Power Off, then stays Connected.
    link.send
        .send(vec![0x3e, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x3c])
        .unwrap();

    drop(hold);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        core.device_list().devices[0].state,
        DeviceState::PoweringOff
    );

    tokio::time::sleep(Duration::from_secs(10)).await;
    let list = core.device_list();
    assert!(list.devices[0].power_off_failed);
    assert_eq!(list.devices[0].state, DeviceState::Detached);
    assert!(closed(&mut link).await);
    assert!(!os.attempts_within(Duration::from_secs(600)).await);
}
