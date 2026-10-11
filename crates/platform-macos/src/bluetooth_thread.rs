//! The thread that owns every IOBluetooth object and pumps its run loop.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{mpsc as std_mpsc, Arc};
use std::time::{Duration, Instant};

use headphone_bridge_core::platform::{DeviceAddress, DeviceIdentity, LinkFailure, Observation};
use objc2::rc::{autoreleasepool, Retained};
use objc2::{sel, Message};
use objc2_core_bluetooth::{CBManager, CBManagerAuthorization};
use objc2_core_foundation::{kCFRunLoopDefaultMode, CFRetained, CFRunLoop, CFRunLoopRunResult};
use objc2_io_bluetooth::{
    IOBluetoothDevice, IOBluetoothRFCOMMChannel, IOBluetoothSDPDataElement,
    IOBluetoothSDPServiceRecord, IOBluetoothSDPUUID, IOBluetoothUserNotification,
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::audio_output;
use crate::delegates::{DeviceEvent, DeviceEvents, DeviceWatcher, RfcommDelegate};
use crate::port_address;

pub type LinkId = u64;

/// An open link and the bytes it receives.
pub type OpenedLink = (LinkId, mpsc::UnboundedReceiver<Vec<u8>>);

pub enum Command {
    Open {
        address: DeviceAddress,
        service: Uuid,
        reply: oneshot::Sender<Result<OpenedLink, LinkFailure>>,
    },
    Write {
        link: LinkId,
        bytes: Vec<u8>,
    },
    Close {
        link: LinkId,
    },
    /// The default audio output may have changed: read it again.
    CheckAudioOutput,
}

/// Wakes the Bluetooth thread's run loop so it handles a command promptly.
#[derive(Clone)]
pub struct Waker(Arc<RunLoopHandle>);

struct RunLoopHandle(CFRetained<CFRunLoop>);

// SAFETY: CFRunLoopWakeUp may be called from any thread; it's the only thing
// done with the handle.
unsafe impl Send for RunLoopHandle {}
unsafe impl Sync for RunLoopHandle {}

impl Waker {
    pub fn wake(&self) {
        self.0 .0.wake_up();
    }
}

/// How long one run loop slice lasts when nothing wakes it.
const PUMP_SLICE: f64 = 0.05;
/// How long to wait for an SDP query when the service isn't cached.
const SDP_TIMEOUT: Duration = Duration::from_secs(3);
/// How long to wait for an RFCOMM channel to report itself open.
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
/// After the open completes with an error, how long to keep polling `isOpen`
/// before believing the error. IOBluetooth reports errors for opens that
/// succeed.
const OPEN_ERROR_GRACE: Duration = Duration::from_millis(1500);
/// ServiceClassIDList, the SDP attribute listing a record's service UUIDs.
const SERVICE_CLASS_ID_LIST: u16 = 0x0001;
/// SDP data element type descriptors.
const SDP_TYPE_UUID: u8 = 3;
const SDP_TYPE_SEQUENCE: u8 = 6;
const SDP_TYPE_ALTERNATIVE: u8 = 7;

pub fn spawn(
    observations: mpsc::UnboundedSender<Observation>,
) -> (std_mpsc::Sender<Command>, Waker) {
    let (commands_tx, commands) = std_mpsc::channel();
    let (waker_tx, waker_rx) = std_mpsc::channel();
    std::thread::Builder::new()
        .name("hb-iobluetooth".into())
        .spawn(move || {
            let run_loop = CFRunLoop::current().expect("thread has no run loop");
            let _ = waker_tx.send(Waker(Arc::new(RunLoopHandle(run_loop))));
            BluetoothThread::new(observations).run(commands);
        })
        .expect("couldn't spawn the Bluetooth thread");
    let waker = waker_rx.recv().expect("Bluetooth thread failed to start");
    (commands_tx, waker)
}

struct BluetoothThread {
    observations: mpsc::UnboundedSender<Observation>,
    events: DeviceEvents,
    watcher: Retained<DeviceWatcher>,
    connect_notification: Option<Retained<IOBluetoothUserNotification>>,
    /// Connected Devices, by IOBluetooth address. A Device is reported to the
    /// core once it's here.
    connected: HashMap<String, ConnectedDevice>,
    links: HashMap<LinkId, Link>,
    next_link: LinkId,
    /// The Device last reported as the audio output.
    audio_output: Option<DeviceAddress>,
}

struct ConnectedDevice {
    /// The object IOBluetooth reported. One from `deviceWithAddressString:`
    /// can claim the Device isn't connected while it is.
    device: Retained<IOBluetoothDevice>,
    disconnect_notification: Retained<IOBluetoothUserNotification>,
}

struct Link {
    channel: Retained<IOBluetoothRFCOMMChannel>,
    delegate: Retained<RfcommDelegate>,
}

impl BluetoothThread {
    fn new(observations: mpsc::UnboundedSender<Observation>) -> BluetoothThread {
        let events = DeviceEvents::default();
        let watcher = DeviceWatcher::new(events.clone());
        BluetoothThread {
            observations,
            events,
            watcher,
            connect_notification: None,
            connected: HashMap::new(),
            links: HashMap::new(),
            next_link: 0,
            audio_output: None,
        }
    }

    /// Every turn runs in its own autorelease pool. This thread has no pool
    /// of its own, so without one every object IOBluetooth autoreleases would
    /// live until the thread exits. That includes each RFCOMM channel it
    /// opens, which stays registered for the Device's connection events: one
    /// left over from before a disconnect reacts to the reconnect.
    fn run(mut self, commands: std_mpsc::Receiver<Command>) {
        autoreleasepool(|_| {
            // Fires for every new baseband connection, and may fire for those
            // that already exist; Devices already Connected are seeded below
            // either way.
            self.connect_notification = unsafe {
                IOBluetoothDevice::registerForConnectNotifications_selector(
                    Some(&self.watcher),
                    Some(sel!(deviceConnected:fromDevice:)),
                )
            };
            for device in paired_devices() {
                if unsafe { device.isConnected() } {
                    self.device_connected(&device);
                }
            }
            self.check_audio_output();
        });
        while autoreleasepool(|_| self.turn(&commands)) {}
        autoreleasepool(|_| self.shut_down());
    }

    /// Pumps the run loop once and handles what happened. False once the
    /// adapter is dropped.
    fn turn(&mut self, commands: &std_mpsc::Receiver<Command>) -> bool {
        pump();
        self.handle_device_events();
        self.sweep_closed_links();
        loop {
            match commands.try_recv() {
                Ok(command) => self.command(command),
                Err(std_mpsc::TryRecvError::Empty) => return true,
                Err(std_mpsc::TryRecvError::Disconnected) => return false,
            }
        }
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Open {
                address,
                service,
                reply,
            } => {
                let result = self.open(&address, service);
                if let Ok((link, _)) = &result {
                    if reply.is_closed() {
                        self.close(*link);
                    }
                }
                let _ = reply.send(result);
            }
            Command::Write { link, bytes } => self.write(link, bytes),
            Command::Close { link } => self.close(link),
            Command::CheckAudioOutput => self.check_audio_output(),
        }
    }

    fn handle_device_events(&mut self) {
        loop {
            let Some(event) = self.events.lock().pop_front() else {
                return;
            };
            match event {
                DeviceEvent::Connected(device) => self.device_connected(&device),
                DeviceEvent::Disconnected(device) => self.device_disconnected(&device),
                DeviceEvent::SdpQueryComplete(device) => {
                    // A Device whose services weren't cached is reported once
                    // they are.
                    if let Some(address) = address_of(&device) {
                        if !self.connected.contains_key(&address) && unsafe { device.isConnected() }
                        {
                            self.report_connected(&device, address);
                        }
                    }
                }
            }
        }
    }

    fn device_connected(&mut self, device: &IOBluetoothDevice) {
        // The address can be missing while the record is still populating; a
        // later notification follows.
        let Some(address) = address_of(device) else {
            return;
        };
        if self.connected.contains_key(&address) {
            return;
        }
        if has_cached_services(device) {
            self.report_connected(device, address);
        } else {
            // Doesn't create a baseband connection: the Device is Connected.
            unsafe { device.performSDPQuery(Some(&self.watcher)) };
        }
    }

    fn report_connected(&mut self, device: &IOBluetoothDevice, address: String) {
        let Some(notification) = (unsafe {
            device.registerForDisconnectNotification_selector(
                Some(&self.watcher),
                Some(sel!(deviceDisconnected:fromDevice:)),
            )
        }) else {
            return;
        };
        self.connected.insert(
            address.clone(),
            ConnectedDevice {
                device: device.retain(),
                disconnect_notification: notification,
            },
        );
        let _ = self
            .observations
            .send(Observation::DeviceConnected(DeviceIdentity {
                address: port_address(&address),
                name: unsafe { device.name() }.to_string(),
                service_uuids: service_uuids(device),
            }));
        // The audio output may have been matched by this Device's name.
        self.check_audio_output();
    }

    fn device_disconnected(&mut self, device: &IOBluetoothDevice) {
        let Some(address) = address_of(device) else {
            return;
        };
        if let Some(connected) = self.connected.remove(&address) {
            // Notifications retain their observer until unregistered.
            unsafe { connected.disconnect_notification.unregister() };
            let _ = self
                .observations
                .send(Observation::DeviceDisconnected(port_address(&address)));
            self.check_audio_output();
        }
    }

    /// Reports the default audio output if the Device it matches has changed.
    fn check_audio_output(&mut self) {
        let output = audio_output::default_output().and_then(|output| {
            // A UID without an address is matched by name instead.
            output.address.or_else(|| {
                let name = output.name?;
                self.connected
                    .iter()
                    .find(|(_, connected)| unsafe { connected.device.name() }.to_string() == name)
                    .map(|(address, _)| port_address(address))
            })
        });
        if output != self.audio_output {
            self.audio_output = output.clone();
            let _ = self
                .observations
                .send(Observation::AudioOutputChanged(output));
        }
    }

    fn open(&mut self, address: &DeviceAddress, service: Uuid) -> Result<OpenedLink, LinkFailure> {
        if matches!(
            unsafe { CBManager::authorization_class() },
            CBManagerAuthorization::Denied | CBManagerAuthorization::Restricted
        ) {
            return Err(LinkFailure::BluetoothPermissionDenied);
        }
        // Only Devices the OS reported Connected are opened, so this never
        // connects one. `isConnected` isn't checked: on a Connected Buds4 Pro
        // it returned false while the OS listed it as connected.
        let iobluetooth_address = address.0.replace(':', "-").to_ascii_lowercase();
        let device = self
            .connected
            .get(&iobluetooth_address)
            .map(|connected| connected.device.clone())
            .ok_or(LinkFailure::DeviceNotConnected)?;
        let channel_id = self.rfcomm_channel_id(&device, service)?;

        let (incoming, incoming_rx) = mpsc::unbounded_channel();
        let delegate = RfcommDelegate::new(incoming);
        let mut channel: Option<Retained<IOBluetoothRFCOMMChannel>> = None;
        let status = unsafe {
            device.openRFCOMMChannelAsync_withChannelID_delegate(
                Some(&mut channel),
                channel_id,
                Some(&delegate),
            )
        };
        let Some(channel) = channel else {
            return Err(self.open_failure(status));
        };

        // The status, and the one passed to the delegate, can report an error
        // for a channel that opens fine: only `isOpen` is trusted.
        let started = Instant::now();
        let mut error_since: Option<Instant> = None;
        while !unsafe { channel.isOpen() } {
            let last_error = delegate.open_status().filter(|&s| s != 0);
            if last_error.is_some() && error_since.is_none() {
                error_since = Some(Instant::now());
            }
            let gave_up = error_since.is_some_and(|since| since.elapsed() > OPEN_ERROR_GRACE)
                || started.elapsed() > OPEN_TIMEOUT;
            if gave_up {
                release(&channel);
                delegate.close();
                return Err(match last_error {
                    Some(status) => self.open_failure(status),
                    None => LinkFailure::Other("timed out opening the control link".into()),
                });
            }
            pump();
        }

        let link = self.next_link;
        self.next_link += 1;
        self.links.insert(link, Link { channel, delegate });
        Ok((link, incoming_rx))
    }

    /// Finds the RFCOMM channel of `service`. Querying SDP for specific UUIDs
    /// silently never completes since macOS Ventura, so this uses the cached
    /// records, and if needed refreshes them with an unfiltered query.
    fn rfcomm_channel_id(
        &mut self,
        device: &IOBluetoothDevice,
        service: Uuid,
    ) -> Result<u8, LinkFailure> {
        let mut record = service_record(device, service);
        if record.is_none() {
            unsafe { device.performSDPQuery(Some(&self.watcher)) };
            let started = Instant::now();
            while started.elapsed() < SDP_TIMEOUT {
                pump();
                if self.sdp_query_completed(device) {
                    break;
                }
            }
            record = service_record(device, service);
        }
        let record = record.ok_or(LinkFailure::ServiceNotFound)?;
        let mut channel_id = 0;
        if unsafe { record.getRFCOMMChannelID(&mut channel_id) } != 0 {
            return Err(LinkFailure::ServiceNotFound);
        }
        Ok(channel_id)
    }

    /// Whether an SDP query for `device` has completed, leaving other queued
    /// events in place.
    fn sdp_query_completed(&self, device: &IOBluetoothDevice) -> bool {
        let address = address_of(device);
        let mut events = self.events.lock();
        let position = events.iter().position(
            |event| matches!(event, DeviceEvent::SdpQueryComplete(d) if address_of(d) == address),
        );
        position.and_then(|at| events.remove(at)).is_some()
    }

    /// Maps an IOReturn from opening a channel. Only the authorization check
    /// tells permission denied apart from IOBluetooth's general error.
    fn open_failure(&self, status: i32) -> LinkFailure {
        if unsafe { CBManager::authorization_class() } != CBManagerAuthorization::AllowedAlways {
            return LinkFailure::BluetoothPermissionDenied;
        }
        match status as u32 {
            // kIOReturnExclusiveAccess, kIOReturnBusy, kIOReturnNoResources:
            // unverified on hardware.
            0xE000_02C5 | 0xE000_02D5 | 0xE000_02BE => LinkFailure::InUseByAnotherProcess,
            0xE000_02E2 => LinkFailure::BluetoothPermissionDenied,
            0xE000_02C0 | 0xE000_02D9 => LinkFailure::DeviceNotConnected,
            _ => LinkFailure::Other(format!("IOBluetooth error 0x{:08X}", status as u32)),
        }
    }

    fn write(&mut self, link: LinkId, mut bytes: Vec<u8>) {
        let Some(Link { channel, .. }) = self.links.get(&link) else {
            return;
        };
        let mtu = usize::from(unsafe { channel.getMTU() }).max(1);
        for chunk in bytes.chunks_mut(mtu) {
            let status = unsafe {
                channel.writeSync_length(chunk.as_mut_ptr() as *mut c_void, chunk.len() as u16)
            };
            if status != 0 {
                // A failed write means the link is gone; the core sees it close.
                self.close(link);
                return;
            }
        }
    }

    fn close(&mut self, link: LinkId) {
        if let Some(Link { channel, delegate }) = self.links.remove(&link) {
            release(&channel);
            delegate.close();
        }
    }

    fn sweep_closed_links(&mut self) {
        let closed: Vec<LinkId> = self
            .links
            .iter()
            .filter(|(_, l)| l.delegate.is_closed() || !unsafe { l.channel.isOpen() })
            .map(|(&id, _)| id)
            .collect();
        for link in closed {
            self.close(link);
        }
    }

    fn shut_down(&mut self) {
        for link in self.links.keys().copied().collect::<Vec<_>>() {
            self.close(link);
        }
        for (_, connected) in self.connected.drain() {
            unsafe { connected.disconnect_notification.unregister() };
        }
        if let Some(notification) = self.connect_notification.take() {
            unsafe { notification.unregister() };
        }
    }
}

