//! The default audio sink, from PulseAudio or PipeWire's PulseAudio server.
//!
//! libpulse is callback-based and its objects aren't `Send`, so a thread of
//! its own runs the main loop.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use headphone_bridge_core::platform::{DeviceAddress, Observation};
use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::subscribe::{Facility, InterestMaskSet};
use libpulse_binding::context::{Context, FlagSet, State};
use libpulse_binding::mainloop::standard::{IterateResult, Mainloop};
use tokio::sync::mpsc;

use crate::sink::{sink_device, Sink};

/// How long to wait before reconnecting to the sound server, e.g. when the
/// app starts at login before PipeWire does.
const RECONNECT_PAUSE: Duration = Duration::from_secs(5);

/// Reports the Device the default sink plays to now and whenever it changes,
/// for the life of the process.
pub fn watch(observations: mpsc::UnboundedSender<Observation>) {
    let spawned = std::thread::Builder::new()
        .name("hb-default-sink".into())
        .spawn(move || {
            let mut last = None;
            while !observations.is_closed() {
                if let Err(error) = run(&observations, &mut last) {
                    eprintln!("lost the sound server: {error}");
                }
                std::thread::sleep(RECONNECT_PAUSE);
            }
        });
    if let Err(error) = spawned {
        eprintln!("couldn't watch the default audio sink: {error}");
    }
}

type Shared<T> = Rc<RefCell<T>>;

/// Watches until the connection to the sound server ends.
fn run(
    observations: &mpsc::UnboundedSender<Observation>,
    last: &mut Option<Option<DeviceAddress>>,
) -> Result<(), String> {
    let mut mainloop = Mainloop::new().ok_or("no main loop")?;
    let mut context = Context::new(&mainloop, "Headphone Bridge").ok_or("no context")?;
    context
        .connect(None, FlagSet::NOFAIL, None)
        .map_err(describe)?;
    loop {
        iterate(&mut mainloop)?;
        match context.get_state() {
            State::Ready => break,
            State::Failed | State::Terminated => return Err("couldn't connect".into()),
            _ => {}
        }
    }

    // Changing the default sink changes the server; a sink's properties can
    // change while it's the default.
    let stale = Rc::new(RefCell::new(true));
    let marker = stale.clone();
    context.set_subscribe_callback(Some(Box::new(move |facility, _, _| {
        if matches!(facility, Some(Facility::Server | Facility::Sink)) {
            *marker.borrow_mut() = true;
        }
    })));
    context.subscribe(InterestMaskSet::SERVER | InterestMaskSet::SINK, |_| {});

    // Each check reads the default sink's name, then that sink.
    let default_sink: Shared<Option<Option<String>>> = Rc::default();
    let output: Shared<Option<Option<DeviceAddress>>> = Rc::default();
    let introspect = context.introspect();
    loop {
        if stale.replace(false) {
            let default_sink = default_sink.clone();
            introspect.get_server_info(move |info| {
                *default_sink.borrow_mut() =
                    Some(info.default_sink_name.as_ref().map(|name| name.to_string()));
            });
        }
        match default_sink.borrow_mut().take() {
            Some(Some(name)) => {
                let output = output.clone();
                let mut found = None;
                introspect.get_sink_info_by_name(&name, move |result| match result {
                    ListResult::Item(sink) => {
                        found = sink_device(&Sink {
                            name: sink.name.as_deref().unwrap_or_default(),
                            bluez5_address: sink.proplist.get_str("api.bluez5.address").as_deref(),
                            device_string: sink.proplist.get_str("device.string").as_deref(),
                        });
                    }
                    ListResult::End | ListResult::Error => {
                        *output.borrow_mut() = Some(found.take());
                    }
                });
            }
            Some(None) => *output.borrow_mut() = Some(None),
            None => {}
        }
        if let Some(device) = output.borrow_mut().take() {
            if last.as_ref() != Some(&device) {
                *last = Some(device.clone());
                observations
                    .send(Observation::AudioOutputChanged(device))
                    .map_err(|_| "the core stopped")?;
            }
        }
        iterate(&mut mainloop)?;
    }
}

fn iterate(mainloop: &mut Mainloop) -> Result<(), String> {
    match mainloop.iterate(true) {
        IterateResult::Success(_) => Ok(()),
        IterateResult::Quit(_) => Err("the main loop quit".into()),
        IterateResult::Err(error) => Err(describe(error)),
    }
}

fn describe(error: libpulse_binding::error::PAErr) -> String {
    error
        .to_string()
        .unwrap_or_else(|| format!("PulseAudio error {}", error.0))
}
