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
