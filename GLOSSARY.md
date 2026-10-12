# Headphone Bridge

A desktop app that reads and changes the sound settings of Bluetooth headphones and earbuds from different vendors, replacing each vendor's mobile app for everyday adjustments.

## Devices and connections

**Device**:
A specific pair of headphones or earbuds the app knows how to control, e.g. your Galaxy Buds4 Pro.
_Avoid_: Headset, buds, cans, peripheral

**Model**:
The product a Device is an instance of, e.g. Galaxy Buds4 Pro or WH-1000XM5. A Model determines which Capabilities a Device has.
_Avoid_: Device type, product, SKU

**Capability**:
A Noise Control Mode, adjustable setting, or action such as Power Off that a Model supports. The app only offers what a Device's Capabilities include.
_Avoid_: Feature, feature flag, support

**Connected**:
The state of a Device that the operating system has paired and connected for audio. The app observes this state; it never changes it.
_Avoid_: Paired, online

**Attached**:
The state of a Connected Device for which the app has an open control link and can read and change settings. A Device can be Connected without being Attached, e.g. when another app holds the control link.
_Avoid_: Connected (when the control link is meant), linked, bound

**Power Off**:
The action of switching a Device off from the app. The Device then stops being Connected of its own accord; the app can't switch it back on.
_Avoid_: Turn off, shut down, disconnect

**Powering Off**:
The state of a Device the app has asked to Power Off that is still Connected. It ends when the Device stops being Connected, or with an error if the Device stays Connected too long.
_Avoid_: Shutting down, Pending (which is for setting values)

**Primary Device**:
The Attached Device shown by default: the one the operating system is currently using as its audio output.
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
The Noise Control Mode the listener last selected, by any means: this app, the vendor app, or a gesture on the Device.
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

**Status**:
Read-only facts reported by a Device, such as battery charge and Effective Mode.
_Avoid_: Info, state
