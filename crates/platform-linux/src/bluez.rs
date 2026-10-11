//! BlueZ, through `bluer`: watching Devices connect and disconnect, and
//! opening control links with a client Profile and `ConnectProfile`.
//!
//! bluetoothd opens the RFCOMM channel itself and hands us its socket over
//! D-Bus, so this needs no root or group membership, and the channel is one
//! more on the Device's existing connection, alongside A2DP.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use bluer::rfcomm::{ConnectRequest, Profile, ReqError, Role, Stream};
use bluer::{Adapter, AdapterEvent, Address, Device, DeviceEvent, DeviceProperty, Session};
use futures::stream::{SelectAll, StreamExt};
use headphone_bridge_core::platform::{
    ControlLink, DeviceAddress, DeviceIdentity, LinkFailure, Observation,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Mutex, OnceCell};
use uuid::Uuid;

use crate::failure::{connect_profile_failure, ConnectProfileFailure};

/// How long BlueZ gets to answer `ConnectProfile`. bluer's own D-Bus timeout
/// is two minutes.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// After `ConnectProfile` succeeds, how long to wait for the channel to reach
/// our Profile before deciding another process's Profile got it.
const NEW_CONNECTION_GRACE: Duration = Duration::from_secs(2);
/// How often, and how far apart, `ConnectProfile` is asked again while BlueZ
/// is busy with the Device, e.g. still connecting its audio just after it
/// connected.
const BUSY_RETRIES: u32 = 10;
const BUSY_PAUSE: Duration = Duration::from_millis(1500);

/// The BlueZ session, shared by the Device watcher and every link opened.
pub struct Bluez {
    session: Session,
    adapter: Adapter,
    /// One registered client Profile per vendor service, registered the first
    /// time a link to that service is opened.
    profiles: Mutex<HashMap<Uuid, Waiters>>,
}

/// Who is waiting for BlueZ to hand over a channel, by Device.
type Waiters = Arc<std::sync::Mutex<HashMap<Address, oneshot::Sender<ConnectRequest>>>>;

/// Connects to BlueZ once, for whoever asks first.
#[derive(Clone, Default)]
pub struct Connection(Arc<OnceCell<Result<Bluez, String>>>);

impl Connection {
    pub async fn get(&self) -> Result<&Bluez, LinkFailure> {
        self.0
            .get_or_init(|| async {
                let session = Session::new().await.map_err(|e| e.to_string())?;
                let adapter = session.default_adapter().await.map_err(|e| e.to_string())?;
                Ok(Bluez {
                    session,
                    adapter,
                    profiles: Mutex::default(),
                })
            })
            .await
            .as_ref()
            .map_err(|e| LinkFailure::Other(format!("Bluetooth is unavailable: {e}")))
    }
}

/// Reports Devices as they become Connected and disconnected, until BlueZ
/// goes away. Never scans, connects or disconnects anything.
pub async fn watch(connection: Connection, observations: mpsc::UnboundedSender<Observation>) {
    let bluez = match connection.get().await {
        Ok(bluez) => bluez,
        Err(error) => return eprintln!("not watching Bluetooth Devices: {error:?}"),
    };
    if let Err(error) = Watcher::new(observations).run(&bluez.adapter).await {
        eprintln!("stopped watching Bluetooth Devices: {error}");
    }
}

struct Watcher {
    observations: mpsc::UnboundedSender<Observation>,
    /// Devices reported Connected, and not since reported disconnected.
    reported: HashSet<Address>,
    /// Devices whose events are being followed.
    followed: HashSet<Address>,
}

/// A Device's events, then `None` once bluer ends the stream.
type DeviceEvents = futures::stream::BoxStream<'static, (Address, Option<DeviceEvent>)>;

impl Watcher {
    fn new(observations: mpsc::UnboundedSender<Observation>) -> Watcher {
        Watcher {
            observations,
            reported: HashSet::new(),
            followed: HashSet::new(),
        }
    }

