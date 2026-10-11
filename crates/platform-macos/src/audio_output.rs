//! The system's default audio output, via CoreAudio.
//!
//! CoreAudio is thread-safe, so this is read from the Bluetooth thread, which
//! matches it to a Connected Device. Change notifications arrive on a HAL
//! thread and only ask the Bluetooth thread to read it again.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::NonNull;

use headphone_bridge_core::platform::DeviceAddress;
use objc2_core_audio::{
    kAudioDevicePropertyDeviceUID, kAudioDevicePropertyTransportType,
    kAudioDeviceTransportTypeBluetooth, kAudioDeviceTransportTypeBluetoothLE,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyDevices,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal,
    kAudioObjectSystemObject, AudioObjectAddPropertyListener, AudioObjectGetPropertyData,
    AudioObjectID, AudioObjectPropertyAddress, AudioObjectPropertySelector,
};
use objc2_core_foundation::{CFRetained, CFString};

/// The default output, when it's a Bluetooth audio device.
pub struct BluetoothOutput {
    /// The Device's address, when CoreAudio's UID carries it.
    pub address: Option<DeviceAddress>,
    /// CoreAudio's name for it, which is the Device's name.
    pub name: Option<String>,
}

/// The default output, or `None` when it isn't a Bluetooth audio device, e.g.
/// the built-in speakers.
pub fn default_output() -> Option<BluetoothOutput> {
    let system = kAudioObjectSystemObject as AudioObjectID;
    let device: AudioObjectID = get(system, kAudioHardwarePropertyDefaultOutputDevice)?;
    if device == 0 {
        return None;
    }
    let transport: u32 = get(device, kAudioDevicePropertyTransportType)?;
    if transport != kAudioDeviceTransportTypeBluetooth
        && transport != kAudioDeviceTransportTypeBluetoothLE
    {
        return None;
    }
    Some(BluetoothOutput {
        address: get_string(device, kAudioDevicePropertyDeviceUID)
            .and_then(|uid| address_from_uid(&uid)),
        name: get_string(device, kAudioObjectPropertyName),
    })
}

/// Calls `on_change` whenever the default output, or the set of audio devices,
/// changes. A Bluetooth Device's audio device appears a moment after it's
/// Connected, so both matter. Listens for the life of the process.
pub fn watch(on_change: impl Fn() + Send + Sync + 'static) {
    let on_change: Box<Box<dyn Fn() + Send + Sync>> = Box::new(Box::new(on_change));
    let context = Box::into_raw(on_change) as *mut c_void;
    for selector in [
        kAudioHardwarePropertyDefaultOutputDevice,
        kAudioHardwarePropertyDevices,
    ] {
        let address = property(selector);
        let status = unsafe {
            AudioObjectAddPropertyListener(
                kAudioObjectSystemObject as AudioObjectID,
                NonNull::from(&address),
                Some(listener),
                context,
            )
        };
        if status != 0 {
            eprintln!("couldn't watch the audio output: OSStatus {status}");
        }
    }
}

unsafe extern "C-unwind" fn listener(
    _object: AudioObjectID,
    _count: u32,
    _addresses: NonNull<AudioObjectPropertyAddress>,
    context: *mut c_void,
) -> i32 {
    // SAFETY: `context` is the callback `watch` leaked, so it lives forever.
    let on_change = unsafe { &*(context as *const Box<dyn Fn() + Send + Sync>) };
    on_change();
    0
}

fn property(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// Reads a fixed-size property. Only for plain integer types.
fn get<T: Copy + Default>(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
) -> Option<T> {
    let address = property(selector);
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    (status == 0 && size as usize == size_of::<T>()).then_some(value)
}

/// Reads a CFString property, which CoreAudio returns retained.
fn get_string(object: AudioObjectID, selector: AudioObjectPropertySelector) -> Option<String> {
    let address = property(selector);
    let mut value: *const CFString = std::ptr::null();
    let mut size = size_of::<*const CFString>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    let value = NonNull::new(value as *mut CFString).filter(|_| status == 0)?;
    // SAFETY: CoreAudio hands over a +1 reference, which this takes.
    Some(unsafe { CFRetained::from_raw(value) }.to_string())
}

/// The Device address at the start of a Classic Bluetooth device's UID, e.g.
/// `AC-80-0A-11-22-33:output`. Other UIDs, such as LE Audio's, give `None`.
fn address_from_uid(uid: &str) -> Option<DeviceAddress> {
    let prefix = uid.get(..17)?;
    let octets: Vec<&str> = prefix.split(['-', ':']).collect();
    let valid = octets.len() == 6
        && octets
            .iter()
            .all(|octet| octet.len() == 2 && octet.chars().all(|c| c.is_ascii_hexdigit()));
    valid.then(|| DeviceAddress(octets.join(":").to_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_address_from_a_classic_bluetooth_uid() {
        assert_eq!(
            address_from_uid("ac-80-0A-11-22-33:output"),
            Some(DeviceAddress("AC:80:0A:11:22:33".into()))
        );
    }

    #[test]
    fn reads_no_address_from_other_uids() {
        assert_eq!(address_from_uid("BuiltInSpeakerDevice"), None);
        assert_eq!(address_from_uid("AppleUSBAudioEngine:Foo:1"), None);
        assert_eq!(address_from_uid("AC-80-0A"), None);
    }
}
