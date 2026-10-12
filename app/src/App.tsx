import { type CSSProperties, useEffect, useState } from "react";
import type { Battery, Device, Labels, LevelRange, NoiseControlMode } from "./deviceList";
import {
  reasonText,
  retryAttach,
  setAmbientLevel,
  setNoiseCancellingLevel,
  setNoiseControlMode,
  useDeviceList,
  useLabels,
} from "./deviceList";
import { BatteryGlyph, DevicePicture, ModeIcon } from "./deviceArt";
import { FrameLogSetting } from "./FrameLogSetting";
import { LaunchAtLoginSetting } from "./LaunchAtLoginSetting";
import { usePendingCommand } from "./usePendingCommand";

export function App() {
  return (
    <main className="window">
      <DeviceListView />
      <details className="card settings">
        <summary>Settings</summary>
        <LaunchAtLoginSetting />
        <FrameLogSetting />
      </details>
    </main>
  );
}

function DeviceListView() {
  const deviceList = useDeviceList();
  const labels = useLabels();
  // The Device picked in the window. Cleared whenever the Primary Device
  // changes, so the selection follows the OS's audio output.
  const [picked, setPicked] = useState<string | null>(null);
  const primary = deviceList?.primary ?? null;
  useEffect(() => setPicked(null), [primary]);

  if (deviceList === null || labels === null) return null;
  if (deviceList.devices.length === 0) return <p className="empty">{labels.noDevice}</p>;
  const { devices } = deviceList;
  const selected =
    devices.find((device) => device.address === picked) ??
    devices.find((device) => device.address === primary) ??
    devices[0];
  return (
    <>
      <Header device={selected} labels={labels} />
      {devices.length > 1 && (
        <DevicePills devices={devices} selected={selected} primary={primary} onPick={setPicked} />
      )}
      {/* Keyed, so pending state doesn't carry over to another Device. */}
      <DeviceView key={selected.address} device={selected} labels={labels} />
    </>
  );
}

/** A pill per Connected Device, to pick the one the window shows. */
function DevicePills({
  devices,
  selected,
  primary,
  onPick,
}: {
  devices: Device[];
  selected: Device;
  primary: string | null;
  onPick: (address: string) => void;
}) {
  return (
    <div className="pills" role="tablist" aria-label="Devices">
      {devices.map((device) => {
        const isPrimary = device.address === primary;
        return (
          <button
            key={device.address}
            className="pill"
            role="tab"
            aria-selected={device === selected}
            onClick={() => onPick(device.address)}
            title={isPrimary ? "Primary Device" : undefined}
          >
            {isPrimary && <span className="pill__primary" aria-hidden />}
            {device.name}
            {isPrimary && <span className="visually-hidden"> (Primary)</span>}
          </button>
        );
      })}
    </div>
  );
}

/** The Device's OS name, and its Model when that says something the name doesn't. */
function Header({ device, labels }: { device: Device; labels: Labels }) {
  const model = device.model && labels.models[device.model];
  return (
    <header className="header">
      <h1 className="title">{device.name}</h1>
      {model && model !== device.name && <div className="subtitle">{model}</div>}
    </header>
  );
}

