mod support;

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use headphone_bridge_core::frame_log::{Direction, FrameRecord};
use headphone_bridge_core::platform::Observation;
use headphone_bridge_core::{Command, Core, DeviceState};
use support::{buds4pro, fake_platform, fixture, received, wait_for};

const LINE: &str = "2026-09-13T13:26:47.128Z galaxy-buds4-pro >> fd 06 00 88 01 01 22 da 58 dd";

#[test]
fn a_frame_record_reads_timestamp_model_direction_and_raw_bytes() {
    let record: FrameRecord = LINE.parse().unwrap();

    assert_eq!(
        record,
        FrameRecord {
            timestamp: UNIX_EPOCH + Duration::from_millis(1_789_306_007_128),
            model: "galaxy-buds4-pro".into(),
            direction: Direction::Sent,
            bytes: vec![0xfd, 0x06, 0x00, 0x88, 0x01, 0x01, 0x22, 0xda, 0x58, 0xdd],
        }
    );
}

#[test]
fn a_frame_record_is_written_in_the_format_it_is_read_from() {
    for line in [
        LINE,
        "2000-02-29T23:59:59.999Z samsung << fd 04 00 78 00 f0 81 dd",
        "1970-01-01T00:00:00.000Z wh-1000xm5 << 3e 0c 00 3c",
    ] {
        assert_eq!(line.parse::<FrameRecord>().unwrap().to_string(), line);
    }
    let leap_day: FrameRecord = "2000-02-29T23:59:59.999Z samsung << dd".parse().unwrap();
    assert_eq!(
        leap_day.timestamp,
        UNIX_EPOCH + Duration::from_millis(951_868_799_999)
    );
}

#[test]
fn malformed_records_are_rejected() {
    for line in [
        "2026-09-13 13:26:47.128 galaxy-buds4-pro >> fd",
        "2026-09-13T13:26:47.128Z galaxy-buds4-pro -> fd",
        "2026-09-13T13:26:47.128Z galaxy-buds4-pro >> FDX",
        "2026-09-13T13:26:47.128Z galaxy-buds4-pro >> fd  06",
        "2026-09-13T13:26:47.128Z galaxy-buds4-pro >>",
        "2026-13-13T13:26:47.128Z galaxy-buds4-pro >> fd",
    ] {
        assert!(line.parse::<FrameRecord>().is_err(), "{line}");
    }
}

#[test]
fn a_file_of_records_skips_comments_and_blank_lines_and_names_bad_lines() {
    let text = format!("# a comment\n\n{LINE}\n{LINE}\n");
    assert_eq!(FrameRecord::parse_all(&text).unwrap().len(), 2);

    let error = FrameRecord::parse_all(&format!("{LINE}\nnonsense\n")).unwrap_err();
    assert!(error.to_string().starts_with("line 2:"), "{error}");
}

const MANAGER_INFO: [u8; 10] = [0xfd, 0x06, 0x00, 0x88, 0x01, 0x01, 0x22, 0xda, 0x58, 0xdd];

/// A fresh path for a test's frame log.
fn log_path(test: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{test}.frames"));
    let _ = std::fs::remove_file(&path);
    path
}

fn read_log(path: &PathBuf) -> Vec<FrameRecord> {
    let text = std::fs::read_to_string(path).unwrap();
    FrameRecord::parse_all(&text).unwrap()
}

#[tokio::test]
async fn every_frame_sent_and_received_is_logged_with_its_direction_and_model() {
    let path = log_path("every_frame_sent_and_received_is_logged");
    let (platform, mut os) = fake_platform();
    let core = Core::start(platform);
    core.command(Command::SetFrameLog(Some(path.clone())))
        .await
        .unwrap();
    let started = SystemTime::now() - Duration::from_millis(1);

    os.observe(Observation::DeviceConnected(buds4pro()));
    let link = os.next_link().await;
    let frames = received(&fixture("galaxy-buds4-pro/capture-3"));
    for frame in &frames {
        link.send.send(frame.clone()).unwrap();
    }
    wait_for(&core, |list| list.devices[0].battery.is_some()).await;
    core.command(Command::SetFrameLog(None)).await.unwrap();
    let ended = SystemTime::now();

    let log = read_log(&path);
    let summary: Vec<(&str, Direction, &[u8])> = log
        .iter()
        .map(|r| (r.model.as_str(), r.direction, r.bytes.as_slice()))
        .collect();
    let mut expected = vec![("samsung", Direction::Sent, &MANAGER_INFO[..])];
    expected.extend(
        frames
            .iter()
            .map(|frame| ("galaxy-buds4-pro", Direction::Received, frame.as_slice())),
    );
    assert_eq!(summary, expected);
    assert!(log.windows(2).all(|w| w[0].timestamp <= w[1].timestamp));
    assert!(log[0].timestamp >= started && log[log.len() - 1].timestamp <= ended);
    assert!(
        core.device_list().devices[0].state == DeviceState::Attached,
        "logging doesn't change the session"
    );
}
