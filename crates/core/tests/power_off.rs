mod support;

use std::time::Duration;

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, Observation};
use headphone_bridge_core::{
    Command, CommandError, Core, DeviceState, Model, NoiseControlMode, NotAttachedReason,
};
use support::{
    buds4pro, fake_platform, fixture, received, replay, sony_frame, wait_for, with_equalizer_read,
    xm5, DeviceLink, FakeOs, SAMSUNG_SERVICE,
};

/// Starts a core with a WH-1000XM5 Attached through the whole attach fixture.
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
async fn power_off_sends_power_set_status_and_the_device_is_powering_off() {
    let (core, _os, mut link) = attached_xm5().await;

    core.command(Command::PowerOff(xm5().address))
        .await
        .unwrap();

    // POWER_SET_STATUS: power off, on, as libmdr and Gadgetbridge send it.
    assert_eq!(written(&mut link).await, sony_frame(0, &[0x24, 0x03, 0x01]));
    wait_for(&core, |list| {
        list.devices[0].state == DeviceState::PoweringOff
    })
    .await;
}

#[tokio::test]
async fn a_device_that_is_powering_off_refuses_every_command() {
    let (core, _os, mut link) = attached_xm5().await;
    core.command(Command::PowerOff(xm5().address))
        .await
        .unwrap();
    written(&mut link).await;

    let address = xm5().address;
    for command in [
        Command::PowerOff(address.clone()),
        Command::SetNoiseControlMode(address.clone(), NoiseControlMode::NoiseCancelling),
        Command::SetAmbientLevel(address.clone(), 15),
        Command::SetNoiseCancellingLevel(address.clone(), 2),
        Command::RetryAttach(address.clone()),
    ] {
        assert_eq!(
            core.command(command.clone()).await,
            Err(CommandError::PoweringOff),
            "{command:?}"
        );
    }
    assert!(link.written.try_recv().is_err(), "nothing more was written");
}

#[tokio::test(start_paused = true)]
async fn a_link_that_closes_while_powering_off_leaves_the_device_powering_off() {
    let (core, mut os, mut link) = attached_xm5().await;
    core.command(Command::PowerOff(xm5().address))
        .await
        .unwrap();
    written(&mut link).await;
    assert_eq!(os.next_attempt().await, xm5().address, "the first Attach");

    // The Device closes the control link as it switches off.
    drop(link);

    assert!(
        !os.attempts_within(Duration::from_secs(9)).await,
        "no reattach is scheduled"
    );
    assert_eq!(
        core.device_list().devices[0].state,
        DeviceState::PoweringOff
    );
}

#[tokio::test(start_paused = true)]
async fn a_device_whose_link_closed_before_the_ack_and_stays_connected_didnt_power_off() {
    let (core, mut os, mut link) = attached_xm5().await;
    core.command(Command::PowerOff(xm5().address))
        .await
        .unwrap();
    written(&mut link).await;
    assert_eq!(os.next_attempt().await, xm5().address, "the first Attach");

    // The link closes before the Device acknowledges, and it stays Connected.
    drop(link);
    tokio::time::sleep(Duration::from_secs(9)).await;
    assert_eq!(
        core.device_list().devices[0].state,
        DeviceState::PoweringOff
    );

    let list = wait_for(&core, |list| list.devices[0].power_off_failed).await;
    assert_eq!(list.devices[0].state, DeviceState::Attaching);
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;
}

#[tokio::test]
async fn a_device_that_is_powering_off_leaves_the_list_once_disconnected() {
    let (core, os, mut link) = attached_xm5().await;
    core.command(Command::PowerOff(xm5().address))
        .await
        .unwrap();
    written(&mut link).await;
    drop(link);

    os.observe(Observation::DeviceDisconnected(xm5().address));

    let list = wait_for(&core, |list| list.devices.is_empty()).await;
    assert_eq!(list.primary, None);
}

