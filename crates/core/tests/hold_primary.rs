#![cfg(test)]

mod support;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, Observation};
use headphone_bridge_core::{AttachPolicy, Core, Device, DeviceList, DeviceState};
use support::{buds4pro, fake_platform, fixture, received, replay, wait_for, xm5, DeviceLink};

/// The state of the Device `identity` in `list`.
fn state_of(list: &DeviceList, identity: &DeviceIdentity) -> DeviceState {
    list.devices
        .iter()
        .find(|device| device.address == identity.address)
        .map(|device| device.state.clone())
        .expect("the Device is in the list")
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
async fn the_primary_device_is_chosen_among_detached_devices_by_the_audio_output() {
    let (platform, os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    os.observe(Observation::DeviceConnected(xm5()));

    os.observe(Observation::AudioOutputChanged(Some(buds4pro().address)));

    let list = wait_for(&core, |list| list.primary == Some(buds4pro().address)).await;
    assert!(list
        .devices
        .iter()
        .all(|device| device.state == DeviceState::Detached));
}

#[tokio::test(start_paused = true)]
async fn with_no_bluetooth_audio_output_the_most_recently_shown_device_is_primary() {
    let (platform, os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    os.observe(Observation::DeviceConnected(xm5()));

    let list = wait_for(&core, |list| list.devices.len() == 2).await;
    assert_eq!(list.primary, Some(xm5().address));
}

#[tokio::test(start_paused = true)]
async fn an_unsupported_model_is_never_primary() {
    let (platform, os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    let buds3pro = DeviceIdentity {
        address: DeviceAddress("AA:BB:CC:DD:EE:03".into()),
        name: "Galaxy Buds3 Pro (A1B2)".into(),
        ..buds4pro()
    };
    os.observe(Observation::DeviceConnected(buds4pro()));
    os.observe(Observation::DeviceConnected(buds3pro.clone()));

    os.observe(Observation::AudioOutputChanged(Some(buds3pro.address)));

    let list = wait_for(&core, |list| list.devices.len() == 2).await;
    assert_eq!(list.primary, Some(buds4pro().address));
}

#[tokio::test(start_paused = true)]
async fn holding_the_primary_device_attaches_only_it_until_the_hold_ends() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    os.observe(Observation::DeviceConnected(xm5()));
    os.observe(Observation::AudioOutputChanged(Some(buds4pro().address)));
    wait_for(&core, |list| list.primary == Some(buds4pro().address)).await;

    let hold = core.hold_primary().expect("a Primary Device");
    assert_eq!(hold.device(), Some(&buds4pro().address));
    assert_eq!(os.next_attempt().await, buds4pro().address);
    let mut link = os.next_link().await;
    assert_eq!(link.address, buds4pro().address);
    identify(&mut link).await;
    let list = wait_for(&core, |list| {
        state_of(list, &buds4pro()) == DeviceState::Attached
    })
    .await;
    assert_eq!(state_of(&list, &xm5()), DeviceState::Detached);
    assert!(!os.attempts_within(Duration::from_secs(600)).await);

    drop(hold);
    wait_for(&core, |list| {
        state_of(list, &buds4pro()) == DeviceState::Detached
    })
    .await;
    assert!(closed(&mut link).await);
}

#[tokio::test(start_paused = true)]
async fn with_no_device_connected_there_is_nothing_to_hold() {
    let (platform, _os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);

    assert!(core.hold_primary().is_none());
}

#[tokio::test(start_paused = true)]
async fn the_hold_stays_on_the_device_it_attached_when_the_primary_device_changes() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    os.observe(Observation::DeviceConnected(xm5()));
    os.observe(Observation::AudioOutputChanged(Some(buds4pro().address)));
    wait_for(&core, |list| list.primary == Some(buds4pro().address)).await;
    let _hold = core.hold_primary().expect("a Primary Device");
    os.next_attempt().await;
    let mut link = os.next_link().await;
    identify(&mut link).await;
    wait_for(&core, |list| {
        state_of(list, &buds4pro()) == DeviceState::Attached
    })
    .await;

    os.observe(Observation::AudioOutputChanged(Some(xm5().address)));

    let list = wait_for(&core, |list| list.primary == Some(xm5().address)).await;
    assert_eq!(state_of(&list, &buds4pro()), DeviceState::Attached);
    assert_eq!(state_of(&list, &xm5()), DeviceState::Detached);
    assert!(!os.attempts_within(Duration::from_secs(600)).await);
}

#[tokio::test(start_paused = true)]
async fn ending_a_hold_on_all_leaves_a_held_primary_device_attached() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    os.observe(Observation::DeviceConnected(xm5()));
    os.observe(Observation::AudioOutputChanged(Some(xm5().address)));
    wait_for(&core, |list| list.primary == Some(xm5().address)).await;
    let all = core.hold_all();
    let mut links = vec![os.next_link().await, os.next_link().await];
    for link in &mut links {
        identify(link).await;
    }
    wait_for(&core, |list| {
        list.devices
            .iter()
            .all(|device| device.state == DeviceState::Attached)
    })
    .await;

    let _primary = core.hold_primary().expect("a Primary Device");
    drop(all);

    let list = wait_for(&core, |list| {
        state_of(list, &buds4pro()) == DeviceState::Detached
    })
    .await;
    assert_eq!(state_of(&list, &xm5()), DeviceState::Attached);
}

#[tokio::test(start_paused = true)]
async fn a_detached_device_keeps_its_last_reported_values_and_when_they_were_reported() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    wait_for(&core, |list| list.primary == Some(xm5().address)).await;
    let before = SystemTime::now();
    let hold = core.hold_primary().expect("a Primary Device");
    let mut link = os.next_link().await;
    identify(&mut link).await;
    let attached = wait_for(&core, |list| {
        list.devices[0].ambient_level.is_some() && list.devices[0].battery.is_some()
    })
    .await
    .devices[0]
        .clone();
    let after = SystemTime::now();
    assert_eq!(
        attached.reported_at, None,
        "while Attached, values are current"
    );

    drop(hold);

    let list = wait_for(&core, |list| list.devices[0].state == DeviceState::Detached).await;
    let detached = &list.devices[0];
    assert_eq!(
        Device {
            state: DeviceState::Detached,
            reported_at: None,
            ..detached.clone()
        },
        Device {
            state: DeviceState::Detached,
            ..attached.clone()
        }
    );
    assert!(detached.model.is_some() && detached.chosen_mode.is_some());
    let reported_at = detached.reported_at.expect("when they were reported");
    let millis = |time: SystemTime| time.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
    assert!((millis(before)..=millis(after)).contains(&reported_at));
}

