//! Objective-C objects that receive IOBluetooth callbacks, and only queue what
//! happened for the Bluetooth thread. IOBluetooth calls them on the main
//! thread (RFCOMM channels and, after startup, notifications) or on the
//! Bluetooth thread, so their state is shared between threads.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass, Message};
use objc2_io_bluetooth::{IOBluetoothDevice, IOBluetoothUserNotification};
use tokio::sync::mpsc;

/// What the [`DeviceWatcher`] saw.
pub enum DeviceEvent {
    Connected(Retained<IOBluetoothDevice>),
    Disconnected(Retained<IOBluetoothDevice>),
    SdpQueryComplete(Retained<IOBluetoothDevice>),
}

// SAFETY: the main thread only retains the device and queues it; only the
// Bluetooth thread calls its methods. Retain and release are thread-safe.
unsafe impl Send for DeviceEvent {}

#[derive(Clone, Default)]
pub struct DeviceEvents(Arc<Mutex<VecDeque<DeviceEvent>>>);

impl DeviceEvents {
    pub fn lock(&self) -> MutexGuard<'_, VecDeque<DeviceEvent>> {
        lock(&self.0)
    }
}

pub struct DeviceWatcherIvars {
    events: DeviceEvents,
}

define_class!(
    /// The target of connect, disconnect and SDP query notifications.
    #[unsafe(super(NSObject))]
    #[name = "HBDeviceWatcher"]
    #[ivars = DeviceWatcherIvars]
    pub struct DeviceWatcher;

    impl DeviceWatcher {
        #[unsafe(method(deviceConnected:fromDevice:))]
        fn device_connected(&self, _notification: *mut AnyObject, device: &IOBluetoothDevice) {
            self.push(DeviceEvent::Connected(device.retain()));
        }

        #[unsafe(method(deviceDisconnected:fromDevice:))]
        fn device_disconnected(
            &self,
            _notification: &IOBluetoothUserNotification,
            device: &IOBluetoothDevice,
        ) {
            self.push(DeviceEvent::Disconnected(device.retain()));
        }

        #[unsafe(method(sdpQueryComplete:status:))]
        fn sdp_query_complete(&self, device: &IOBluetoothDevice, _status: i32) {
            self.push(DeviceEvent::SdpQueryComplete(device.retain()));
        }
    }
);

impl DeviceWatcher {
    pub fn new(events: DeviceEvents) -> Retained<DeviceWatcher> {
        let this = DeviceWatcher::alloc().set_ivars(DeviceWatcherIvars { events });
        unsafe { msg_send![super(this), init] }
    }

    fn push(&self, event: DeviceEvent) {
        self.ivars().events.lock().push_back(event);
    }
}

pub struct RfcommDelegateIvars {
    /// Dropped when the channel closes, which ends the core's `incoming`.
    incoming: Mutex<Option<mpsc::UnboundedSender<Vec<u8>>>>,
    /// The status of the last `rfcommChannelOpenComplete:status:`.
    open_status: Mutex<Option<i32>>,
    closed: AtomicBool,
}

define_class!(
    /// The delegate of one RFCOMM channel. IOBluetooth doesn't retain it, so
    /// it must be kept alive as long as the channel.
    #[unsafe(super(NSObject))]
    #[name = "HBRfcommDelegate"]
    #[ivars = RfcommDelegateIvars]
    pub struct RfcommDelegate;

    impl RfcommDelegate {
        #[unsafe(method(rfcommChannelData:data:length:))]
        fn rfcomm_channel_data(&self, _channel: *mut AnyObject, data: *mut c_void, length: usize) {
            if data.is_null() || length == 0 {
                return;
            }
            // The buffer is only valid during the callback.
            let bytes = unsafe { std::slice::from_raw_parts(data as *const u8, length) }.to_vec();
            if let Some(incoming) = &*self.incoming() {
                let _ = incoming.send(bytes);
            }
        }

        #[unsafe(method(rfcommChannelOpenComplete:status:))]
        fn rfcomm_channel_open_complete(&self, _channel: *mut AnyObject, status: i32) {
            *lock(&self.ivars().open_status) = Some(status);
        }

        #[unsafe(method(rfcommChannelClosed:))]
        fn rfcomm_channel_closed(&self, _channel: *mut AnyObject) {
            self.close();
        }
    }
);

impl RfcommDelegate {
    pub fn new(incoming: mpsc::UnboundedSender<Vec<u8>>) -> Retained<RfcommDelegate> {
        let this = RfcommDelegate::alloc().set_ivars(RfcommDelegateIvars {
            incoming: Mutex::new(Some(incoming)),
            open_status: Mutex::new(None),
            closed: AtomicBool::new(false),
        });
        unsafe { msg_send![super(this), init] }
    }

    pub fn open_status(&self) -> Option<i32> {
        *lock(&self.ivars().open_status)
    }

    pub fn is_closed(&self) -> bool {
        self.ivars().closed.load(Ordering::Acquire)
    }

    /// Ends the core's `incoming` stream.
    pub fn close(&self) {
        self.ivars().closed.store(true, Ordering::Release);
        self.incoming().take();
    }

    fn incoming(&self) -> MutexGuard<'_, Option<mpsc::UnboundedSender<Vec<u8>>>> {
        lock(&self.ivars().incoming)
    }
}

/// Locks `mutex`, ignoring poisoning: every value it guards stays valid.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
