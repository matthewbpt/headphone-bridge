//! Control Link sessions: one runner for every Vendor, from the Control Link
//! opening until it closes or the Device turns out to be unsupported.

use std::time::Duration;

use tokio::sync::mpsc;

use crate::command::{Equalizer, NoiseControlMode};
use crate::device_list::NotAttachedReason;
use crate::frame_log::{Direction, FrameLog};
use crate::platform::ControlLink;
use crate::update::Update;

/// A command for the Device, already checked against its Capabilities.
pub(crate) enum SessionCommand {
    SetNoiseControlMode(NoiseControlMode),
    SetAmbientLevel(u8),
    SetNoiseCancellingLevel(u8),
    SetEqualizer(Equalizer),
    PowerOff,
}

/// A Vendor's protocol session with one Device: pure, no I/O and no clock.
/// [`run_session`] drives it over the Control Link.
pub(crate) trait VendorSession {
    /// A whole frame received from the Device.
    type Frame;

    /// The Vendor's slug, which frames are logged under until the Model is
    /// identified.
    const VENDOR: &'static str;

    /// The whole frame as it appeared on the wire, for the frame log.
    fn raw(frame: &Self::Frame) -> &[u8];

    /// Feeds bytes received from the Device, in chunks of any size, and
    /// returns the whole frames they complete.
    fn deframe(&mut self, bytes: &[u8]) -> Vec<Self::Frame>;

    /// Handles a frame received from the Device. Any frames to send in
    /// response, such as its ACK, are then in [`VendorSession::outgoing`].
    fn receive(&mut self, frame: &Self::Frame) -> Vec<Update>;

    /// Asks the Device to carry out `command`, queueing its frames.
    ///
    /// A command the Vendor can't carry, such as Power Off on a Model
    /// without it, is ignored: the Capabilities checks reject it before it
    /// gets here, and its acceptance has already been replied to, so an
    /// error would have nowhere to go.
    fn apply(&mut self, command: SessionCommand);

    /// Takes the frames to write to the Device, in order.
    fn outgoing(&mut self) -> Vec<Vec<u8>>;

    /// While a request is waiting for its ACK, a number that changes each
    /// time it is sent again or another request goes out. `None` while
    /// nothing is waiting, as always for a Vendor without ACKs.
    fn awaiting_ack(&self) -> Option<u64> {
        None
    }

    /// Queues the request that is waiting for its ACK again. Returns
    /// `false`, queueing nothing, once the Device is given up on.
    fn resend(&mut self) -> bool {
        true
    }
}

/// How long a Device has to identify itself once its Control Link is open.
const IDENTIFY_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a request waits for its ACK before it is sent again.
const ACK_TIMEOUT: Duration = Duration::from_secs(1);

