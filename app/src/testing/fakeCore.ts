import { act } from "react";
import type { Commands, Events, Permission, Platform } from "../core";
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
  private readonly listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
  private readonly answers = new Map<AnsweredCommand, Answer>();
  private readonly calls: Call[] = [];
  private launchAtLogin = false;
  private frameLog: string | null = null;
  private readonly platform: Platform;
  private nearbyDevices: Permission;
  private appSettingsOpens = 0;

  /**
   * A core in the desktop app, unless `platform` says otherwise, with the
   * Nearby devices permission granted unless `nearbyDevices` says otherwise.
   */
  constructor(
    deviceList: DeviceList,
    {
      platform = "desktop",
      nearbyDevices = "granted",
    }: { platform?: Platform; nearbyDevices?: Permission } = {},
  ) {
    this.deviceList = structuredClone(deviceList);
    this.platform = platform;
    this.nearbyDevices = nearbyDevices;
  }

  /** Emits a new Device list snapshot, as the core does after every change. */
  emit(deviceList: DeviceList) {
    this.deviceList = structuredClone(deviceList);
    this.send("device-list", deviceList);
  }

  /**
   * Changes the Nearby devices permission, as the listener does in the app's
   * Android settings, and reports it as the app does on coming back.
   */
  setNearbyDevices(permission: Permission) {
    this.nearbyDevices = permission;
    this.send("nearby-devices-permission", permission);
  }

  /** How many times the window opened the app's Android settings. */
  appSettingsOpened(): number {
    return this.appSettingsOpens;
  }

  private send<E extends keyof Events>(event: E, payload: Events[E]) {
    act(() => {
      for (const listener of this.listeners.get(event) ?? []) {
        listener({ payload: structuredClone(payload) });
      }
    });
  }

  /**
   * How the core answers every later call of `command`: accept it at once
   * (the default), reject it with an error, or hold it until `release`.
   * Turning the frame log on or off is answered at once: accepted or rejected.
   */
  answer<C extends AnsweredCommand>(
    command: C,
    answer: C extends RecordedCommand ? Answer : Exclude<Answer, "hold">,
  ) {
    this.answers.set(command, answer);
  }

  /** Answers the oldest held call of `command`. */
  async release(command: RecordedCommand, answer: Exclude<Answer, "hold"> = "accept") {
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
    if (!Object.hasOwn(this.commands, command)) {
      return Promise.reject(`the fake core has no command ${command}`);
    }
    const answer = this.commands[command as keyof Commands] as (args: unknown) => Promise<unknown>;
    return answer(args);
  }

  /** How the fake core answers each of the window's commands. */
  private readonly commands: {
    [C in keyof Commands]: (args: Commands[C]["args"]) => Promise<Commands[C]["result"]>;
  } = {
    labels: () => Promise.resolve(structuredClone(LABELS)),
    platform: () => Promise.resolve(this.platform),
    device_list: () => Promise.resolve(structuredClone(this.deviceList)),
    set_noise_control_mode: (args) => this.record("set_noise_control_mode", args),
    set_ambient_level: (args) => this.record("set_ambient_level", args),
    set_noise_cancelling_level: (args) => this.record("set_noise_cancelling_level", args),
    retry_attach: (args) => this.record("retry_attach", args),
    power_off: (args) => this.record("power_off", args),
    frame_log: () => Promise.resolve(this.frameLog),
    set_frame_log: ({ enabled }) => {
      const answer = this.answers.get("set_frame_log") ?? "accept";
      if (typeof answer === "object") return Promise.reject(answer.reject);
      this.frameLog = enabled ? "/tmp/headphone-bridge-frames.log" : null;
      return Promise.resolve(this.frameLog);
    },
    share_frame_log: (args) => this.record("share_frame_log", args),
    nearby_devices_permission: () => Promise.resolve(this.nearbyDevices),
    open_app_settings: () => {
      this.appSettingsOpens += 1;
      return Promise.resolve();
    },
  };

  /** The window's `listen`, for `@tauri-apps/api/event`. */
  listen(event: string, handler: (event: { payload: unknown }) => void): () => void {
    const listeners = this.listeners.get(event) ?? new Set();
    this.listeners.set(event, listeners);
    listeners.add(handler);
    return () => listeners.delete(handler);
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

  private record<C extends RecordedCommand>(command: C, args: Commands[C]["args"]): Promise<void> {
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
export type RecordedCommand =
  | "set_noise_control_mode"
  | "set_ambient_level"
  | "set_noise_cancelling_level"
  | "retry_attach"
  | "power_off"
  | "share_frame_log";

/** The commands a test can tell the fake core how to answer. */
type AnsweredCommand = RecordedCommand | "set_frame_log";

/** Accept at once, reject with the core's error, or hold until released. */
export type Answer = "accept" | "hold" | { reject: unknown };

export interface SentCommand {
  command: RecordedCommand;
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
    { reason: { kind: "linkDropped" }, text: "Another app or device may be using it" },
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