function DeviceView({ device, labels }: { device: Device; labels: Labels }) {
  // The mode asked for, until the next snapshot moves the Chosen Mode. Held
  // here rather than in the selector, so the rest of the Device view can
  // treat a pending mode as Chosen.
  const mode = usePendingCommand((mode: NoiseControlMode) => labels.modes[mode]);
  const { clear: clearMode } = mode;
  useEffect(clearMode, [device.chosenMode, clearMode]);

  const chooseMode = (chosen: NoiseControlMode) =>
    mode.send(chosen, () => setNoiseControlMode(device.address, chosen));

  const attached = device.state.kind === "attached";
  // A pending mode counts as Chosen, so the slider comes and goes the
  // moment a mode is clicked, and goes back if the Device refuses.
  const shownMode = mode.pending ?? device.chosenMode;
  const ambientLevel = device.capabilities.ambientLevel;
  const noiseCancellingLevel = device.capabilities.noiseCancellingLevel;
  return (
    <>
      <section className="hero">
        <DevicePicture model={device.model} labels={labels} />
        <Status device={device} labels={labels} />
      </section>
      {attached && (
        <section className="card">
          {device.capabilities.noiseControlModes.length > 0 && (
            <>
              <NoiseControlModeSelector
                supported={device.capabilities.noiseControlModes}
                chosen={device.chosenMode}
                pending={mode.pending}
                shown={shownMode}
                labels={labels}
                onChoose={chooseMode}
              />
              {device.effectiveModeReason && (
                <p className="hint">{labels.hints[device.effectiveModeReason]}</p>
              )}
              {mode.error && (
                <p className="error" role="alert">
                  {mode.error}
                </p>
              )}
            </>
          )}
          {ambientLevel && shownMode === "ambient" && (
            <LevelSlider
              title="Ambient Level"
              level={device.ambientLevel}
              range={ambientLevel}
              set={(level) => setAmbientLevel(device.address, level)}
            />
          )}
          {noiseCancellingLevel && shownMode === "noiseCancelling" && (
            <LevelSlider
              title="Noise Cancelling Level"
              level={device.noiseCancellingLevel}
              range={noiseCancellingLevel}
              set={(level) => setNoiseCancellingLevel(device.address, level)}
            />
          )}
        </section>
      )}
    </>
  );
}

/**
 * The window's mode order, from hearing everything to hearing nothing. The
 * core's Capability order is its own; the window shows this one.
 */
const MODE_ORDER: NoiseControlMode[] = ["off", "ambient", "adaptive", "noiseCancelling"];

/** Under the picture: the battery once Attached, else why the controls aren't there. */
function Status({ device, labels }: { device: Device; labels: Labels }) {
  switch (device.state.kind) {
    case "attached":
      return device.battery ? (
        <BatteryView battery={device.battery} />
      ) : (
        <div className="status muted">–</div>
      );
    case "attaching":
      return (
        <div className="status muted">
          <span className="spinner" aria-hidden /> Attaching…
        </div>
      );
    case "notAttached":
      return (
        <div className="status status--column">
          <span className="muted">{reasonText(labels, device.state.reason)}</span>
          {device.state.reason.kind !== "unsupportedModel" && (
            <RetryButton address={device.address} />
          )}
        </div>
      );
  }
}

/**
 * Round icon buttons joined by a track, after the Galaxy Buds app. The
 * Chosen Mode is filled; a pending mode takes the fill, dimmed and ringed,
 * until the Device confirms it.
 */
function NoiseControlModeSelector({
  supported,
  chosen,
  pending,
  shown,
  labels,
  onChoose,
}: {
  supported: NoiseControlMode[];
  chosen: NoiseControlMode | null;
  pending: NoiseControlMode | null;
  /** The pending mode, else the Chosen Mode: the one filled. */
  shown: NoiseControlMode | null;
  labels: Labels;
  onChoose: (mode: NoiseControlMode) => void;
}) {
  const modes = MODE_ORDER.filter((mode) => supported.includes(mode));
  return (
    <div
      className="modes"
      role="radiogroup"
      aria-label="Noise Control Mode"
      style={{ "--count": modes.length } as CSSProperties}
    >
      <div className="modes__track" aria-hidden />
      {modes.map((mode) => (
        <button
          key={mode}
          className="mode"
          role="radio"
          data-filled={shown === mode}
          aria-checked={chosen === mode}
          aria-busy={pending === mode}
          disabled={pending !== null || chosen === mode}
          onClick={() => onChoose(mode)}
        >
          <span className="mode__circle">
            <ModeIcon mode={mode} />
          </span>
          <span className="mode__label">{labels.modes[mode]}</span>
        </button>
      ))}
    </div>
  );
}

