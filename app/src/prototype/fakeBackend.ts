// PROTOTYPE: an in-memory core for the window redesign. Answers the
// commands deviceList.ts and the settings invoke, and emits "device-list"
// after each change. Setters land after ~400 ms so pending states show.
import type { DeviceList, Labels, NoiseControlMode } from "../deviceList";
import { SCENARIOS, type Scenario } from "./scenarios";

// Copied from app/src-tauri/src/labels.rs.
const LABELS: Labels = {
  noDevice: "No Device",
  modes: { off: "Off", noiseCancelling: "Noise Cancelling", ambient: "Ambient", adaptive: "Adaptive" },
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

const DELAY_MS = 400;

export function currentScenario(): Scenario {
  const key = new URLSearchParams(location.search).get("scenario");
  return SCENARIOS.find((s) => s.key === key) ?? SCENARIOS[0];
}

const scenario = currentScenario();
let snapshot: DeviceList = structuredClone(scenario.deviceList);
let frameLog: string | null = null;
const listeners = new Set<(event: { payload: unknown }) => void>();

function emit() {
  const payload = structuredClone(snapshot);
  listeners.forEach((listener) => listener({ payload }));
}

/** Mutates the Device after the delay, or doesn't, per the scenario. */
function command(address: string, change: (device: DeviceList["devices"][number]) => void) {
  return new Promise<void>((resolve, reject) => {
    setTimeout(() => {
      if (scenario.behaviour === "reject") return reject("notAttached");
      resolve();
      if (scenario.behaviour === "neverConfirm") return;
      const device = snapshot.devices.find((d) => d.address === address);
      if (!device) return;
      change(device);
      emit();
    }, DELAY_MS);
  });
}

export const fake = {
  listen(event: string, handler: (event: { payload: unknown }) => void) {
    if (event === "device-list") listeners.add(handler);
    return () => listeners.delete(handler);
  },

  async invoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
    switch (cmd) {
      case "labels":
        return LABELS;
      case "device_list":
        return structuredClone(snapshot);
      case "set_noise_control_mode": {
        const mode = args.mode as NoiseControlMode;
        return command(args.address as string, (device) => {
          device.chosenMode = mode;
          // A bud out of the ear keeps the Effective Mode off.
          if (!device.effectiveModeReason) device.effectiveMode = mode;
        });
      }
      case "set_ambient_level":
        return command(args.address as string, (device) => {
          device.ambientLevel = args.level as number;
        });
      case "retry_attach": {
        const address = args.address as string;
        const device = snapshot.devices.find((d) => d.address === address);
        if (!device) throw "unknownDevice";
        const original = SCENARIOS.flatMap((s) => s.deviceList.devices).find(
          (d) => d.model === device.model && d.state.kind === "attached",
        );
        setTimeout(() => {
          device.state = { kind: "attaching" };
          emit();
        }, DELAY_MS);
        setTimeout(() => {
          if (original) Object.assign(device, structuredClone(original), { name: device.name });
          snapshot.primary ??= device.address;
          emit();
        }, DELAY_MS + 1800);
        return undefined;
      }
      case "frame_log":
        return frameLog;
      case "set_frame_log":
        frameLog = args.enabled ? "/tmp/PROTOTYPE-headphone-bridge-frames.log" : null;
        return frameLog;
      default:
        throw `prototype stub: no fake for ${cmd}`;
    }
  },
};
