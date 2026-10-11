mod support;

use std::time::Duration;

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, LinkFailure, Observation};
use headphone_bridge_core::{
    Command, CommandError, Core, DeviceList, DeviceState, NoiseControlMode, NotAttachedReason,
};
use support::{
    buds4pro, fake_platform, fixture, received, replay, sony_frame, wait_for, xm5, SAMSUNG_SERVICE,
};

/// Plays the Buds4 Pro's extended status on `link`, which Attaches it.
fn identify_buds4pro(link: &support::DeviceLink) {
    link.send
        .send(received(&fixture("galaxy-buds4-pro/capture-3"))[0].clone())
        .unwrap();
}

fn not_attached(reason: NotAttachedReason) -> DeviceState {
    DeviceState::NotAttached { reason }
}

/// Whether the only Device in `list` is in `state`.
fn is(list: &DeviceList, state: DeviceState) -> bool {
    list.devices.first().map(|device| &device.state) == Some(&state)
}

#[tokio::test(start_paused = true)]
async fn a_device_in_use_by_another_app_is_attached_on_retry_once_that_app_lets_go() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let address = buds4pro().address;
    os.fail_links(&address, LinkFailure::InUseByAnotherProcess);
    os.observe(Observation::DeviceConnected(buds4pro()));
    os.next_attempt().await;
    wait_for(&core, |list| {
        is(list, not_attached(NotAttachedReason::InUseByAnotherProcess))
    })
    .await;

    os.allow_links(&address);
    core.command(Command::RetryAttach(address.clone()))
        .await
        .unwrap();

    let link = os.next_link().await;
    identify_buds4pro(&link);
    wait_for(&core, |list| is(list, DeviceState::Attached)).await;
}

#[tokio::test(start_paused = true)]
async fn an_unsupported_model_is_never_retried_or_sent_a_command() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let buds2 = DeviceIdentity {
        address: DeviceAddress("A1:B2:C3:00:00:02".into()),
        name: "Galaxy Buds2 (0002)".into(),
        service_uuids: vec![SAMSUNG_SERVICE],
    };
    os.observe(Observation::DeviceConnected(buds2.clone()));
    wait_for(&core, |list| {
        is(list, not_attached(NotAttachedReason::UnsupportedModel))
    })
    .await;

    assert_eq!(
        core.command(Command::RetryAttach(buds2.address.clone()))
            .await,
        Err(CommandError::UnsupportedModel)
    );
    assert_eq!(
        core.command(Command::SetNoiseControlMode(
            buds2.address.clone(),
            NoiseControlMode::Off
        ))
        .await,
        Err(CommandError::UnsupportedModel)
    );
    assert!(
        !os.attempts_within(Duration::from_secs(600)).await,
        "no control link is ever opened to an unsupported Model"
    );
}

