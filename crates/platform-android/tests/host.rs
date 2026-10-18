//! The host as Kotlin sees it: JSON events and commands in, JSON messages out,
//! with a real core behind it.

#![cfg(test)]

use std::path::PathBuf;
use std::time::Duration;

use headphone_bridge_core::frame_log::{Direction, FrameRecord};
use headphone_bridge_core::platform::LinkFailure;
use headphone_bridge_core::{Command, CommandError, DeviceList, DeviceState, NotAttachedReason};
use headphone_bridge_platform_android::messages::{CommandFailure, Message};
use headphone_bridge_platform_android::Host;
use tokio::sync::mpsc;

const XM5: &str = "AC:80:0A:11:22:33";
const BUDS: &str = "A1:B2:C3:D4:E5:F6";

/// Kotlin's side of the boundary: every message the host sends it.
struct Kotlin {
    messages: mpsc::UnboundedReceiver<Message>,
}

fn start() -> (Host, Kotlin) {
    let (sink, messages) = mpsc::unbounded_channel();
    let host = Host::start(move |message| sink.send(message).is_ok());
    (host, Kotlin { messages })
}

fn connect_xm5(host: &Host) {
    host.event(&format!(
        r#"{{"type":"connected","address":"{XM5}","name":"WH-1000XM5",
             "uuids":["956c7b26-d49a-4ba8-b03f-b17d393cb6e2"]}}"#
    ))
    .unwrap();
}

impl Kotlin {
    /// The next message that `wanted` picks out, skipping any others.
    async fn next<T>(&mut self, mut wanted: impl FnMut(Message) -> Option<T>) -> T {
        let wait = async {
            loop {
                let message = self.messages.recv().await.expect("host stopped");
                if let Some(found) = wanted(message) {
                    return found;
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(5), wait)
            .await
            .expect("timed out waiting for a message")
    }

    async fn device_list(&mut self, condition: impl Fn(&DeviceList) -> bool) -> DeviceList {
        self.next(|message| match message {
            Message::DeviceList { list } if condition(&list) => Some(list),
            _ => None,
        })
        .await
    }

    /// Whether the host asked to open a control link since last asked.
    fn asked_to_open(&mut self) -> bool {
        std::iter::from_fn(|| self.messages.try_recv().ok())
            .any(|message| matches!(message, Message::Open { .. }))
    }
}

fn state_of(list: &DeviceList, address: &str) -> Option<DeviceState> {
    list.devices
        .iter()
        .find(|device| device.address.0 == address)
        .map(|device| device.state.clone())
}

#[tokio::test]
async fn a_connected_device_stays_detached_until_something_holds_it() {
    let (host, mut kotlin) = start();
    connect_xm5(&host);
    kotlin
        .device_list(|list| state_of(list, XM5) == Some(DeviceState::Detached))
        .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!kotlin.asked_to_open());
}

/// The XM5's Attach, from the core's fixtures.
fn xm5_attach() -> Vec<FrameRecord> {
    let text = include_str!("../../core/tests/fixtures/wh-1000xm5/attach.frames");
    FrameRecord::parse_all(text).unwrap()
}

impl Kotlin {
    /// Waits for the host to ask for a control link to `address`; returns the
    /// request.
    async fn open_asked(&mut self, address: &str) -> u64 {
        self.next(|message| match message {
            Message::Open {
                request,
                address: asked,
                ..
            } if asked.0 == address => Some(request),
            _ => None,
        })
        .await
    }