#[tokio::test(start_paused = true)]
async fn a_device_still_connected_10_s_after_acking_power_off_didnt_power_off() {
    let (core, _os, mut link) = attached_xm5().await;
    core.command(Command::PowerOff(xm5().address))
        .await
        .unwrap();
    written(&mut link).await;
    link.send.send(ACK_SEQ_1.to_vec()).unwrap();

    tokio::time::sleep(Duration::from_secs(9)).await;
    let device = &core.device_list().devices[0];
    assert_eq!(device.state, DeviceState::PoweringOff);
    assert!(!device.power_off_failed);

    tokio::time::sleep(Duration::from_secs(2)).await;
    let device = &core.device_list().devices[0];
    assert!(device.power_off_failed);
    assert_eq!(
        device.state,
        DeviceState::Attached,
        "its link is still open"
    );
    // Its controls work again.
    core.command(Command::SetAmbientLevel(xm5().address, 3))
        .await
        .unwrap();
    assert_eq!(
        written(&mut link).await,
        sony_frame(1, &[0x68, 0x17, 0x01, 0x01, 0x01, 0x01, 0x03])
    );
}

#[tokio::test(start_paused = true)]
async fn a_device_that_didnt_power_off_and_closed_its_link_is_attached_again() {
    let (core, mut os, mut link) = attached_xm5().await;
    core.command(Command::PowerOff(xm5().address))
        .await
        .unwrap();
    written(&mut link).await;
    link.send.send(ACK_SEQ_1.to_vec()).unwrap();
    assert_eq!(os.next_attempt().await, xm5().address, "the first Attach");
    tokio::time::sleep(Duration::from_secs(1)).await;
    drop(link);

    tokio::time::sleep(Duration::from_secs(8)).await;
    assert_eq!(
        core.device_list().devices[0].state,
        DeviceState::PoweringOff
    );
    let list = wait_for(&core, |list| {
        list.devices[0].power_off_failed && list.devices[0].state == DeviceState::Attaching
    })
    .await;
    assert!(
        list.devices[0].capabilities.power_off,
        "the window keeps showing ⏻, disabled"
    );
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    let list = wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;
    assert!(list.devices[0].power_off_failed, "until the next Power Off");
}

#[tokio::test(start_paused = true)]
async fn a_device_that_disconnects_before_the_timeout_reports_no_failure() {
    let (core, os, mut link) = attached_xm5().await;
    core.command(Command::PowerOff(xm5().address))
        .await
        .unwrap();
    written(&mut link).await;
    link.send.send(ACK_SEQ_1.to_vec()).unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;

    os.observe(Observation::DeviceDisconnected(xm5().address));
    wait_for(&core, |list| list.devices.is_empty()).await;
    // The same Device comes back later, as if switched on again.
    tokio::time::sleep(Duration::from_secs(10)).await;
    os.observe(Observation::DeviceConnected(xm5()));

    let list = wait_for(&core, |list| !list.devices.is_empty()).await;
    assert!(!list.devices[0].power_off_failed);
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert!(!core.device_list().devices[0].power_off_failed);
}

#[tokio::test(start_paused = true)]
async fn a_power_off_that_is_never_acknowledged_follows_the_no_ack_path() {
    let (core, mut os, mut link) = attached_xm5().await;
    core.command(Command::PowerOff(xm5().address))
        .await
        .unwrap();
    let power_off = sony_frame(0, &[0x24, 0x03, 0x01]);
    for _ in 0..10 {
        assert_eq!(written(&mut link).await, power_off);
    }

    wait_for(&core, |list| {
        list.devices[0].state
            == DeviceState::NotAttached {
                reason: NotAttachedReason::NoResponse,
            }
    })
    .await;
    assert!(!core.device_list().devices[0].power_off_failed);
    let mut link = os.next_link().await;
    replay(&mut link, &fixture("wh-1000xm5/attach")).await;
    wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;
}

#[tokio::test(start_paused = true)]
async fn an_xm5_keeps_its_power_off_capability_while_reattaching_but_refuses_it() {
    let (core, mut os, link) = attached_xm5().await;

    drop(link);
    let list = wait_for(&core, |list| {
        list.devices[0].state
            == DeviceState::NotAttached {
                reason: NotAttachedReason::LinkDropped,
            }
    })
    .await;
    assert_eq!(list.devices[0].model, Some(Model::Wh1000Xm5));
    assert!(list.devices[0].capabilities.power_off);
    assert_eq!(
        core.command(Command::PowerOff(xm5().address)).await,
        Err(CommandError::NotAttached)
    );

    let mut link = os.next_link().await;
    let list = wait_for(&core, |list| {
        list.devices[0].state == DeviceState::Attaching
    })
    .await;
    assert!(list.devices[0].capabilities.power_off);
    for command in [
        Command::PowerOff(xm5().address),
        Command::SetNoiseControlMode(xm5().address, NoiseControlMode::Ambient),
        Command::SetAmbientLevel(xm5().address, 3),
    ] {
        assert_eq!(
            core.command(command.clone()).await,
            Err(CommandError::NotAttached),
            "{command:?}"
        );
    }
    assert_eq!(
        written(&mut link).await,
        sony_frame(0, &[0x00, 0x00]),
        "only the attach's first read"
    );
    assert!(link.written.try_recv().is_err(), "nothing more was written");
}

