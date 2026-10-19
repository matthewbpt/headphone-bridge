//! Sharing the frame log: on Android it's in app-private storage, so the
//! window hands it to the share sheet instead of showing where it is.

use std::path::Path;

/// Why the frame log wasn't shared, as the UI names it.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShareError {
    /// The frame log is off, so there's no file.
    FrameLogOff,
    /// Nothing is logged yet: no Device has been Attached since it was turned on.
    FrameLogEmpty,
    /// The share sheet couldn't be opened.
    ShareFailed(String),
}

/// The frame log file, if there's anything in it to share.
pub fn frame_log_to_share(file: Option<&Path>) -> Result<&Path, ShareError> {
    use std::io::BufRead;
    let file = file.ok_or(ShareError::FrameLogOff)?;
    // Each time it's turned on the core writes a `#` header line; frames
    // come only once a Device is Attached.
    let has_frames = std::fs::File::open(file).is_ok_and(|opened| {
        std::io::BufReader::new(opened)
            .lines()
            .map_while(Result::ok)
            .any(|line| !line.trim().is_empty() && !line.starts_with('#'))
    });
    if has_frames {
        Ok(file)
    } else {
        Err(ShareError::FrameLogEmpty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new, empty directory for one test.
    fn dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "headphone-bridge-share-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn refuses_while_the_frame_log_is_off() {
        assert_eq!(frame_log_to_share(None), Err(ShareError::FrameLogOff));
    }

    #[test]
    fn refuses_a_frame_log_nothing_was_logged_to() {
        let dir = dir();
        let file = dir.join("frame-log.frames");
        assert_eq!(
            frame_log_to_share(Some(&file)),
            Err(ShareError::FrameLogEmpty)
        );

        std::fs::write(&file, "").unwrap();
        assert_eq!(
            frame_log_to_share(Some(&file)),
            Err(ShareError::FrameLogEmpty)
        );

        // Turned on twice, with no Device Attached in between.
        std::fs::write(
            &file,
            "# Headphone Bridge frame log\n# Headphone Bridge frame log\n",
        )
        .unwrap();
        assert_eq!(
            frame_log_to_share(Some(&file)),
            Err(ShareError::FrameLogEmpty)
        );
    }

    #[test]
    fn shares_a_frame_log_with_frames_in_it() {
        let file = dir().join("frame-log.frames");
        std::fs::write(&file, "# Headphone Bridge frame log\n> 01 02\n").unwrap();
        assert_eq!(frame_log_to_share(Some(&file)), Ok(file.as_path()));
    }

    #[test]
    fn names_each_reason_as_the_ui_does() {
        let json = |error| serde_json::to_string(&error).unwrap();
        assert_eq!(json(ShareError::FrameLogOff), r#""frameLogOff""#);
        assert_eq!(json(ShareError::FrameLogEmpty), r#""frameLogEmpty""#);
        assert_eq!(
            json(ShareError::ShareFailed("no app".into())),
            r#"{"shareFailed":"no app"}"#
        );
    }
}
