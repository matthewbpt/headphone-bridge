//! Objective-C objects that receive IOBluetooth callbacks. They run on the
//! Bluetooth thread while it pumps its run loop, and only queue what happened.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::rc::Rc;

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

pub type DeviceEvents = Rc<RefCell<VecDeque<DeviceEvent>>>;

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
        self.ivars().events.borrow_mut().push_back(event);
    }
}

pub struct RfcommDelegateIvars {
    /// Dropped when the channel closes, which ends the core's `incoming`.
    incoming: RefCell<Option<mpsc::UnboundedSender<Vec<u8>>>>,
    /// The status of the last `rfcommChannelOpenComplete:status:`.
    open_status: Cell<Option<i32>>,
    closed: Cell<bool>,
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
            if let Some(incoming) = &*self.ivars().incoming.borrow() {
                let _ = incoming.send(bytes);
            }
        }

        #[unsafe(method(rfcommChannelOpenComplete:status:))]
        fn rfcomm_channel_open_complete(&self, _channel: *mut AnyObject, status: i32) {
            self.ivars().open_status.set(Some(status));
        }

        #[unsafe(method(rfcommChannelClosed:))]
        fn rfcomm_channel_closed(&self, _channel: *mut AnyObject) {
            self.ivars().closed.set(true);
            self.ivars().incoming.borrow_mut().take();
        }
    }
);

impl RfcommDelegate {
    pub fn new(incoming: mpsc::UnboundedSender<Vec<u8>>) -> Retained<RfcommDelegate> {
        let this = RfcommDelegate::alloc().set_ivars(RfcommDelegateIvars {
            incoming: RefCell::new(Some(incoming)),
            open_status: Cell::new(None),
            closed: Cell::new(false),
        });
        unsafe { msg_send![super(this), init] }
    }

    pub fn open_status(&self) -> Option<i32> {
        self.ivars().open_status.get()
    }

    pub fn is_closed(&self) -> bool {
        self.ivars().closed.get()
    }

    /// Ends the core's `incoming` stream.
    pub fn close(&self) {
        self.ivars().closed.set(true);
        self.ivars().incoming.borrow_mut().take();
    }
}