/** Above this many steps, notches crowd the track; it shows the value instead. */
const MAX_NOTCHES = 10;

/**
 * A level, in the Model's own steps. A level set here shows a dashed thumb
 * until the Device reports it.
 */
function LevelSlider({
  title,
  level,
  range,
  set,
}: {
  title: string;
  /** The reported level; null until the Device first reports it. */
  level: number | null;
  range: LevelRange;
  /** Asks the Device for a level; without it the slider only shows the level. */
  set?: (level: number) => Promise<void>;
}) {
  // The levels asked for, until snapshots report them. Dragging across
  // notches asks for each in turn. Only a change of level confirms one: a
  // snapshot still reporting the old level says nothing about those asked.
  const { pending, error, send, confirm } = usePendingCommand(
    (asked: number) => `${title} ${asked}`,
  );
  useEffect(() => confirm(level), [level, confirm]);

  const shown = pending ?? level;
  const choose = (asked: number) => {
    asked = Math.min(range.max, Math.max(range.min, asked));
    if (asked === shown || !set) return;
    send(asked, () => set(asked));
  };
  // Nothing to act on until the Device reports a level, or with no way to set one.
  const disabled = shown === null || !set;

  const steps = range.max - range.min + 1;
  const notched = steps <= MAX_NOTCHES;
  return (
    <div className="level">
      <div className="level__title">{title}</div>
      <div className="level__row">
        <button
          className="level__step"
          aria-label="Less"
          disabled={disabled || shown <= range.min}
          onClick={() => shown !== null && choose(shown - 1)}
        >
          <svg viewBox="0 0 16 16" aria-hidden>
            <path d="M3 8H13" />
          </svg>
        </button>
        <div className="level__track" data-pending={pending !== null}>
          {notched && (
            <div className="level__notches" aria-hidden>
              {Array.from({ length: steps }, (_, i) => (
                <span key={i} className="level__notch" data-testid="notch" />
              ))}
            </div>
          )}
          <input
            type="range"
            min={range.min}
            max={range.max}
            step={1}
            value={shown ?? range.min}
            disabled={disabled}
            aria-label={title}
            aria-busy={pending !== null}
            onChange={(e) => choose(Number(e.target.value))}
          />
        </div>
        <button
          className="level__step"
          aria-label="More"
          disabled={disabled || shown >= range.max}
          onClick={() => shown !== null && choose(shown + 1)}
        >
          <svg viewBox="0 0 16 16" aria-hidden>
            <path d="M3 8H13M8 3V13" />
          </svg>
        </button>
        {!notched && <output className="level__value">{shown ?? "–"}</output>}
      </div>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

function RetryButton({ address }: { address: string }) {
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <button
        className="button"
        onClick={() => {
          setError(null);
          retryAttach(address).catch((e) => setError(`Couldn't retry: ${e}`));
        }}
      >
        Retry
      </button>
      {error && (
        <span className="error" role="alert">
          {error}
        </span>
      )}
    </>
  );
}

/** For earbuds, L and R merge while they're known and equal; Case shows only while reported. */
function BatteryView({ battery }: { battery: Battery }) {
  const values: { label?: string; level: number | null }[] =
    battery.kind === "headphones"
      ? [{ level: battery.level }]
      : battery.left !== null && battery.left === battery.right
        ? [{ label: "L·R", level: battery.left }]
        : [
            { label: "L", level: battery.left },
            { label: "R", level: battery.right },
          ];
  if (battery.kind === "buds" && battery.case !== null) values.push({ label: "Case", level: battery.case });
  return (
    <div className="status battery" role="group" aria-label="Battery">
      {values.map(({ label, level }, i) => (
        <span key={label ?? "level"} className="battery__value">
          {i > 0 && <span className="battery__separator"> · </span>}
          <BatteryGlyph level={level} />
          {label && <span className="battery__label">{label} </span>}
          {level === null ? "–" : `${level}%`}
        </span>
      ))}
    </div>
  );
}
