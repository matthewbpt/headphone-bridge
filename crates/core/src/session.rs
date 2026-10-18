//! Control link sessions: one runner per Vendor, from the link opening
//! until it closes or the Device turns out to be unsupported.

use std::time::Duration;

use tokio::sync::mpsc;

use crate::command::{Equalizer, NoiseControlMode};
use crate::device_list::NotAttachedReason;
use crate::frame_log::{Direction, FrameLog};
use crate::model::Model;
use crate::platform::ControlLink;
use crate::update::Update;
use crate::{samsung, sony};

/// A command for the Device, already checked against its Capabilities.
pub(crate) enum SessionCommand {
    SetNoiseControlMode(NoiseControlMode),
    SetAmbientLevel(u8),
    SetNoiseCancellingLevel(u8),
    SetEqualizer(Equalizer),
    PowerOff,
}

/// How long a Sony request waits for its ACK before it is sent again.
const ACK_TIMEOUT: Duration = Duration::from_secs(1);

/// Runs a Samsung control link session until the link closes, or until the
/// Device turns out not to be its `candidate` Model. Returns why it ended.
pub(crate) async fn run_samsung_session(
    mut link: ControlLink,
    candidate: Model,
    mut commands: mpsc::UnboundedReceiver<SessionCommand>,
    frame_log: FrameLog,
    mut on_update: impl FnMut(Update),
) -> NotAttachedReason {
    // Frames are logged under the vendor's slug until the Model is identified.
    let mut slug = "samsung";
    // Every frame written goes through here, so all of them are logged.
    let send = |slug: &str, frame: Vec<u8>| {
        frame_log.record(slug, Direction::Sent, &frame);
        link.outgoing.send(frame).is_ok()
    };
    if !send(slug, samsung::Session::handshake()) {
        return NotAttachedReason::LinkDropped;
    }
    let mut session = samsung::Session::new(candidate);
    loop {
        tokio::select! {
            bytes = link.incoming.recv() => {
                let Some(bytes) = bytes else {
                    return NotAttachedReason::LinkDropped;
                };
                for frame in session.deframe(&bytes) {
                    let updates = session.receive(&frame);
                    for update in &updates {
                        if let Update::Identified(model) = update {
                            slug = model.slug();
                        }
                    }
                    frame_log.record(slug, Direction::Received, &frame.raw);
                    for update in updates {
                        let unsupported = update == Update::Unsupported;
                        on_update(update);
                        if unsupported {
                            // Dropping the link Detaches without writing anything more.
                            return NotAttachedReason::UnsupportedModel;
                        }
                    }
                }
            }
            Some(command) = commands.recv() => {
                let frames = match command {
                    SessionCommand::SetNoiseControlMode(mode) => {
                        session.set_noise_control_mode(mode)
                    }
                    SessionCommand::SetAmbientLevel(level) => session.set_ambient_level(level),
                    SessionCommand::SetNoiseCancellingLevel(level) => {
                        session.set_noise_cancelling_level(level)
                    }
                    SessionCommand::SetEqualizer(equalizer) => {
                        session.set_equalizer_preset(equalizer.preset)
                    }
                    // No Samsung Model can Power Off, so the command is
                    // rejected before it gets here.
                    SessionCommand::PowerOff => {
                        debug_assert!(false, "no Samsung Model can Power Off");
                        Vec::new()
                    }
                };
                for frame in frames {
                    if !send(slug, frame) {
                        return NotAttachedReason::LinkDropped;
                    }
                }
            }
        }
    }
}

/// Runs a Sony control link session until the link closes, or until the
/// Device turns out to be an unsupported Model. Returns why it ended.
pub(crate) async fn run_sony_session(
    mut link: ControlLink,
    mut commands: mpsc::UnboundedReceiver<SessionCommand>,
    frame_log: FrameLog,
    mut on_update: impl FnMut(Update),
) -> NotAttachedReason {
    // Frames are logged under the vendor's slug until the Model is identified.
    let mut slug = "sony";
    let mut session = sony::Session::new();
    // Every frame the session sends goes out, and into the log, through here.
    let write = |session: &mut sony::Session, slug: &str| {
        session.outgoing().into_iter().all(|frame| {
            frame_log.record(slug, Direction::Sent, &frame);
            link.outgoing.send(frame).is_ok()
        })
    };
    if !write(&mut session, slug) {
        return NotAttachedReason::LinkDropped;
    }
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
                        }
                    }
                    frame_log.record(slug, Direction::Received, &frame.raw);
                    if updates.contains(&Update::Unsupported) {
                        on_update(Update::Unsupported);
                        // Dropping the link Detaches without writing anything more.
                        return NotAttachedReason::UnsupportedModel;
                    }
                    // Write first, so ACKs are out before the update is reported.
                    if !write(&mut session, slug) {
                        return NotAttachedReason::LinkDropped;
                    }
                    updates.into_iter().for_each(&mut on_update);
                }
            }
            Some(command) = commands.recv() => {
                match command {
                    SessionCommand::SetNoiseControlMode(mode) => {
                        session.set_noise_control_mode(mode)
                    }
                    SessionCommand::SetAmbientLevel(level) => session.set_ambient_level(level),
                    // No Sony Model has a Noise Cancelling Level, so the
                    // command is rejected before it gets here.
                    SessionCommand::SetNoiseCancellingLevel(_) => {
                        debug_assert!(false, "no Sony Model has a Noise Cancelling Level");
                    }
                    SessionCommand::SetEqualizer(equalizer) => session.set_equalizer(&equalizer),
                    SessionCommand::PowerOff => session.power_off(),
                }
                if !write(&mut session, slug) {
                    return NotAttachedReason::LinkDropped;
                }
            }
        }
    }
}
