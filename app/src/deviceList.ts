// Mirrors the core's Device list snapshot (crates/core/src/device_list.rs).
export interface DeviceList {
  devices: Device[];
  /**
   * The Primary Device's address: the Connected Device the OS plays audio
   * through, else the most recently Attached or shown, whether or not it's
   * Attached. Null while no supported Device is Connected.
   */
  primary: string | null;
}

export interface Device {
  address: string;
  /** The name the OS reports. */
  name: string;
  /**
   * Null until identified, and for unsupported Models. Kept, with the
   * Capabilities, while the Device is Attached again in the same Connected spell.
   */
  model: Model | null;
  state: DeviceState;
  capabilities: Capabilities;
  /** The Noise Control Mode last selected by any means. Null until reported. */
  chosenMode: NoiseControlMode | null;
  /** The Noise Control Mode the Device is applying right now. Null until reported. */
  effectiveMode: NoiseControlMode | null;
  /** Why the Effective Mode differs from the Chosen Mode. Null while they agree. */
  effectiveModeReason: EffectiveModeReason | null;
  /** In the Model's own steps, within `capabilities.ambientLevel`. Null until reported. */
  ambientLevel: number | null;
  /** In the Model's own steps, within `capabilities.noiseCancellingLevel`. Null until reported. */
  noiseCancellingLevel: number | null;
  /** Null until the Device first reports it. */
  battery: Battery | null;
  /**
   * The Device acknowledged the last Power Off but was still Connected well
   * after. Cleared when it is asked to Power Off again.
   */
  powerOffFailed: boolean;
  /**
   * When a Detached Device last reported the values it keeps, in milliseconds
   * since the Unix epoch: they may be out of date. Null while they're current,
   * or there are none.
   */
  reportedAt: number | null;
}

export type Model = "galaxyBuds4Pro" | "galaxyBuds2" | "wh1000xm5";

export type DeviceState =
  | { kind: "attaching" }
  | { kind: "attached" }
  /** Asked to Power Off and still Connected: every command is refused. */
  | { kind: "poweringOff" }
  /** The app chose not to Attach it, so another app or device can. */
  | { kind: "detached" }
  /** The app can't Attach it. */
  | { kind: "notAttached"; reason: NotAttachedReason };

export type NotAttachedReason =
  | { kind: "unsupportedModel" }
  | { kind: "inUseByAnotherProcess" }
  | { kind: "bluetoothPermissionDenied" }
  | { kind: "serviceNotFound" }
  | { kind: "deviceNotConnected" }
  | { kind: "linkDropped" }
  | { kind: "noResponse" }
  | { kind: "other"; message: string };

export type NoiseControlMode = "off" | "noiseCancelling" | "ambient" | "adaptive";

export type EffectiveModeReason = "budOutOfEar" | "unknown";

export interface Capabilities {
  noiseControlModes: NoiseControlMode[];
  ambientLevel: LevelRange | null;
  noiseCancellingLevel: LevelRange | null;
  /** Whether the app can Power Off the Device. */
  powerOff: boolean;
}

/** A level Capability's steps in the Model's own units, both ends included. */
export interface LevelRange {
  min: number;
  max: number;
}

/** Percentages; for earbuds, null where the level is unknown. */
export type Battery =
  | { kind: "buds"; left: number | null; right: number | null; case: number | null }
  | { kind: "headphones"; level: number };

/**
 * The labels for a Device's state and settings, shared with the tray menu
 * (app/src-tauri/src/labels.rs), so the two can't drift apart.
 */
export interface Labels {
  noDevice: string;
  modes: Record<NoiseControlMode, string>;
  models: Record<Model, string>;
  hints: Record<EffectiveModeReason, string>;
  /** Every reason but "other", whose label is its own message. */
  reasons: { reason: NotAttachedReason; text: string }[];
}

/** Why a Device isn't Attached, labelled as the tray labels it. */
export function reasonText(labels: Labels, reason: NotAttachedReason): string {
  if (reason.kind === "other") return reason.message;
  return labels.reasons.find((label) => label.reason.kind === reason.kind)?.text ?? reason.kind;
}
