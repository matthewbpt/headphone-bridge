import { act } from "react";
import type { DeviceList, Labels } from "../deviceList";

/**
 * A scripted stand-in for the core, behind the Tauri commands and events the
 * window uses. Tests render the whole window against it, emit Device list
 * snapshots, and check the commands it records.
 *
 * Like the real core, it never changes the snapshot itself: a command only
 * asks the Device, and the test emits the snapshot that confirms it (or
 * doesn't).
 */
export class FakeCore {
  private deviceList: DeviceList;
  private readonly listeners = new Set<(event: { payload: DeviceList }) => void>();
  private readonly answers = new Map<Command, Answer>();
  private readonly calls: Call[] = [];
  private launchAtLogin = false;
  private frameLog: string | null = null;

  constructor(deviceList: DeviceList) {
    this.deviceList = structuredClone(deviceList);
  }

  /** Emits a new Device list snapshot, as the core does after every change. */
  emit(deviceList: DeviceList) {
    this.deviceList = structuredClone(deviceList);
    act(() => {
      for (const listener of this.listeners) listener({ payload: structuredClone(deviceList) });
    });
  }

  /**
   * How the core answers every later call of `command`: accept it at once
   * (the default), reject it with an error, or hold it until `release`.
   */
  answer(command: Command, answer: Answer) {
    this.answers.set(command, answer);
  }

  /** Answers the oldest held call of `command`. */
  async release(command: Command, answer: Exclude<Answer, "hold"> = "accept") {
    const call = this.calls.find((call) => call.command === command && call.held);
    if (!call) throw new Error(`no held ${command} call to release`);
    call.held = false;
    await act(async () => {
      if (answer === "accept") call.settle.resolve();
      else call.settle.reject(answer.reject);
    });
  }

  /** Every command the window sent, oldest first. */
  sent(): SentCommand[] {
    return this.calls.map(({ command, args }) => ({ command, args }));
  }

  /** The window's `invoke`, for `@tauri-apps/api/core`. */
  invoke(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
    switch (command) {
      case "labels":
        return Promise.resolve(structuredClone(LABELS));
      case "device_list":
        return Promise.resolve(structuredClone(this.deviceList));
      case "set_noise_control_mode":
      case "set_ambient_level":
      case "retry_attach":
        return this.record(command, args);
      case "frame_log":
        return Promise.resolve(this.frameLog);
      case "set_frame_log":
        this.frameLog = args.enabled ? "/tmp/headphone-bridge-frames.log" : null;
        return Promise.resolve(this.frameLog);
      default:
        return Promise.reject(`the fake core has no command ${command}`);
    }
  }

  /** The window's `listen`, for `@tauri-apps/api/event`. */
  listen(event: string, handler: (event: { payload: DeviceList }) => void): () => void {
    if (event !== "device-list") return () => {};
    this.listeners.add(handler);
    return () => this.listeners.delete(handler);
  }

  /** The OS login item, for `@tauri-apps/plugin-autostart`. */
  readonly autostart = {
    isEnabled: () => Promise.resolve(this.launchAtLogin),
    enable: () => {
      this.launchAtLogin = true;
      return Promise.resolve();
    },
    disable: () => {
      this.launchAtLogin = false;
      return Promise.resolve();
    },
  };

  private record(command: Command, args: Record<string, unknown>): Promise<void> {
    const answer = this.answers.get(command) ?? "accept";
    let settle!: Call["settle"];
    const promise = new Promise<void>((resolve, reject) => (settle = { resolve, reject }));
    this.calls.push({ command, args, settle, held: answer === "hold" });
    if (answer === "accept") settle.resolve();
    else if (answer !== "hold") settle.reject(answer.reject);
    return promise;
  }
}

/** The commands the fake core records. */
export type Command = "set_noise_control_mode" | "set_ambient_level" | "retry_attach";

/** Accept at once, reject with the core's error, or hold until released. */
export type Answer = "accept" | "hold" | { reject: unknown };

export interface SentCommand {
  command: Command;
  args: Record<string, unknown>;
}

interface Call extends SentCommand {
  settle: { resolve: () => void; reject: (error: unknown) => void };
  held: boolean;
}

/** The core's labels, copied from app/src-tauri/src/labels.rs. */
export const LABELS: Labels = {
  noDevice: "No Device",
  modes: {
    off: "Off",
    noiseCancelling: "Noise Cancelling",
    ambient: "Ambient",
    adaptive: "Adaptive",
  },
  models: { galaxyBuds4Pro: "Galaxy Buds4 Pro", wh1000xm5: "WH-1000XM5" },
  hints: { budOutOfEar: "Paused: an earbud is out of the ear", unknown: "Paused by the Device" },
  reasons: [
    { reason: { kind: "unsupportedModel" }, text: "Unsupported Model" },
    { reason: { kind: "inUseByAnotherProcess" }, text: "In use by another app" },
    {
      reason: { kind: "bluetoothPermissionDenied" },
      text: "Bluetooth permission denied: enable it in System Settings › Privacy & Security › Bluetooth",
    },
    { reason: { kind: "serviceNotFound" }, text: "Control service not found" },
    { reason: { kind: "deviceNotConnected" }, text: "Not Connected" },
    { reason: { kind: "linkDropped" }, text: "Control link dropped" },
    { reason: { kind: "noResponse" }, text: "The Device didn't respond" },
  ],
};

/** The core the Tauri API stand-ins talk to; set by `renderWindow`. */
let current: FakeCore | null = null;

export function useFakeCore(core: FakeCore | null) {
  current = core;
}

export function currentCore(): FakeCore {
  if (!current) throw new Error("no fake core: render the window with renderWindow");
  return current;
}