/// Stops `channel` calling its delegate, and closes it if it's open.
/// `closeChannel` closes the Device's RFCOMM channel with the same id whatever
/// the state of this object, so on one that's closed, or never finished
/// opening, it could only close another.
fn release(channel: &IOBluetoothRFCOMMChannel) {
    unsafe {
        channel.setDelegate(None);
        if channel.isOpen() {
            channel.closeChannel();
        }
    }
}

/// Runs the thread's run loop for one slice, so IOBluetooth can deliver
/// callbacks. With no sources yet, the run loop returns at once, so sleep.
fn pump() {
    let result = CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, PUMP_SLICE, false);
    if result == CFRunLoopRunResult::Finished {
        std::thread::sleep(Duration::from_secs_f64(PUMP_SLICE));
    }
}

fn paired_devices() -> Vec<Retained<IOBluetoothDevice>> {
    let Some(devices) = (unsafe { IOBluetoothDevice::pairedDevices() }) else {
        return Vec::new();
    };
    devices
        .to_vec()
        .into_iter()
        .filter_map(|device| device.downcast::<IOBluetoothDevice>().ok())
        .collect()
}

fn address_of(device: &IOBluetoothDevice) -> Option<String> {
    unsafe { device.addressString() }
        .map(|address| address.to_string())
        .filter(|address| !address.is_empty())
}