#[tokio::test(start_paused = true)]
async fn an_xm5_that_drops_its_link_when_idle_is_re_attached_after_a_pause() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    wait_for(&core, |list| list.devices[0].battery.is_some()).await;

    drop(link);
    let dropped_at = tokio::time::Instant::now();

    let list = wait_for(&core, |list| {
        is(list, not_attached(NotAttachedReason::LinkDropped))
    })
    .await;
    assert_eq!(list.devices[0].battery, None, "no stale values remain");
    let mut link = os.next_link().await;
    assert_eq!(dropped_at.elapsed(), Duration::from_secs(1));
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    wait_for(&core, |list| {
        is(list, DeviceState::Attached) && list.devices[0].battery.is_some()
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn attaching_is_retried_with_growing_pauses_then_left_to_the_retry_button() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.fail_links(&buds4pro().address, LinkFailure::ServiceNotFound);
    os.observe(Observation::DeviceConnected(buds4pro()));
    os.next_attempt().await;
    let start = tokio::time::Instant::now();

    let mut attempts = Vec::new();
    while os.attempts_within(Duration::from_secs(600)).await {
        attempts.push(start.elapsed().as_secs());
    }

    assert_eq!(attempts, [1, 3, 7, 15, 31, 61]);
    assert!(is(
        &core.device_list(),
        not_attached(NotAttachedReason::ServiceNotFound)
    ));
    os.allow_links(&buds4pro().address);
    core.command(Command::RetryAttach(buds4pro().address))
        .await
        .unwrap();
    let link = os.next_link().await;
    identify_buds4pro(&link);
    wait_for(&core, |list| is(list, DeviceState::Attached)).await;
}

#[tokio::test(start_paused = true)]
async fn failures_only_the_listener_can_fix_are_not_retried_automatically() {
    for failure in [
        LinkFailure::InUseByAnotherProcess,
        LinkFailure::BluetoothPermissionDenied,
    ] {
        let (platform, mut os) = fake_platform();
        let _core = Core::start(platform);
        os.fail_links(&buds4pro().address, failure.clone());
        os.observe(Observation::DeviceConnected(buds4pro()));
        os.next_attempt().await;

        assert!(
            !os.attempts_within(Duration::from_secs(600)).await,
            "{failure:?} was retried"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_disconnect_cancels_the_pending_retry_and_a_reconnect_attaches_straight_away() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let address = buds4pro().address;
    os.fail_links(&address, LinkFailure::DeviceNotConnected);
    os.observe(Observation::DeviceConnected(buds4pro()));
    os.next_attempt().await;

    os.observe(Observation::DeviceDisconnected(address.clone()));
    wait_for(&core, |list| list.devices.is_empty()).await;
    assert!(!os.attempts_within(Duration::from_secs(600)).await);

    os.allow_links(&address);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;
    identify_buds4pro(&link);
    let list = wait_for(&core, |list| is(list, DeviceState::Attached)).await;
    assert!(list.devices[0].battery.is_some());
}

#[tokio::test(start_paused = true)]
async fn another_sony_model_is_not_retried() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let wh1000xm6 = DeviceIdentity {
        address: DeviceAddress("AC:80:0A:44:55:66".into()),
        name: "WH-1000XM6".into(),
        service_uuids: vec![support::SONY_SERVICE],
    };
    os.observe(Observation::DeviceConnected(wh1000xm6.clone()));
    os.next_attempt().await;
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")[..5]).await;
    link.send
        .send(support::sony_frame(1, b"\x05\x01\x0aWH-1000XM6"))
        .unwrap();
    wait_for(&core, |list| {
        is(list, not_attached(NotAttachedReason::UnsupportedModel))
    })
    .await;

    assert!(!os.attempts_within(Duration::from_secs(600)).await);
    assert_eq!(
        core.command(Command::RetryAttach(wh1000xm6.address)).await,
        Err(CommandError::UnsupportedModel)
    );
}

#[tokio::test(start_paused = true)]
async fn an_xm5_that_never_answers_is_given_up_on_and_retried() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    let opened_at = tokio::time::Instant::now();
    // The protocol info request, whose ACK never comes, sent 10 times.
    for _ in 0..10 {
        link.written.recv().await.unwrap();
    }

    assert_eq!(link.written.recv().await, None, "the link is closed");
    assert_eq!(opened_at.elapsed(), Duration::from_secs(10));
    wait_for(&core, |list| {
        is(list, not_attached(NotAttachedReason::NoResponse))
    })
    .await;

    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    wait_for(&core, |list| is(list, DeviceState::Attached)).await;
    tokio::time::sleep(Duration::from_secs(600)).await;
    assert!(
        is(&core.device_list(), DeviceState::Attached),
        "an identified Device stays Attached however long it's quiet"
    );
}

/// An ACK with sequence number 0: for a frame sent with 1.
const ACK_SEQ_0: [u8; 9] = [0x3e, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x3c];

/// Starts a core with a WH-1000XM5 Attached through the whole attach fixture.
async fn attached_xm5() -> (Core, support::FakeOs, support::DeviceLink) {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    wait_for(&core, |list| list.devices[0].ambient_level.is_some()).await;
    (core, os, link)
}

#[tokio::test(start_paused = true)]
async fn an_xm5_request_whose_ack_is_lost_is_resent_after_a_second() {
    let (core, _os, mut link) = attached_xm5().await;
    let off = sony_frame(1, &[0x68, 0x17, 0x01, 0x00, 0x01, 0x01, 0x0a]);

    core.command(Command::SetNoiseControlMode(
        xm5().address,
        NoiseControlMode::Off,
    ))
    .await
    .unwrap();
    core.command(Command::SetAmbientLevel(xm5().address, 3))
        .await
        .unwrap();
    assert_eq!(written(&mut link).await.unwrap(), off);
    let sent_at = tokio::time::Instant::now();

    assert_eq!(
        written(&mut link).await.unwrap(),
        off,
        "the same frame again"
    );
    assert_eq!(sent_at.elapsed(), Duration::from_secs(1));
    // Once it's ACKed, the queued level set goes out.
    link.send.send(ACK_SEQ_0.to_vec()).unwrap();
    assert_eq!(
        written(&mut link).await.unwrap(),
        sony_frame(0, &[0x68, 0x17, 0x01, 0x00, 0x01, 0x01, 0x03])
    );
}

#[tokio::test(start_paused = true)]
async fn an_xm5_that_stops_acking_is_detached_after_10_tries_and_re_attached() {
    let (core, mut os, mut link) = attached_xm5().await;
    let off = sony_frame(1, &[0x68, 0x17, 0x01, 0x00, 0x01, 0x01, 0x0a]);

    core.command(Command::SetNoiseControlMode(
        xm5().address,
        NoiseControlMode::Off,
    ))
    .await
    .unwrap();
    let sent_at = tokio::time::Instant::now();
    for _ in 0..10 {
        assert_eq!(written(&mut link).await.unwrap(), off);
    }

    assert_eq!(written(&mut link).await, None, "the link is closed");
    assert_eq!(sent_at.elapsed(), Duration::from_secs(10));
    wait_for(&core, |list| {
        is(list, not_attached(NotAttachedReason::NoResponse))
    })
    .await;
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    wait_for(&core, |list| is(list, DeviceState::Attached)).await;
}

/// The next frame the core writes, or `None` once the link is closed.
async fn written(link: &mut support::DeviceLink) -> Option<Vec<u8>> {
    tokio::time::timeout(Duration::from_secs(60), link.written.recv())
        .await
        .expect("timed out waiting for the core to write")
}