/// Runs `session` over `link` until the link closes, the Device turns out to
/// be an unsupported Model, or it stops responding. Returns why it ended.
pub(crate) async fn run_session<S: VendorSession>(
    mut session: S,
    mut link: ControlLink,
    mut commands: mpsc::UnboundedReceiver<SessionCommand>,
    frame_log: FrameLog,
    mut on_update: impl FnMut(Update),
) -> NotAttachedReason {
    // Frames are logged under the Vendor's slug until the Model is identified.
    let mut slug = S::VENDOR;
    // Every frame the session sends goes out, and into the log, through here.
    let write = |session: &mut S, slug: &str| {
        session.outgoing().into_iter().all(|frame| {
            frame_log.record(slug, Direction::Sent, &frame);
            link.outgoing.send(frame).is_ok()
        })
    };
    if !write(&mut session, slug) {
        return NotAttachedReason::LinkDropped;
    }
    // A Device that never identifies itself, e.g. one that ACKs but never
    // replies, would otherwise be left Attaching for good.
    let identify_timeout = tokio::time::sleep(IDENTIFY_TIMEOUT);
    tokio::pin!(identify_timeout);
    let mut identified = false;
    // A request whose ACK is lost is sent again every second, and a Device
    // that acknowledges none of the tries is given up on, like a dropped link
    // but retried as one that didn't respond.
    let ack_timeout = tokio::time::sleep(ACK_TIMEOUT);
    tokio::pin!(ack_timeout);
    let mut awaiting_ack = None;
    loop {
        if session.awaiting_ack() != awaiting_ack {
            awaiting_ack = session.awaiting_ack();
            ack_timeout
                .as_mut()
                .reset(tokio::time::Instant::now() + ACK_TIMEOUT);
        }
        tokio::select! {
            () = &mut identify_timeout, if !identified => {
                return NotAttachedReason::NoResponse;
            }
            () = &mut ack_timeout, if awaiting_ack.is_some() => {
                if !session.resend() {
                    return NotAttachedReason::NoResponse;
                }
                if !write(&mut session, slug) {
                    return NotAttachedReason::LinkDropped;
                }
            }
            bytes = link.incoming.recv() => {
                let Some(bytes) = bytes else {
                    return NotAttachedReason::LinkDropped;
                };
                for frame in session.deframe(&bytes) {
                    let updates = session.receive(&frame);
                    for update in &updates {
                        if let Update::Identified(model) = update {
                            slug = model.slug();
                            identified = true;
                        }
                    }
                    frame_log.record(slug, Direction::Received, S::raw(&frame));
                    if updates.contains(&Update::Unsupported) {
                        on_update(Update::Unsupported);
                        // Dropping the link Detaches without writing anything more.
                        return NotAttachedReason::UnsupportedModel;
                    }
                    // Write first, so ACKs are out before the updates are reported.
                    if !write(&mut session, slug) {
                        return NotAttachedReason::LinkDropped;
                    }
                    updates.into_iter().for_each(&mut on_update);
                }
            }
            Some(command) = commands.recv() => {
                session.apply(command);
                if !write(&mut session, slug) {
                    return NotAttachedReason::LinkDropped;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use tokio::task::JoinHandle;

    use super::*;
    use crate::frame_log::FrameRecord;
    use crate::model::Model;
    use crate::sony::frame::DataType;
    use crate::{samsung, sony};

    use headphone_bridge_fixtures::{BUDS2_ATTACH, XM5_ATTACH};

    /// The WH-1000XM5 Attach up to the model name reply and what the app
    /// writes in answer to it.
    const XM5_IDENTIFYING: usize = 9;

    /// Something the Device sees written, or the app is told, in the order
    /// it happens.
    #[derive(Debug, PartialEq)]
    enum Event {
        Written(Vec<u8>),
        Reported(Update),
    }

    #[derive(Debug)]
    struct Timeline {
        written: mpsc::UnboundedReceiver<Vec<u8>>,
        events: Vec<Event>,
    }

    impl Timeline {
        /// Moves every frame written so far onto the timeline.
        fn catch_up(&mut self) {
            while let Ok(frame) = self.written.try_recv() {
                self.events.push(Event::Written(frame));
            }
        }
    }

    /// The Device's end of an in-memory Control Link, with a session
    /// running on the app's end.
    struct Device {
        to_app: mpsc::UnboundedSender<Vec<u8>>,
        commands: mpsc::UnboundedSender<SessionCommand>,
        timeline: Arc<Mutex<Timeline>>,
        session: JoinHandle<NotAttachedReason>,
    }

    impl Device {
        fn start<S>(session: S, frame_log: FrameLog) -> Device
        where
            S: VendorSession + Send + 'static,
        {
            let (to_app, incoming) = mpsc::unbounded_channel();
            let (outgoing, written) = mpsc::unbounded_channel();
            let (commands, commands_rx) = mpsc::unbounded_channel();
            let timeline = Arc::new(Mutex::new(Timeline {
                written,
                events: Vec::new(),
            }));
            let reported = Arc::clone(&timeline);
            let on_update = move |update| {
                let mut timeline = reported.lock().unwrap();
                timeline.catch_up();
                timeline.events.push(Event::Reported(update));
            };
            let link = ControlLink { incoming, outgoing };
            let session = tokio::spawn(run_session(
                session,
                link,
                commands_rx,
                frame_log,
                on_update,
            ));
            Device {
                to_app,
                commands,
                timeline,
                session,
            }
        }

        /// Sends `frames` to the app and lets the session handle them.
        async fn send(&self, frames: Vec<Vec<u8>>) {
            for frame in frames {
                self.to_app.send(frame).unwrap();
            }
            settle().await;
        }

        async fn command(&self, command: SessionCommand) {
            self.commands.send(command).unwrap();
            settle().await;
        }

        /// Takes what has happened since last asked.
        fn events(&self) -> Vec<Event> {
            let mut timeline = self.timeline.lock().unwrap();
            timeline.catch_up();
            std::mem::take(&mut timeline.events)
        }

        /// Closes the link and returns why the session ended.
        async fn close(self) -> NotAttachedReason {
            drop(self.to_app);
            self.session.await.unwrap()
        }
    }

    /// Lets the session catch up, well within any timeout.
    async fn settle() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    fn fixture(text: &str) -> Vec<FrameRecord> {
        FrameRecord::parse_all(text).unwrap()
    }

    fn frames(records: &[FrameRecord], direction: Direction) -> Vec<Vec<u8>> {
        records
            .iter()
            .filter(|record| record.direction == direction)
            .map(|record| record.bytes.clone())
            .collect()
    }

    fn written(records: &[FrameRecord]) -> Vec<Event> {
        frames(records, Direction::Sent)
            .into_iter()
            .map(Event::Written)
            .collect()
    }

    /// A fresh path for a test's frame log.
    fn log_path(test: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "headphone-bridge-session-{}-{test}.frames",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// Runs `session` through the Device's side of `records`, then returns
    /// the frame log as slug, direction and bytes.
    async fn logged(
        session: impl VendorSession + Send + 'static,
        records: &[FrameRecord],
        test: &str,
    ) -> Vec<(String, Direction, Vec<u8>)> {
        let path = log_path(test);
        let frame_log = FrameLog::default();
        frame_log.start(&path).unwrap();
        let device = Device::start(session, frame_log.clone());
        device.send(frames(records, Direction::Received)).await;
        device.close().await;
        frame_log.stop();
        let log = fixture(&std::fs::read_to_string(&path).unwrap());
        let _ = std::fs::remove_file(&path);
        log.into_iter()
            .map(|record| (record.model, record.direction, record.bytes))
            .collect()
    }

    fn as_logged(records: &[FrameRecord]) -> Vec<(String, Direction, Vec<u8>)> {
        records
            .iter()
            .map(|record| (record.model.clone(), record.direction, record.bytes.clone()))
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn a_received_frames_acks_are_written_before_its_updates_are_reported() {
        let identifying = &fixture(XM5_ATTACH)[..XM5_IDENTIFYING];
        let device = Device::start(sony::Session::new(), FrameLog::default());

        device.send(frames(identifying, Direction::Received)).await;

        let mut expected = written(identifying);
        expected.push(Event::Reported(Update::Identified(Model::Wh1000Xm5)));
        assert_eq!(device.events(), expected);
    }

    #[tokio::test(start_paused = true)]
    async fn a_sony_frame_revealing_an_unsupported_model_reports_it_alone_and_writes_nothing_more()
    {
        let records = fixture(XM5_ATTACH);
        // Up to the model name reply, which names a WH-1000XM4 instead.
        let asked_for_name = &records[..6];
        let mut name = vec![0x05, 0x01, 0x0a];
        name.extend_from_slice(b"WH-1000XM4");
        let xm4 = sony::frame::encode(DataType::Mdr, 1, &name);
        let device = Device::start(sony::Session::new(), FrameLog::default());

        let mut from_device = frames(asked_for_name, Direction::Received);
        from_device.push(xm4);
        device.send(from_device).await;

        let mut expected = written(asked_for_name);
        expected.push(Event::Reported(Update::Unsupported));
        assert_eq!(
            device.session.is_finished().then(|| device.events()),
            Some(expected),
            "the session ends with nothing written after the name reply"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_samsung_frame_revealing_an_unsupported_model_ends_the_session_with_nothing_more_written(
    ) {
        // A Device named as a Buds4 Pro that reports a Buds2's device id.
        // Samsung's session already reports such a frame as Unsupported
        // alone, so only the Sony test pins that the runner reports it
        // alone; this one pins that the session ends on it.
        let records = fixture(BUDS2_ATTACH);
        let device = Device::start(
            samsung::Session::new(Model::GalaxyBuds4Pro),
            FrameLog::default(),
        );

        device.send(frames(&records, Direction::Received)).await;

        assert_eq!(
            device.session.is_finished().then(|| device.events()),
            Some(
                written(&records)
                    .into_iter()
                    .chain([Event::Reported(Update::Unsupported)])
                    .collect()
            )
        );
    }

    #[tokio::test(start_paused = true)]
    async fn sony_frames_are_logged_under_the_vendor_until_the_identifying_frame() {
        let identifying = &fixture(XM5_ATTACH)[..XM5_IDENTIFYING];

        let log = logged(sony::Session::new(), identifying, "sony-slugs").await;

        assert_eq!(log, as_logged(identifying));
    }

    #[tokio::test(start_paused = true)]
    async fn samsung_frames_are_logged_under_the_vendor_until_the_identifying_frame() {
        let records = fixture(BUDS2_ATTACH);

        let log = logged(
            samsung::Session::new(Model::GalaxyBuds2),
            &records,
            "samsung-slugs",
        )
        .await;

        assert_eq!(log, as_logged(&records));
    }

    #[tokio::test(start_paused = true)]
    async fn a_power_off_a_samsung_device_cant_carry_writes_nothing_and_keeps_the_session() {
        let device = Device::start(
            samsung::Session::new(Model::GalaxyBuds2),
            FrameLog::default(),
        );
        device
            .send(frames(&fixture(BUDS2_ATTACH), Direction::Received))
            .await;
        device.events();

        device.command(SessionCommand::PowerOff).await;

        assert_eq!(device.events(), Vec::new());
        assert!(!device.session.is_finished());
    }

    #[tokio::test(start_paused = true)]
    async fn a_noise_cancelling_level_a_sony_device_cant_carry_writes_nothing_and_keeps_the_session(
    ) {
        let records = fixture(XM5_ATTACH);
        // Identified, and with the support function request acknowledged,
        // so nothing waits to go out.
        let acknowledged = &records[..XM5_IDENTIFYING + 1];
        let device = Device::start(sony::Session::new(), FrameLog::default());
        device.send(frames(acknowledged, Direction::Received)).await;
        device.events();

        device
            .command(SessionCommand::SetNoiseCancellingLevel(2))
            .await;

        assert_eq!(device.events(), Vec::new());
        assert!(!device.session.is_finished());
    }
}
