import type { Capabilities, Device, DeviceList } from "../deviceList";

// Made-up Devices for the window's tests, shaped like the core's snapshots.

export const BUDS_ADDRESS = "AA:BB:CC:00:00:01";
export const XM5_ADDRESS = "AA:BB:CC:00:00:02";

const buds4ProCapabilities: Capabilities = {
  noiseControlModes: ["off", "noiseCancelling", "ambient", "adaptive"],
  ambientLevel: { min: 0, max: 4 },
  noiseCancellingLevel: { min: 0, max: 4 },
  powerOff: false,
};

const xm5Capabilities: Capabilities = {
  noiseControlModes: ["off", "noiseCancelling", "ambient"],
  ambientLevel: { min: 1, max: 20 },
  noiseCancellingLevel: null,
  powerOff: true,
};

/** An Attached Galaxy Buds4 Pro in Ambient. */
export function buds4Pro(overrides: Partial<Device> = {}): Device {
  return {
    address: BUDS_ADDRESS,
    name: "Matthew's Buds4 Pro",
    model: "galaxyBuds4Pro",
    state: { kind: "attached" },
    capabilities: buds4ProCapabilities,
    chosenMode: "ambient",
    effectiveMode: "ambient",
    effectiveModeReason: null,
    ambientLevel: 1,
    noiseCancellingLevel: 4,
    battery: { kind: "buds", left: 100, right: 100, case: null },
    powerOffFailed: false,
    reportedAt: null,
    ...overrides,
  };
}

/** An Attached WH-1000XM5 in Noise Cancelling. */
export function xm5(overrides: Partial<Device> = {}): Device {
  return {
    address: XM5_ADDRESS,
    name: "WH-1000XM5",
    model: "wh1000xm5",
    state: { kind: "attached" },
    capabilities: xm5Capabilities,
    chosenMode: "noiseCancelling",
    effectiveMode: "noiseCancelling",
    effectiveModeReason: null,
    ambientLevel: 12,
    noiseCancellingLevel: null,
    battery: { kind: "headphones", level: 70 },
    powerOffFailed: false,
    reportedAt: null,
    ...overrides,
  };
}

/** A Device not yet Attached: nothing reported. */
export function notAttached(device: Device, state: Device["state"]): Device {
  return {
    ...device,
    state,
    capabilities: { noiseControlModes: [], ambientLevel: null, noiseCancellingLevel: null, powerOff: false },
    chosenMode: null,
    effectiveMode: null,
    effectiveModeReason: null,
    ambientLevel: null,
    noiseCancellingLevel: null,
    battery: null,
    reportedAt: null,
  };
}

/**
 * A Device Detached during the same Connected spell, e.g. while it reattaches:
 * nothing reported, but the core keeps its Model and Capabilities.
 */
export function reattaching(device: Device, state: Device["state"]): Device {
  return { ...notAttached(device, state), model: device.model, capabilities: device.capabilities };
}

/** A Device list of one Device, Primary while it's Attached. */
export function only(device: Device): DeviceList {
  return {
    devices: [device],
    primary: device.state.kind === "attached" ? device.address : null,
  };
}

/** No Device Connected, as Android reports when the app can't see them. */
export const NO_DEVICES: DeviceList = { devices: [], primary: null };
