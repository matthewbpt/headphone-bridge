import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

// Mirrors the core's Device list snapshot (crates/core/src/device_list.rs).
export interface DeviceList {
  devices: Device[];
}

export interface Device {
  address: string;
  /** The name the OS reports. */
  name: string;
  /** Null until identified, and for unsupported Models. */
  model: Model | null;
  state: DeviceState;
  capabilities: Capabilities;
  /** The Noise Control Mode last selected by any means. Null until reported. */
  chosenMode: NoiseControlMode | null;
  /** In the Model's own steps, within `capabilities.ambientLevel`. Null until reported. */
  ambientLevel: number | null;
  /** Null until the Device first reports it. */
  battery: Battery | null;
}

export type Model = "galaxyBuds4Pro" | "wh1000xm5";

export type DeviceState =
  | { kind: "attaching" }
  | { kind: "attached" }
  | { kind: "notAttached"; reason: NotAttachedReason };

export type NotAttachedReason =
  | { kind: "unsupportedModel" }
  | { kind: "inUseByAnotherProcess" }
  | { kind: "bluetoothPermissionDenied" }
  | { kind: "serviceNotFound" }
  | { kind: "deviceNotConnected" }
  | { kind: "linkDropped" }
  | { kind: "other"; message: string };

export type NoiseControlMode = "off" | "noiseCancelling" | "ambient" | "adaptive";

export interface Capabilities {
  noiseControlModes: NoiseControlMode[];
  ambientLevel: { min: number; max: number } | null;
}

/** Percentages; for buds, null where the level is unknown. */
export type Battery =
  | { kind: "buds"; left: number | null; right: number | null; case: number | null }
  | { kind: "headphones"; level: number };

/** Why the core rejected a command before sending anything. */
export type CommandError =
  | "unknownDevice"
  | "notAttached"
  | "missingCapability"
  | "outOfRange"
  | { frameLogUnavailable: string };

/** Asks the Device to switch mode. Resolves once the core accepts the command. */
export function setNoiseControlMode(address: string, mode: NoiseControlMode): Promise<void> {
  return invoke("set_noise_control_mode", { address, mode });
}

/** Asks the Device to set the Ambient Level, in the Model's own steps. */
export function setAmbientLevel(address: string, level: number): Promise<void> {
  return invoke("set_ambient_level", { address, level });
}

/**
 * The latest Device list snapshot from the core, or null until the first one
 * arrives. The UI holds no device state beyond this.
 */
export function useDeviceList(): DeviceList | null {
  const [deviceList, setDeviceList] = useState<DeviceList | null>(null);

  useEffect(() => {
    let receivedEvent = false;
    const unlisten = listen<DeviceList>("device-list", (event) => {
      receivedEvent = true;
      setDeviceList(event.payload);
    });
    // Snapshots emitted before the listener was registered are missed, so
    // start from the current one unless an event has already superseded it.
    invoke<DeviceList>("device_list").then((current) => {
      if (!receivedEvent) setDeviceList(current);
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);

  return deviceList;
}