    async fn run(mut self, adapter: &Adapter) -> bluer::Result<()> {
        let mut adapter_events = adapter.events().await?;
        // Connected changes arrive per Device, so follow every Device BlueZ
        // knows, and every one it learns about.
        let mut device_events = SelectAll::new();
        for address in adapter.device_addresses().await? {
            self.follow(adapter, &mut device_events, address).await;
            self.check(adapter, address).await;
        }
        loop {
            tokio::select! {
                // bluer reports any interface added to or removed from a
                // Device, e.g. Battery1 on connect and disconnect, as the
                // Device being added or removed, so check what it is now.
                Some(event) = adapter_events.next() => match event {
                    AdapterEvent::DeviceAdded(address) | AdapterEvent::DeviceRemoved(address) => {
                        self.follow(adapter, &mut device_events, address).await;
                        self.check(adapter, address).await;
                    }
                    _ => {}
                },
                Some((address, event)) = device_events.next() => match event {
                    Some(DeviceEvent::PropertyChanged(property)) => match property {
                        DeviceProperty::Connected(false) => self.disconnected(address),
                        DeviceProperty::Connected(true)
                        | DeviceProperty::ServicesResolved(_)
                        | DeviceProperty::Uuids(_) => self.check(adapter, address).await,
                        _ => {}
                    },
                    // bluer also ends a Device's stream when any one of its
                    // interfaces is removed, though the Device remains.
                    None => {
                        self.followed.remove(&address);
                        self.follow(adapter, &mut device_events, address).await;
                        self.check(adapter, address).await;
                    }
                },
                else => {
                    eprintln!("stopped watching Bluetooth Devices: the adapter went away");
                    return Ok(());
                }
            }
        }
    }

    /// Follows the Device at `address`, if it exists and isn't followed.
    async fn follow(
        &mut self,
        adapter: &Adapter,
        device_events: &mut SelectAll<DeviceEvents>,
        address: Address,
    ) {
        if self.followed.contains(&address) {
            return;
        }
        let Ok(device) = adapter.device(address) else {
            return;
        };
        // Following a removed Device would end at once, and follow again.
        if device.is_connected().await.is_err() {
            return;
        }
        if let Ok(events) = device.events().await {
            self.followed.insert(address);
            let end = futures::stream::once(async move { (address, None) });
            device_events.push(
                events
                    .map(move |event| (address, Some(event)))
                    .chain(end)
                    .boxed(),
            );
        }
    }

    /// Reports the Device Connected once BlueZ knows its services. BlueZ
    /// resolves them after a new Device connects, but on a paired Device's
    /// reconnect it keeps the services it already knows and never sets
    /// ServicesResolved.
    async fn check(&mut self, adapter: &Adapter, address: Address) {
        let Ok(device) = adapter.device(address) else {
            return;
        };
        if !device.is_connected().await.unwrap_or(false) {
            return self.disconnected(address);
        }
        if self.reported.contains(&address) {
            return;
        }
        let service_uuids: HashSet<Uuid> = device.uuids().await.ok().flatten().unwrap_or_default();
        if service_uuids.is_empty() && !device.is_services_resolved().await.unwrap_or(false) {
            return;
        }
        let Ok(name) = device.alias().await else {
            return;
        };
        let service_uuids = service_uuids.into_iter().collect();
        self.reported.insert(address);
        let _ = self
            .observations
            .send(Observation::DeviceConnected(DeviceIdentity {
                address: port_address(address),
                name,
                service_uuids,
            }));
    }

    fn disconnected(&mut self, address: Address) {
        if self.reported.remove(&address) {
            let _ = self
                .observations
                .send(Observation::DeviceDisconnected(port_address(address)));
        }
    }
}

/// bluer prints addresses in the port's form, `AC:80:0A:11:22:33`.
fn port_address(address: Address) -> DeviceAddress {
    DeviceAddress(address.to_string())
}

impl Bluez {
    /// Opens a control link to `service` on the Connected Device at `address`.
    pub async fn open(
        &self,
        address: &DeviceAddress,
        service: Uuid,
    ) -> Result<ControlLink, LinkFailure> {
        let address: Address = address
            .0
            .parse()
            .map_err(|_| LinkFailure::Other(format!("not a Bluetooth address: {address}")))?;
        let device = self.adapter.device(address).map_err(failure)?;
        // ConnectProfile would connect a disconnected Device, and the app
        // never connects Devices.
        if !device.is_connected().await.map_err(failure)? {
            return Err(LinkFailure::DeviceNotConnected);
        }
        let waiters = self.profile(service).await?;
        let mut busy = 0;
        let stream = loop {
            let (tx, rx) = oneshot::channel();
            waiters.lock().unwrap().insert(address, tx);
            let result = connect_profile(&device, service, rx).await;
            waiters.lock().unwrap().remove(&address);
            match result {
                Ok(stream) => break stream,
                Err(ConnectProfileFailure::Busy) if busy < BUSY_RETRIES => {
                    busy += 1;
                    tokio::time::sleep(BUSY_PAUSE).await;
                }
                Err(ConnectProfileFailure::Busy) => {
                    return Err(LinkFailure::Other(
                        "Bluetooth stayed busy connecting the Device".into(),
                    ))
                }
                Err(ConnectProfileFailure::Fail(failure)) => return Err(failure),
            }
        };
        Ok(spawn_link(device, service, stream))
    }