#[tokio::test(start_paused = true)]
async fn a_detached_devices_values_are_dropped_when_it_stops_being_connected() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    wait_for(&core, |list| list.primary == Some(xm5().address)).await;
    let hold = core.hold_primary().expect("a Primary Device");
    let mut link = os.next_link().await;
    identify(&mut link).await;
    wait_for(&core, |list| list.devices[0].ambient_level.is_some()).await;
    drop(hold);
    wait_for(&core, |list| list.devices[0].state == DeviceState::Detached).await;

    os.observe(Observation::DeviceDisconnected(xm5().address));
    wait_for(&core, |list| list.devices.is_empty()).await;
    os.observe(Observation::DeviceConnected(xm5()));

    let list = wait_for(&core, |list| list.devices.len() == 1).await;
    let device = &list.devices[0];
    assert_eq!(device.state, DeviceState::Detached);
    assert_eq!(device.model, None);
    assert_eq!(device.chosen_mode, None);
    assert_eq!(device.battery, None);
    assert_eq!(device.reported_at, None);
}

#[tokio::test(start_paused = true)]
async fn kept_values_give_way_once_the_device_is_attached_again() {
    let (platform, mut os) = fake_platform();
    let core = Core::builder()
        .attach_policy(AttachPolicy::OnRequest)
        .start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    wait_for(&core, |list| list.primary == Some(xm5().address)).await;
    let hold = core.hold_primary().expect("a Primary Device");
    let mut link = os.next_link().await;
    identify(&mut link).await;
    wait_for(&core, |list| list.devices[0].chosen_mode.is_some()).await;
    drop(hold);
    wait_for(&core, |list| list.devices[0].reported_at.is_some()).await;

    let _hold = core.hold_primary().expect("a Primary Device");
    let mut link = os.next_link().await;
    // Up to the Model name: the Device has yet to report its settings.
    replay(&mut link, &fixture("wh-1000xm5/attach")[..16]).await;

    let list = wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;
    assert_eq!(list.devices[0].reported_at, None);
    assert_eq!(list.devices[0].chosen_mode, None);
}
