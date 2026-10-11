import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

// Mirrors the core's Device list snapshot (crates/core/src/device_list.rs).
export interface DeviceList {
  devices: Device[];
}

export interface Device {
  address: string;
  name: string;
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
