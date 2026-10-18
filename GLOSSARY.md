# Headphone Bridge

An app for desktop and Android that reads and changes the sound settings of Bluetooth headphones and earbuds from different Vendors, replacing each Vendor's mobile app for everyday adjustments.

## Devices and connections

**Device**:
A specific pair of headphones or earbuds the app knows how to control, e.g. your Galaxy Buds4 Pro.
_Avoid_: Headset, buds, cans, peripheral

**Model**:
The product a Device is an instance of, e.g. Galaxy Buds4 Pro or WH-1000XM5. A Model determines which Capabilities a Device has.
_Avoid_: Device type, product, SKU

**Vendor**:
The company whose Models share one protocol for reading and changing settings, e.g. Samsung or Sony. The app speaks each Vendor's protocol in place of the Vendor's own app.
_Avoid_: Brand, manufacturer, maker

**Capability**:
A Noise Control Mode, adjustable setting, or action such as Power Off that a Model supports. The app only offers what a Device's Capabilities include.
_Avoid_: Feature, feature flag, support

**Connected**:
The state of a Device that the operating system has paired and connected for audio. The app observes this state; it never changes it.
_Avoid_: Paired, online

**Control Link**:
The connection to a Connected Device over which the app speaks its Vendor's protocol, separate from the audio. Only one app or device can hold a Device's Control Link at a time.
_Avoid_: Connection, socket, RFCOMM channel

**Attached**:
The state of a Connected Device for which the app has an open Control Link and can read and change settings. A Device can be Connected without being Attached, e.g. when another app holds the Control Link.
_Avoid_: Connected (when the Control Link is meant), linked, bound

**Detach**:
To close a Device's Control Link on purpose so another app or device can use it. The Device stays Connected.
_Avoid_: Release, yield, disconnect, let go

**Detached**:
The state of a Connected Device that the app has chosen not to Attach, as opposed to one it can't Attach.
_Avoid_: Idle, released, Not Attached (which is for when the app can't Attach)

**Hold**:
A request that keeps a Device Attached while it lasts, whatever the Attach Policy; when it ends, the Device can be Detached. A Hold is either on every Connected Device, ones that Connect later included, as the App Screen takes while it's in the foreground, or on the Primary Device alone, as a widget or tile action takes, which stays on that Device even if another becomes Primary meanwhile.
_Avoid_: Lock, lease, claim, keep-alive

**Attach Policy**:
When the app Attaches a Connected Device that no Hold is on: **Always**, for as long as it's Connected, as on desktop; or **On request**, never, so it stays Detached and another app or device can have its Control Link, as on Android.
_Avoid_: Attach mode, auto-attach

**Bluetooth Unavailable**:
Why the app can't Attach a Connected Device when the operating system's Bluetooth is switched off or not ready yet. It clears by itself once Bluetooth is ready.
_Avoid_: Bluetooth off, adapter down, no Bluetooth

**Power Off**:
The action of switching a Device off from the app. The Device then stops being Connected of its own accord; the app can't switch it back on.
_Avoid_: Turn off, shut down, disconnect

**Powering Off**:
The state of a Device the app has asked to Power Off that is still Connected. It ends when the Device stops being Connected, or with an error if the Device stays Connected too long.
_Avoid_: Shutting down, Pending (which is for setting values)

**Primary Device**:
The Connected Device the app shows and acts on by default: the one the operating system is currently using as its audio output.
_Avoid_: Active device, current device, selected device

## Settings

**Noise Control Mode**:
A Device's treatment of outside sound: Off, Noise Cancelling, Ambient, or Adaptive.
_Avoid_: ANC mode, sound mode, NC/ASM, listening mode

**Noise Cancelling**:
The Noise Control Mode that suppresses outside sound.
_Avoid_: ANC (in user-facing text)

**Ambient**:
The Noise Control Mode that passes outside sound through to the listener.
_Avoid_: Transparency, ambient sound mode, awareness, pass-through

**Adaptive**:
The Noise Control Mode in which the Device itself blends Noise Cancelling and Ambient according to surroundings.
_Avoid_: Auto, Adaptive Sound Control (a different Sony feature)

**Chosen Mode**:
The Noise Control Mode the listener last selected, by any means: this app, the Vendor's app, or a gesture on the Device.
_Avoid_: Selected mode, user mode, setting

**Effective Mode**:
The Noise Control Mode the Device is applying right now. Usually equal to the Chosen Mode, but can differ temporarily, e.g. while an earbud is out of the ear.
_Avoid_: Actual mode, current mode, real mode

**Level**:
An adjustable setting that belongs to one Noise Control Mode, expressed in the Model's own steps. A mode can have several Levels. Ambient Level and Noise Cancelling Level are Levels.
_Avoid_: Strength, intensity, slider

**Ambient Level**:
How much outside sound Ambient lets through, expressed in the Model's own steps.
_Avoid_: Ambient volume, transparency level

**Noise Cancelling Level**:
How strongly Noise Cancelling suppresses outside sound, expressed in the Model's own steps.
_Avoid_: ANC level, NC strength, noise reduction level

**Pending**:
The state of a setting value the app has asked a Device for, until the Device reports it, the request is rejected, or it times out. A Pending value is shown as if it already applied.
_Avoid_: Optimistic, in flight, unconfirmed

**Equalizer**:
The adjustable setting that shapes a Device's tonal balance. It is always set to one Preset.
_Avoid_: EQ, sound profile, tone

**Preset**:
A named Equalizer curve the Model offers, under the Model's own name, e.g. Bright or Dynamic. Some Presets, such as a Manual or Custom one, have Bands the listener can edit; the rest are fixed.
_Avoid_: Profile, sound mode, custom curve

**Band**:
One adjustable frequency region of the Equalizer, expressed in the Model's own steps.
_Avoid_: Slider, frequency, channel

**Clear Bass**:
A Model-specific bass enhancement adjusted alongside the Equalizer's Bands, expressed in the Model's own steps. It is not a Band.
_Avoid_: Bass boost, bass level, sub-band

**Equalizer Unavailable**:
The Status of a Device that has an Equalizer Capability but currently refuses Equalizer changes, as reported by the Device itself.
_Avoid_: Disabled, locked, unsupported

**Status**:
Read-only facts reported by a Device, such as battery charge and Effective Mode.
_Avoid_: Info, state

## The app

**App Screen**:
The app's own window on Android, as opposed to its home screen widget and Quick Settings tile.
_Avoid_: Main screen, activity, window (on Android)

**Simulated Device**:
A Device that exists only in a debug build of the app, answering as a real Model would, so the app can be tried without Bluetooth, e.g. on the Android emulator. It's Connected while switched on, and Attached and Detached like a real Device.
_Avoid_: Fake device, mock device, demo device