#[tokio::test]
async fn a_buds4pro_refuses_power_off_as_a_missing_capability_and_is_sent_nothing() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(buds4pro()));
    let mut link = os.next_link().await;
    link.written.recv().await.expect("MANAGER_INFO");
    link.send
        .send(received(&fixture("galaxy-buds4-pro/capture-3"))[0].clone())
        .unwrap();
    wait_for(&core, |list| list.devices[0].state == DeviceState::Attached).await;

    assert_eq!(
        core.command(Command::PowerOff(buds4pro().address)).await,
        Err(CommandError::MissingCapability)
    );
    assert_eq!(core.device_list().devices[0].state, DeviceState::Attached);
    assert!(link.written.try_recv().is_err(), "nothing was written");
}

#[tokio::test]
async fn an_xm5_that_is_not_attached_refuses_power_off() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    os.observe(Observation::DeviceConnected(xm5()));
    // The link is open, but the Device hasn't identified itself.
    let _link = os.next_link().await;
    wait_for(&core, |list| {
        list.devices[0].state == DeviceState::Attaching
    })
    .await;

    assert_eq!(
        core.command(Command::PowerOff(xm5().address)).await,
        Err(CommandError::NotAttached)
    );
}

#[tokio::test]
async fn an_unsupported_model_refuses_power_off() {
    let (platform, os) = fake_platform();
    let core = Core::start(platform);
    let buds2 = DeviceIdentity {
        address: DeviceAddress("A1:B2:C3:00:00:02".into()),
        name: "Galaxy Buds2 (0002)".into(),
        service_uuids: vec![SAMSUNG_SERVICE],
    };
    os.observe(Observation::DeviceConnected(buds2.clone()));
    let unsupported = DeviceState::NotAttached {
        reason: NotAttachedReason::UnsupportedModel,
    };
    wait_for(&core, |list| {
        list.devices.first().map(|device| &device.state) == Some(&unsupported)
    })
    .await;

    assert_eq!(
        core.command(Command::PowerOff(buds2.address)).await,
        Err(CommandError::UnsupportedModel)
    );
}

/// An ACK with sequence number 1: for a frame sent with 0.
const ACK_SEQ_1: [u8; 9] = [0x3e, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x02, 0x3c];

/// `EQEBB_RET_PARAM`: Off, Clear Bass and every Band at 0.
const FLAT_OFF: [u8; 10] = [0x57, 0x00, 0x00, 0x06, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a];

/// The next frame the core writes.
async fn written(link: &mut DeviceLink) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(5), link.written.recv())
        .await
        .expect("timed out waiting for the core to write")
        .expect("link closed")
}

#[tokio::test]
async fn real_xm5_power_offs_replay_from_their_frame_log() {
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    let records = fixture("wh-1000xm5/real-power-off");
    // Each session starts with the first request of an Attach.
    let first_request = &records[0].bytes;
    let session_at: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, r)| &r.bytes == first_request)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(session_at.len(), 2);

    for (i, &start) in session_at.iter().enumerate() {
        let end = session_at.get(i + 1).copied().unwrap_or(records.len());
        // Recorded before the Equalizer was read on Attach, so re-based onto
        // an Attach that reads it: Off, flat.
        let session = &with_equalizer_read(&records[start..end], &FLAT_OFF);
        // The Power Off is the session's last sent data frame, then its ACK.
        let power_off_at = session.len() - 2;

        os.observe(Observation::DeviceConnected(xm5()));
        let mut link = os.next_link().await;
        replay(&mut link, &session[..power_off_at]).await;
        wait_for(&core, |list| list.devices[0].ambient_level.is_some()).await;
        core.command(Command::PowerOff(xm5().address))
            .await
            .unwrap();
        replay(&mut link, &session[power_off_at..]).await;
        assert_eq!(
            core.device_list().devices[0].state,
            DeviceState::PoweringOff
        );
        drop(link);

        os.observe(Observation::DeviceDisconnected(xm5().address));
        wait_for(&core, |list| list.devices.is_empty()).await;
    }
}
