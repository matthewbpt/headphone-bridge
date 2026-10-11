// PROTOTYPE: made-up Device list snapshots, one per scenario in
// PROTOTYPE-BRIEF.md. Picked with `?scenario=`. Throw away.
import type { Capabilities, Device, DeviceList } from "../deviceList";

/** How the fake Device answers a setter. */
export type Behaviour = "confirm" | "neverConfirm" | "reject";

export interface Scenario {
  key: string;
  name: string;
  /** What to do to see the point of the scenario. */
  tip?: string;
  behaviour: Behaviour;
  deviceList: DeviceList;
}

const buds4ProCapabilities: Capabilities = {
  noiseControlModes: ["off", "noiseCancelling", "ambient", "adaptive"],
  ambientLevel: { min: 0, max: 4 },
};
const xm5Capabilities: Capabilities = {
  noiseControlModes: ["off", "noiseCancelling", "ambient"],
  ambientLevel: { min: 1, max: 20 },
};
const noCapabilities: Capabilities = { noiseControlModes: [], ambientLevel: null };

const BUDS_ADDRESS = "AA:BB:CC:00:00:01";
const XM5_ADDRESS = "AA:BB:CC:00:00:02";

function buds(overrides: Partial<Device> = {}): Device {
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
    battery: { kind: "buds", left: 100, right: 100, case: null },
    ...overrides,
  };
}

function xm5(overrides: Partial<Device> = {}): Device {
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
    battery: { kind: "headphones", level: 70 },
    ...overrides,
  };
}

const one = (device: Device): DeviceList => ({
  devices: [device],
  primary: device.state.kind === "attached" ? device.address : null,
});

export const SCENARIOS: Scenario[] = [
  {
    key: "buds-ambient",
    name: "Buds4 Pro · Ambient",
    tip: "Ambient Level slider shows. Pick another mode: the slider hides at once.",
    behaviour: "confirm",
    deviceList: one(buds()),
  },
  {
    key: "buds-nc",
    name: "Buds4 Pro · Noise Cancelling",
    tip: "Uneven buds, case reported. Pick Ambient: the slider shows while it's pending.",
    behaviour: "confirm",
    deviceList: one(
      buds({
        chosenMode: "noiseCancelling",
        effectiveMode: "noiseCancelling",
        battery: { kind: "buds", left: 82, right: 64, case: 45 },
      }),
    ),
  },
  {
    key: "pending",
    name: "Mode pending",
    tip: "This Device never confirms. Click a mode: it stays pending, then reverts after 10 s.",
    behaviour: "neverConfirm",
    deviceList: one(buds({ chosenMode: "noiseCancelling", effectiveMode: "noiseCancelling" })),
  },
  {
    key: "rejected",
    name: "Mode rejected",
    tip: "This Device rejects every command. Click Ambient: the slider shows, then reverts.",
    behaviour: "reject",
    deviceList: one(buds({ chosenMode: "noiseCancelling", effectiveMode: "noiseCancelling" })),
  },
  {
    key: "hint",
    name: "Effective Mode hint",
    tip: "Right bud out of ear: Noise Cancelling chosen, Off effective.",
    behaviour: "confirm",
    deviceList: one(
      buds({
        chosenMode: "noiseCancelling",
        effectiveMode: "off",
        effectiveModeReason: "budOutOfEar",
        battery: { kind: "buds", left: 58, right: 61, case: 90 },
      }),
    ),
  },
  {
    key: "xm5",
    name: "WH-1000XM5 · 3 modes",
    tip: "Headphones battery. Pick Ambient to see a 20-step Ambient Level.",
    behaviour: "confirm",
    deviceList: one(xm5()),
  },
  {
    key: "attaching",
    name: "Attaching…",
    behaviour: "confirm",
    deviceList: {
      devices: [
        buds({
          state: { kind: "attaching" },
          capabilities: noCapabilities,
          chosenMode: null,
          effectiveMode: null,
          ambientLevel: null,
          battery: null,
        }),
      ],
      primary: null,
    },
  },
  {
    key: "not-attached",
    name: "Not Attached + Retry",
    tip: "Retry goes Attaching… then Attached.",
    behaviour: "confirm",
    deviceList: {
      devices: [
        xm5({
          state: { kind: "notAttached", reason: { kind: "inUseByAnotherProcess" } },
          chosenMode: null,
          effectiveMode: null,
          ambientLevel: null,
          battery: null,
        }),
      ],
      primary: null,
    },
  },
  {
    key: "unsupported",
    name: "Unsupported Model",
    tip: "Generic glyph, no Retry.",
    behaviour: "confirm",
    deviceList: {
      devices: [
        {
          address: "AA:BB:CC:00:00:03",
          name: "AirPods Pro",
          model: null,
          state: { kind: "notAttached", reason: { kind: "unsupportedModel" } },
          capabilities: noCapabilities,
          chosenMode: null,
          effectiveMode: null,
          effectiveModeReason: null,
          ambientLevel: null,
          battery: null,
        },
      ],
      primary: null,
    },
  },
  {
    key: "two-devices",
    name: "Two Devices Connected",
    tip: "Switcher shows. Buds are Primary; XM5 has its own state.",
    behaviour: "confirm",
    deviceList: {
      devices: [
        buds({ battery: { kind: "buds", left: 47, right: 52, case: 80 } }),
        xm5({ name: "Matthew's XM5" }),
      ],
      primary: BUDS_ADDRESS,
    },
  },
];
