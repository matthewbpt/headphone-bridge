mod support;

use std::time::Duration;

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, LinkFailure, Observation};
use headphone_bridge_core::{
    Command, CommandError, Core, DeviceList, DeviceState, NoiseControlMode, NotAttachedReason,
};
use support::{buds4pro, fake_platform, fixture, received, replay, wait_for, xm5, SAMSUNG_SERVICE};

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