fn has_cached_services(device: &IOBluetoothDevice) -> bool {
    unsafe { device.services() }.is_some_and(|services| services.count() > 0)
}

fn service_record(
    device: &IOBluetoothDevice,
    service: Uuid,
) -> Option<Retained<IOBluetoothSDPServiceRecord>> {
    let bytes = service.as_bytes();
    let uuid =
        unsafe { IOBluetoothSDPUUID::uuidWithBytes_length(bytes.as_ptr() as *const c_void, 16) }?;
    unsafe { device.getServiceRecordForUUID(Some(&uuid)) }
}

/// The 128-bit service class UUIDs of every cached SDP record.
fn service_uuids(device: &IOBluetoothDevice) -> Vec<Uuid> {
    let Some(records) = (unsafe { device.services() }) else {
        return Vec::new();
    };
    let mut uuids = Vec::new();
    for record in records.to_vec() {
        let Ok(record) = record.downcast::<IOBluetoothSDPServiceRecord>() else {
            continue;
        };
        if let Some(list) = unsafe { record.getAttributeDataElement(SERVICE_CLASS_ID_LIST) } {
            collect_uuids(&list, &mut uuids);
        }
    }
    uuids
}

/// ServiceClassIDList is a sequence of UUIDs in the SDP spec, but IOBluetooth
/// can hand back a lone UUID element instead (seen with a Buds4 Pro).
fn collect_uuids(element: &IOBluetoothSDPDataElement, uuids: &mut Vec<Uuid>) {
    match unsafe { element.getTypeDescriptor() } {
        SDP_TYPE_UUID => {
            let uuid = unsafe { element.getUUIDValue() }
                .and_then(|uuid| unsafe { uuid.getUUIDWithLength(16) });
            if let Some(uuid) = uuid.and_then(|uuid| Uuid::from_slice(&uuid.to_vec()).ok()) {
                uuids.push(uuid);
            }
        }
        SDP_TYPE_SEQUENCE | SDP_TYPE_ALTERNATIVE => {
            for child in unsafe { element.getArrayValue() }
                .map(|a| a.to_vec())
                .unwrap_or_default()
            {
                if let Ok(child) = child.downcast::<IOBluetoothSDPDataElement>() {
                    collect_uuids(&child, uuids);
                }
            }
        }
        _ => {}
    }
}
