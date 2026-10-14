import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import type { DeviceList, Labels, NoiseControlMode } from "./deviceList";

// The window's only way in to the core: its commands, as named and typed
// functions, and the hooks that follow its snapshots, labels and frame log.

/**
 * Every command the window sends the core, by the name the shell handles it
 * under (app/src-tauri/src/lib.rs), with its arguments and result. Exported
 * for the fake core alone, which answers each command typed against it;
 * everything else calls the named functions below.
 */
export type Commands = {
  labels: { args: Record<string, never>; result: Labels };
  platform: { args: Record<string, never>; result: Platform };
  device_list: { args: Record<string, never>; result: DeviceList };
  set_noise_control_mode: { args: { address: string; mode: NoiseControlMode }; result: void };
  set_ambient_level: { args: { address: string; level: number }; result: void };
  set_noise_cancelling_level: { args: { address: string; level: number }; result: void };
  retry_attach: { args: { address: string }; result: void };
  power_off: { args: { address: string }; result: void };
  frame_log: { args: Record<string, never>; result: string | null };
  set_frame_log: { args: { enabled: boolean }; result: string | null };
  share_frame_log: { args: Record<string, never>; result: void };
};

/** Which kind of app the window is in: some settings exist only on one. */
export type Platform = "desktop" | "android";

/** Why the core rejected a command before sending anything. */
export type CommandError =
  | "unknownDevice"
  | "notAttached"
  | "unsupportedModel"
  | "poweringOff"
  | "missingCapability"
  | "outOfRange"
  | "settingsNotReported"
  | { frameLogUnavailable: string };

/** Why the shell couldn't hand the frame log to Android's share sheet. */
export type ShareError = "frameLogOff" | "frameLogEmpty" | { shareFailed: string };

/** Why a command was rejected, by the core or the shell. */
type Rejection = CommandError | ShareError;

/** Each reason given, as the name it's sent under. */
type RejectionKey = Rejection extends infer E ? (E extends string ? E : keyof E) : never;

/**
 * Each reason in words, for the listener. A reason that carries a message is
 * worded from it. The type makes every reason in `Rejection` need words.
 */
const WORDS: {
  [K in RejectionKey]: K extends Rejection
    ? string
    : (message: Extract<Rejection, Record<K, unknown>>[K]) => string;
} = {
  unknownDevice: "the Device isn't Connected",
  notAttached: "the Device isn't Attached",
  unsupportedModel: "the Model isn't supported",
  poweringOff: "the Device is Powering Off",
  missingCapability: "the Model doesn't have that setting",
  outOfRange: "that value is outside the Model's range",
  settingsNotReported: "the Device hasn't reported its settings yet",
  frameLogUnavailable: (message) => message,
  frameLogOff: "the frame log is off",
  frameLogEmpty: "it's empty until a Device is Attached",
  shareFailed: (message) => message,
};

/**
 * The core's reason as the name it's sent under, with the message it carries
 * if any, or null when `error` isn't shaped like one.
 */
function reasonOf(error: unknown): [key: string, message?: unknown] | null {
  if (typeof error === "string") return [error];
  if (typeof error === "object" && error !== null) {
    const entries = Object.entries(error);
    if (entries.length === 1) return entries[0];
  }
  return null;
}

/**
 * Why a command was rejected, in words: the core's reason from `WORDS`, or
 * anything else (such as a transport failure) in its own string form.
 */
function inWords(error: unknown): string {
  const reason = reasonOf(error);
  if (reason && Object.hasOwn(WORDS, reason[0])) {
    const words = WORDS[reason[0] as RejectionKey];
    if (typeof words === "string" && reason.length === 1) return words;
    if (typeof words === "function" && reason.length === 2) return words(reason[1] as never);
  }
  return String(error);
}

/**
 * Why a command below was rejected, in words, for a caller's `catch`: the
 * message of the `Error` it rejects with.
 */
export function whyRejected(rejection: unknown): string {
  return rejection instanceof Error ? rejection.message : inWords(rejection);
}

/** Sends `command`; when the core rejects it, rejects with an `Error` whose message is why, in words. */
function send<C extends keyof Commands>(
  command: C,
  args: Commands[C]["args"],
): Promise<Commands[C]["result"]> {
  return invoke<Commands[C]["result"]>(command, args).catch((error: unknown) => {
    throw new Error(inWords(error));
  });
}

/** Asks the Device to switch mode. Resolves once the core accepts the command. */
export function setNoiseControlMode(address: string, mode: NoiseControlMode): Promise<void> {
  return send("set_noise_control_mode", { address, mode });
}

/** Asks the Device to set the Ambient Level, in the Model's own steps. */
export function setAmbientLevel(address: string, level: number): Promise<void> {
  return send("set_ambient_level", { address, level });
}

/** Asks the Device to set the Noise Cancelling Level, in the Model's own steps. */
export function setNoiseCancellingLevel(address: string, level: number): Promise<void> {
  return send("set_noise_cancelling_level", { address, level });
}

/** Tries again to Attach a Connected Device that isn't Attached. */
export function retryAttach(address: string): Promise<void> {
  return send("retry_attach", { address });
}

/** Asks the Device to Power Off. Resolves once the core accepts the command. */
export function powerOff(address: string): Promise<void> {
  return send("power_off", { address });
}

/**
 * Turns the frame log on or off. Resolves with the file it's logging to, or
 * null once it's off.
 */
function setFrameLog(enabled: boolean): Promise<string | null> {
  return send("set_frame_log", { enabled });
}

/**
 * Hands the frame log file to Android's share sheet. Rejects, with why in
 * words, while the frame log is off or empty.
 */
export function shareFrameLog(): Promise<void> {
  return send("share_frame_log", {});
}

/**
 * The frame log's file: null while it's off, undefined until it's known. Turning
 * it on or off through `setEnabled` follows the file; when the core rejects
 * that, it rejects with why, in words, and the file is unchanged.
 */
export function useFrameLog(): {
  file: string | null | undefined;
  setEnabled: (enabled: boolean) => Promise<void>;
} {
  const [file, setFile] = useState<string | null | undefined>(undefined);
  useEffect(() => {
    send("frame_log", {}).then(setFile);
  }, []);
  const setEnabled = useCallback((enabled: boolean) => setFrameLog(enabled).then(setFile), []);
  return { file, setEnabled };
}

/** Which kind of app the window is in, or null until it's known. */
export function usePlatform(): Platform | null {
  const [platform, setPlatform] = useState<Platform | null>(null);
  useEffect(() => {
    send("platform", {}).then(setPlatform);
  }, []);
  return platform;
}

/** The labels, or null until they arrive. */
export function useLabels(): Labels | null {
  const [labels, setLabels] = useState<Labels | null>(null);
  useEffect(() => {
    send("labels", {}).then(setLabels);
  }, []);
  return labels;
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
    send("device_list", {}).then((current) => {
      if (!receivedEvent) setDeviceList(current);
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);

  return deviceList;
}
