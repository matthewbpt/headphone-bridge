use std::time::{Duration, UNIX_EPOCH};

use headphone_bridge_core::frame_log::{Direction, FrameRecord};

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