    /// The Device waiters of the client Profile for `service`, registering
    /// the Profile if this is the first link to it.
    async fn profile(&self, service: Uuid) -> Result<Waiters, LinkFailure> {
        let mut profiles = self.profiles.lock().await;
        if let Some(waiters) = profiles.get(&service) {
            return Ok(waiters.clone());
        }
        let mut handle = self
            .session
            .register_profile(Profile {
                uuid: service,
                name: Some("Headphone Bridge".into()),
                role: Some(Role::Client),
                // Only open a link when asked, so the app never grabs one from
                // the vendor's app by itself.
                auto_connect: Some(false),
                ..Default::default()
            })
            .await
            .map_err(failure)?;
        let waiters = Waiters::default();
        // Always drain BlueZ's requests: ConnectProfile only returns once our
        // Profile has accepted or rejected the channel.
        let router = waiters.clone();
        tokio::spawn(async move {
            while let Some(request) = handle.next().await {
                let waiter = router.lock().unwrap().remove(&request.device());
                match waiter {
                    Some(waiter) => {
                        if let Err(oneshot_request) = waiter.send(request) {
                            oneshot_request.reject(ReqError::Canceled);
                        }
                    }
                    None => request.reject(ReqError::Rejected),
                }
            }
        });
        profiles.insert(service, waiters.clone());
        Ok(waiters)
    }
}

/// Asks BlueZ to open `service`'s channel and waits for it to reach our
/// Profile through `request`.
async fn connect_profile(
    device: &Device,
    service: Uuid,
    mut request: oneshot::Receiver<ConnectRequest>,
) -> Result<Stream, ConnectProfileFailure> {
    let fail = ConnectProfileFailure::Fail;
    let accept = |request: ConnectRequest| request.accept().map_err(|e| fail(failure(e)));
    let call = device.connect_profile(&service);
    tokio::pin!(call);
    let connecting = async {
        tokio::select! {
            arrived = &mut request => {
                let stream = accept(arrived.map_err(|_| fail(router_stopped()))?)?;
                let _ = call.await;
                Ok(stream)
            }
            called = &mut call => {
                called.map_err(|e| {
                    let (name, message) = dbus_error(&e);
                    connect_profile_failure(&name, &message)
                })?;
                // BlueZ reports success without handing us the channel when
                // it's already connected, through another process's Profile.
                match tokio::time::timeout(NEW_CONNECTION_GRACE, request).await {
                    Ok(Ok(arrived)) => accept(arrived),
                    _ => Err(fail(LinkFailure::InUseByAnotherProcess)),
                }
            }
        }
    };
    tokio::time::timeout(CONNECT_TIMEOUT, connecting)
        .await
        .unwrap_or_else(|_| {
            Err(fail(LinkFailure::Other(
                "Bluetooth didn't open the link in time".into(),
            )))
        })
}

/// Pumps the channel's bytes to and from the core. When the core drops the
/// link, the channel is closed through BlueZ, which keeps its own copy of
/// the socket.
fn spawn_link(device: Device, service: Uuid, stream: Stream) -> ControlLink {
    let (mut reader, mut writer) = stream.into_split();
    let (incoming_tx, incoming) = mpsc::unbounded_channel();
    let (outgoing, mut outgoing_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let reading = tokio::spawn(async move {
        let mut buffer = [0u8; 1024];
        loop {
            match reader.read(&mut buffer).await {
                Ok(0) | Err(_) => return,
                Ok(n) => {
                    if incoming_tx.send(buffer[..n].to_vec()).is_err() {
                        return;
                    }
                }
            }
        }
    });
    tokio::spawn(async move {
        while let Some(bytes) = outgoing_rx.recv().await {
            if writer.write_all(&bytes).await.is_err() {
                break;
            }
        }
        reading.abort();
        drop(writer);
        let _ = device.disconnect_profile(&service).await;
    });
    ControlLink { incoming, outgoing }
}

fn router_stopped() -> LinkFailure {
    LinkFailure::Other("Bluetooth stopped handing over links".into())
}

/// The D-Bus error name and BlueZ message `error` came from.
fn dbus_error(error: &bluer::Error) -> (String, String) {
    use bluer::{ErrorKind, InternalErrorKind};
    let name = match &error.kind {
        ErrorKind::NotFound => "org.freedesktop.DBus.Error.UnknownObject".to_string(),
        ErrorKind::Internal(InternalErrorKind::DBus(name)) => name.clone(),
        ErrorKind::Internal(_) => error.kind.to_string(),
        // The rest parse from `org.bluez.Error.<variant>`.
        kind => format!("org.bluez.Error.{kind:?}"),
    };
    (name, error.message.clone())
}

fn failure(error: bluer::Error) -> LinkFailure {
    let (name, message) = dbus_error(&error);
    match connect_profile_failure(&name, &message) {
        ConnectProfileFailure::Fail(failure) => failure,
        ConnectProfileFailure::Busy => LinkFailure::Other(error.to_string()),
    }
}
