import { type CSSProperties, useEffect, useState } from "react";
import type { Battery, Device, Labels, NoiseControlMode } from "./deviceList";
import {
  reasonText,
  retryAttach,
  setAmbientLevel,
  setNoiseControlMode,
  useDeviceList,
  useLabels,
} from "./deviceList";
import { FrameLogSetting } from "./FrameLogSetting";
import { LaunchAtLoginSetting } from "./LaunchAtLoginSetting";
import { ModeIcon } from "./ModeIcon";

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
      <section className="card">
        {/* Keyed, so pending state doesn't carry over to another Device. */}
        <DeviceView key={selected.address} device={selected} labels={labels} />
      </section>
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
      {devices.map((device) => (
        <button
          key={device.address}
          className="pill"
          role="tab"
          aria-selected={device === selected}
          onClick={() => onPick(device.address)}
          title={device.address === primary ? "Primary Device" : undefined}
        >
          {device.address === primary && <span className="pill__primary" aria-hidden />}
          {device.name}
          {device.address === primary && <span className="visually-hidden"> (Primary)</span>}
        </button>
      ))}
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
  const [pendingMode, setPendingMode] = useState<NoiseControlMode | null>(null);
  const [modeError, setModeError] = useState<string | null>(null);

  useEffect(() => setPendingMode(null), [device.chosenMode]);
  useEffect(() => {
    if (pendingMode === null) return;
    const timer = setTimeout(() => {
      setPendingMode(null);
      setModeError(`${labels.modes[pendingMode]} wasn't confirmed by the Device`);
    }, PENDING_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [pendingMode, labels]);

  const chooseMode = (mode: NoiseControlMode) => {
    setModeError(null);
    setPendingMode(mode);
    setNoiseControlMode(device.address, mode).catch((e) => {
      setPendingMode(null);
      setModeError(`Couldn't set ${labels.modes[mode]}: ${e}`);
    });
  };

  const attached = device.state.kind === "attached";
  // A pending mode counts as Chosen, so the slider comes and goes the
  // moment a mode is clicked, and goes back if the Device refuses.
  const shownMode = pendingMode ?? device.chosenMode;
  const ambientLevel = device.capabilities.ambientLevel;
  return (
    <>
      <div>{stateText(device)}</div>
      {device.state.kind === "notAttached" && (
        <div>{reasonText(labels, device.state.reason)}</div>
      )}
      {device.state.kind === "notAttached" && device.state.reason.kind !== "unsupportedModel" && (
        <RetryButton address={device.address} />
      )}
      {attached && device.capabilities.noiseControlModes.length > 0 && (
        <>
          <NoiseControlModeSelector
            device={device}
            pending={pendingMode}
            labels={labels}
            onChoose={chooseMode}
          />
          {device.effectiveModeReason && (
            <p className="hint">{labels.hints[device.effectiveModeReason]}</p>
          )}
          {modeError && (
            <p className="error" role="alert">
              {modeError}
            </p>
          )}
        </>
      )}
      {attached && ambientLevel && shownMode === "ambient" && (
        <AmbientLevelSlider device={device} range={ambientLevel} />
      )}
      {device.battery && <BatteryView battery={device.battery} />}
    </>
  );
}

/**
 * The window's mode order, from hearing everything to hearing nothing. The
 * core's Capability order is its own; the window shows this one.
 */
const MODE_ORDER: NoiseControlMode[] = ["off", "ambient", "adaptive", "noiseCancelling"];

/** How long a pressed button stays pending if the Device never confirms. */
const PENDING_TIMEOUT_MS = 10_000;

/**
 * Round icon buttons joined by a track, after the Galaxy Buds app. The
 * Chosen Mode is filled; a pending mode takes the fill, dimmed and ringed,
 * until the Device confirms it.
 */
function NoiseControlModeSelector({
  device,
  pending,
  labels,
  onChoose,
}: {
  device: Device;
  pending: NoiseControlMode | null;
  labels: Labels;
  onChoose: (mode: NoiseControlMode) => void;
}) {
  const modes = MODE_ORDER.filter((mode) => device.capabilities.noiseControlModes.includes(mode));
  const filled = pending ?? device.chosenMode;
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
          data-filled={filled === mode}
          aria-checked={device.chosenMode === mode}
          aria-busy={pending === mode}
          disabled={pending !== null || device.chosenMode === mode}
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
 * The Ambient Level, in the Model's own steps. A level set here shows a
 * dashed thumb until the Device reports it.
 */
function AmbientLevelSlider({
  device,
  range,
}: {
  device: Device;
  range: { min: number; max: number };
}) {
  // The level last asked for, until a snapshot reports it. Dragging across
  // notches asks for each in turn.
  const [pending, setPending] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setPending((level) => (level === device.ambientLevel ? null : level));
  }, [device.ambientLevel]);
  useEffect(() => {
    if (pending === null) return;
    const timer = setTimeout(() => {
      setPending(null);
      setError(`Ambient Level ${pending} wasn't confirmed by the Device`);
    }, PENDING_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [pending]);

  const shown = pending ?? device.ambientLevel;
  const choose = (level: number) => {
    level = Math.min(range.max, Math.max(range.min, level));
    if (level === shown) return;
    setError(null);
    setPending(level === device.ambientLevel ? null : level);
    setAmbientLevel(device.address, level).catch((e) => {
      setPending(null);
      setError(`Couldn't set Ambient Level ${level}: ${e}`);
    });
  };

  const steps = range.max - range.min + 1;
  const notched = steps <= MAX_NOTCHES;
  return (
    <div className="level">
      <div className="level__title">Ambient Level</div>
      <div className="level__row">
        <button
          className="level__step"
          aria-label="Less"
          disabled={shown === null || shown <= range.min}
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
            disabled={shown === null}
            aria-label="Ambient Level"
            aria-busy={pending !== null}
            onChange={(e) => choose(Number(e.target.value))}
          />
        </div>
        <button
          className="level__step"
          aria-label="More"
          disabled={shown === null || shown >= range.max}
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

function stateText(device: Device): string {
  switch (device.state.kind) {
    case "attaching":
      return "Connected, Attaching…";
    case "attached":
      return "Attached";
    case "notAttached":
      return "Connected, not Attached";
  }
}


function RetryButton({ address }: { address: string }) {
  const [error, setError] = useState<string | null>(null);
  return (
    <div>
      <button
        onClick={() => {
          setError(null);
          retryAttach(address).catch((e) => setError(`Couldn't retry: ${e}`));
        }}
      >
        Retry
      </button>
      {error && <div role="alert">{error}</div>}
    </div>
  );
}

function BatteryView({ battery }: { battery: Battery }) {
  const percent = (level: number | null) => (level === null ? "–" : `${level}%`);
  if (battery.kind === "headphones") return <div>Battery: {percent(battery.level)}</div>;
  return (
    <div>
      Battery: L {percent(battery.left)} · R {percent(battery.right)} · Case {percent(battery.case)}
    </div>
  );
}