    /// Plays a recorded session on `link`, as the Device at the other end of
    /// the socket.
    async fn replay(&mut self, host: &Host, link: i32, records: &[FrameRecord]) {
        for record in records {
            match record.direction {
                Direction::Received => host
                    .event(&format!(
                        r#"{{"type":"data","link":{link},"bytes":{:?}}}"#,
                        record.bytes
                    ))
                    .unwrap(),
                Direction::Sent => {
                    let written = self
                        .next(|message| match message {
                            Message::Write { link: on, bytes } if on == link => Some(bytes),
                            _ => None,
                        })
                        .await;
                    assert_eq!(written, record.bytes, "expected {record}");
                }
            }
        }
    }
}

#[tokio::test]
async fn a_held_device_attaches_over_the_link_kotlin_opens() {
    let (host, mut kotlin) = start();
    connect_xm5(&host);
    host.event(r#"{"type":"holdAll"}"#).unwrap();
    let request = kotlin.open_asked(XM5).await;
    host.event(&format!(
        r#"{{"type":"opened","request":{request},"link":1}}"#
    ))
    .unwrap();
    kotlin.replay(&host, 1, &xm5_attach()).await;
    kotlin
        .device_list(|list| state_of(list, XM5) == Some(DeviceState::Attached))
        .await;
}

impl Kotlin {
    async fn closed(&mut self, link: i32) {
        self.next(|message| {
            matches!(message, Message::Close { link: closed } if closed == link).then_some(())
        })
        .await
    }
}

#[tokio::test]
async fn ending_the_hold_closes_the_link_and_detaches() {
    let (host, mut kotlin) = start();
    connect_xm5(&host);
    host.event(r#"{"type":"holdAll"}"#).unwrap();
    let request = kotlin.open_asked(XM5).await;
    host.event(&format!(
        r#"{{"type":"opened","request":{request},"link":1}}"#
    ))
    .unwrap();
    kotlin.replay(&host, 1, &xm5_attach()).await;
    host.event(r#"{"type":"endHoldAll"}"#).unwrap();
    // In either order.
    let (mut closed, mut detached) = (false, false);
    kotlin
        .next(|message| {
            match message {
                Message::Close { link: 1 } => closed = true,
                Message::DeviceList { list } => {
                    detached = state_of(&list, XM5) == Some(DeviceState::Detached)
                }
                _ => {}
            }
            (closed && detached).then_some(())
        })
        .await;
}

#[tokio::test]
async fn a_link_opened_after_the_hold_ended_is_closed() {
    let (host, mut kotlin) = start();
    connect_xm5(&host);
    host.event(r#"{"type":"holdAll"}"#).unwrap();
    let request = kotlin.open_asked(XM5).await;
    host.event(r#"{"type":"endHoldAll"}"#).unwrap();
    kotlin
        .device_list(|list| state_of(list, XM5) == Some(DeviceState::Detached))
        .await;
    host.event(&format!(
        r#"{{"type":"opened","request":{request},"link":2}}"#
    ))
    .unwrap();
    kotlin.closed(2).await;
}

#[tokio::test]
async fn without_the_nearby_devices_permission_a_device_is_not_attached_for_want_of_it() {
    let (host, mut kotlin) = start();
    connect_xm5(&host);
    host.event(r#"{"type":"holdAll"}"#).unwrap();
    let request = kotlin.open_asked(XM5).await;
    host.event(&format!(
        r#"{{"type":"openFailed","request":{request},"reason":"bluetoothPermissionDenied"}}"#
    ))
    .unwrap();
    kotlin
        .device_list(|list| {
            state_of(list, XM5)
                == Some(DeviceState::NotAttached {
                    reason: NotAttachedReason::LinkFailed(LinkFailure::BluetoothPermissionDenied),
                })
        })
        .await;
}

#[tokio::test]
async fn a_hold_on_the_primary_device_attaches_only_the_one_playing_audio() {
    let (host, mut kotlin) = start();
    connect_xm5(&host);
    host.event(&format!(
        r#"{{"type":"connected","address":"{BUDS}","name":"Galaxy Buds4 Pro",
             "uuids":["2e73a4ad-332d-41fc-90e2-16bef06523f2"]}}"#
    ))
    .unwrap();
    host.event(&format!(r#"{{"type":"audioOutput","address":"{XM5}"}}"#))
        .unwrap();
    kotlin
        .device_list(|list| {
            list.primary
                .as_ref()
                .is_some_and(|primary| primary.0 == XM5)
        })
        .await;
    host.event(r#"{"type":"holdPrimary"}"#).unwrap();
    kotlin.open_asked(XM5).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!kotlin.asked_to_open(), "only the Primary Device is held");
    assert_eq!(
        state_of(&host.core().device_list(), BUDS),
        Some(DeviceState::Detached)
    );
}

impl Kotlin {
    async fn command_result(&mut self, request: i64) -> Result<(), CommandFailure> {
        self.next(|message| match message {
            Message::CommandResult {
                request: of,
                result,
            } if of == request => Some(result),
            _ => None,
        })
        .await
    }
}

#[tokio::test]
async fn a_command_is_answered_with_its_result() {
    let (host, mut kotlin) = start();
    connect_xm5(&host);
    host.event(r#"{"type":"holdAll"}"#).unwrap();
    let request = kotlin.open_asked(XM5).await;
    host.event(&format!(
        r#"{{"type":"opened","request":{request},"link":1}}"#
    ))
    .unwrap();
    kotlin.replay(&host, 1, &xm5_attach()).await;
    host.command(
        7,
        &format!(r#"{{"kind":"setNoiseControlMode","address":"{XM5}","mode":"noiseCancelling"}}"#),
    );
    assert!(matches!(kotlin.command_result(7).await, Ok(())));
}

#[tokio::test]
async fn a_command_the_core_refuses_or_cannot_read_says_why() {
    let (host, mut kotlin) = start();
    host.command(8, r#"{"kind":"powerOff","address":"00:00:00:00:00:00"}"#);
    assert!(matches!(
        kotlin.command_result(8).await,
        Err(CommandFailure::Rejected(CommandError::UnknownDevice))
    ));
    host.command(9, r#"{"kind":"powerOff"}"#);
    assert!(matches!(
        kotlin.command_result(9).await,
        Err(CommandFailure::BadRequest(_))
    ));
}

#[tokio::test]
async fn an_open_kotlin_never_got_fails_as_bluetooth_unavailable_instead_of_waiting_for_ever() {
    // Kotlin hasn't connected: nothing reaches it, but the test still sees it.
    let (sink, messages) = mpsc::unbounded_channel();
    let host = Host::start(move |message| {
        let _ = sink.send(message);
        false
    });
    let mut kotlin = Kotlin { messages };
    connect_xm5(&host);
    host.event(r#"{"type":"holdAll"}"#).unwrap();
    kotlin
        .device_list(|list| {
            state_of(list, XM5)
                == Some(DeviceState::NotAttached {
                    reason: NotAttachedReason::LinkFailed(LinkFailure::BluetoothUnavailable),
                })
        })
        .await;
}

#[tokio::test]
async fn kotlin_connecting_late_is_sent_the_device_list_it_missed() {
    let (host, mut kotlin) = start();
    host.resend_device_list();
    kotlin.device_list(|list| list.devices.is_empty()).await;
}

#[tokio::test]
async fn a_host_started_with_the_frame_log_on_logs_the_first_attach() {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("host_first_attach.frames");
    let _ = std::fs::remove_file(&path);
    let (sink, messages) = mpsc::unbounded_channel();
    let (host, started) =
        Host::start_with_frame_log(move |message| sink.send(message).is_ok(), &path);
    started.unwrap();
    let mut kotlin = Kotlin { messages };

    connect_xm5(&host);
    host.event(r#"{"type":"holdAll"}"#).unwrap();
    let request = kotlin.open_asked(XM5).await;
    host.event(&format!(
        r#"{{"type":"opened","request":{request},"link":1}}"#
    ))
    .unwrap();
    let records = xm5_attach();
    kotlin.replay(&host, 1, &records).await;
    kotlin
        .device_list(|list| state_of(list, XM5) == Some(DeviceState::Attached))
        .await;
    host.core()
        .command(Command::SetFrameLog(None))
        .await
        .unwrap();

    let log = FrameRecord::parse_all(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let bytes = |records: &[FrameRecord]| -> Vec<_> {
        records
            .iter()
            .map(|r| (r.direction, r.bytes.clone()))
            .collect()
    };
    assert_eq!(bytes(&log), bytes(&records));
}
