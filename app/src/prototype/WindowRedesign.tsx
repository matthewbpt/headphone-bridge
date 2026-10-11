// PROTOTYPE: the redesigned window, after the Galaxy Buds app. The layout is
// settled (PROTOTYPE-BRIEF.md); the variants only differ in image style and
// Device switcher, passed in as props. Rewrite properly when folding in.
import { useEffect, useRef, useState } from "react";
import type { Battery, Device, Labels, NoiseControlMode } from "../deviceList";
import {
  reasonText,
  retryAttach,
  setAmbientLevel,
  setNoiseControlMode,
  useDeviceList,
  useLabels,
} from "../deviceList";
import { FrameLogSetting } from "../FrameLogSetting";
import { LaunchAtLoginSetting } from "../LaunchAtLoginSetting";
import { type ArtStyle, BatteryGlyph, DeviceImage, ModeIcon } from "./art";
import "./redesign.css";

export type SwitcherStyle = "pills" | "dropdown";

export function WindowRedesign({ art, switcher }: { art: ArtStyle; switcher: SwitcherStyle }) {
  const deviceList = useDeviceList();
  const labels = useLabels();
  // The Device picked in the window. Cleared whenever the Primary Device
  // changes, so the selection follows the OS's audio output.
  const [picked, setPicked] = useState<string | null>(null);
  const primary = deviceList?.primary ?? null;
  useEffect(() => setPicked(null), [primary]);

  if (deviceList === null || labels === null) return null;
  const { devices } = deviceList;
  const selected =
    devices.find((device) => device.address === picked) ??
    devices.find((device) => device.address === primary) ??
    devices[0];

  return (
    <div className="window">
      {selected ? (
        <>
          <Header
            device={selected}
            devices={devices}
            primary={primary}
            labels={labels}
            dropdown={switcher === "dropdown" && devices.length > 1}
            onPick={setPicked}
          />
          {switcher === "pills" && devices.length > 1 && (
            <DevicePills devices={devices} selected={selected} primary={primary} onPick={setPicked} />
          )}
          {/* Keyed, so pending state doesn't carry over to another Device. */}
          <DeviceView key={selected.address} device={selected} labels={labels} art={art} />
        </>
      ) : (
        <div className="empty">
          <DeviceImage model={null} style={art} />
          <p>{labels.noDevice}</p>
        </div>
      )}
      <details className="card settings">
        <summary>Settings</summary>
        <LaunchAtLoginSetting />
        <FrameLogSetting />
      </details>
    </div>
  );
}

function Header({
  device,
  devices,
  primary,
  labels,
  dropdown,
  onPick,
}: {
  device: Device;
  devices: Device[];
  primary: string | null;
  labels: Labels;
  dropdown: boolean;
  onPick: (address: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);

  const model = device.model && labels.models[device.model];
  return (
    <header className="header" ref={ref}>
      {dropdown ? (
        <button className="title title--button" aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen(!open)}>
          <span>{device.name}</span>
          <svg className="chevron" viewBox="0 0 12 12" aria-hidden>
            <path d="M3 4.5 L6 7.5 L9 4.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          </svg>
        </button>
      ) : (
        <h1 className="title">{device.name}</h1>
      )}
      {model && model !== device.name && <div className="subtitle">{model}</div>}
      {open && (
        <ul className="menu" role="listbox" aria-label="Devices">
          {devices.map((d) => (
            <li
              key={d.address}
              role="option"
              aria-selected={d === device}
              onClick={() => {
                onPick(d.address);
                setOpen(false);
              }}
            >
              <span className="menu__check">{d === device ? "✓" : ""}</span>
              <span className="menu__name">{d.name}</span>
              {d.address === primary && <span className="tag">Primary</span>}
            </li>
          ))}
        </ul>
      )}
    </header>
  );
}

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
          role="tab"
          aria-selected={device === selected}
          className="pill"
          onClick={() => onPick(device.address)}
          title={device.address === primary ? "Primary Device" : undefined}
        >
          {device.address === primary && <span className="pill__primary" aria-label="Primary" />}
          {device.name}
        </button>
      ))}
    </div>
  );
}

/** How long a pressed button stays pending if the Device never confirms. */
const PENDING_TIMEOUT_MS = 10_000;

function DeviceView({ device, labels, art }: { device: Device; labels: Labels; art: ArtStyle }) {
  // The mode asked for, until the next snapshot moves the Chosen Mode. Lifted
  // here because a pending mode counts as Chosen for the slider's visibility.
  const [pending, setPending] = useState<NoiseControlMode | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => setPending(null), [device.chosenMode]);
  useEffect(() => {
    if (pending === null) return;
    const timer = setTimeout(() => {
      setPending(null);
      setError(`${labels.modes[pending]} wasn't confirmed by the Device`);
    }, PENDING_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [pending, labels]);

  const choose = (mode: NoiseControlMode) => {
    setError(null);
    setPending(mode);
    setNoiseControlMode(device.address, mode).catch((e) => {
      setPending(null);
      setError(`Couldn't set ${labels.modes[mode]}: ${e}`);
    });
  };

  const attached = device.state.kind === "attached";
  const shownMode = pending ?? device.chosenMode;
  const range = device.capabilities.ambientLevel;
  return (
    <>
      <section className="hero">
        <DeviceImage model={device.model} style={art} />
        <Status device={device} labels={labels} />
      </section>
      {attached && device.capabilities.noiseControlModes.length > 0 && (
        <section className="card">
          <ModeSelector
            modes={device.capabilities.noiseControlModes}
            shown={shownMode}
            pending={pending}
            labels={labels}
            onChoose={choose}
          />
          {device.effectiveModeReason && <p className="hint">{labels.hints[device.effectiveModeReason]}</p>}
          {error && (
            <p className="error" role="alert">
              {error}
            </p>
          )}
          {shownMode === "ambient" && range && <AmbientLevelSlider device={device} range={range} />}
        </section>
      )}
    </>
  );
}

function Status({ device, labels }: { device: Device; labels: Labels }) {
  const [error, setError] = useState<string | null>(null);
  switch (device.state.kind) {
    case "attached":
      return device.battery ? <BatteryView battery={device.battery} /> : <div className="status muted">–</div>;
    case "attaching":
      return (
        <div className="status muted">
          <span className="spinner" aria-hidden /> Attaching…
        </div>
      );
    case "notAttached":
      return (
        <div className="status status--column">
          <span className="reason">{reasonText(labels, device.state.reason)}</span>
          {device.state.reason.kind !== "unsupportedModel" && (
            <button
              className="button"
              onClick={() => {
                setError(null);
                retryAttach(device.address).catch((e) => setError(`Couldn't retry: ${e}`));
              }}
            >
              Retry
            </button>
          )}
          {error && (
            <span className="error" role="alert">
              {error}
            </span>
          )}
        </div>
      );
  }
}

function BatteryView({ battery }: { battery: Battery }) {
  const percent = (level: number | null) => (level === null ? "–" : `${level}%`);
  const items: { label?: string; level: number | null }[] =
    battery.kind === "headphones"
      ? [{ level: battery.level }]
      : battery.left !== null && battery.left === battery.right
        ? [{ label: "L·R", level: battery.left }]
        : [
            { label: "L", level: battery.left },
            { label: "R", level: battery.right },
          ];
  if (battery.kind === "buds" && battery.case !== null) items.push({ label: "Case", level: battery.case });
  return (
    <div className="status battery" aria-label="Battery">
      {items.map((item, i) => (
        <span key={i} className="battery__item">
          {i > 0 && <span className="battery__sep">·</span>}
          <BatteryGlyph level={item.level} />
          {item.label && <span className="battery__label">{item.label}</span>}
          <span>{percent(item.level)}</span>
        </span>
      ))}
    </div>
  );
}

function ModeSelector({
  modes,
  shown,
  pending,
  labels,
  onChoose,
}: {
  modes: NoiseControlMode[];
  shown: NoiseControlMode | null;
  pending: NoiseControlMode | null;
  labels: Labels;
  onChoose: (mode: NoiseControlMode) => void;
}) {
  return (
    <div className="modes" role="radiogroup" aria-label="Noise Control Mode" style={{ ["--count" as string]: modes.length }}>
      <div className="modes__track" aria-hidden />
      {modes.map((mode) => (
        <button
          key={mode}
          role="radio"
          className={`mode${pending === mode ? " mode--pending" : ""}`}
          aria-checked={shown === mode}
          aria-busy={pending === mode}
          disabled={pending !== null || shown === mode}
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

function AmbientLevelSlider({ device, range }: { device: Device; range: { min: number; max: number } }) {
  // The level last asked for, until a snapshot reports it.
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
  const disabled = shown === null || device.state.kind !== "attached";
  return (
    <div className="level">
      <div className="level__title">Ambient Level</div>
      <div className="level__row">
        <button className="level__step" aria-label="Less" disabled={disabled || shown === range.min} onClick={() => choose((shown ?? range.min) - 1)}>
          <svg viewBox="0 0 16 16" aria-hidden><path d="M3 8 H13" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" /></svg>
        </button>
        <div className={`level__track${pending !== null ? " level__track--pending" : ""}`}>
          <div className="level__dots" aria-hidden>
            {Array.from({ length: steps }, (_, i) => (
              <span key={i} className="level__dot" />
            ))}
          </div>
          <input
            type="range"
            aria-label="Ambient Level"
            min={range.min}
            max={range.max}
            step={1}
            value={shown ?? range.min}
            disabled={disabled}
            aria-busy={pending !== null}
            onChange={(e) => choose(Number(e.target.value))}
          />
        </div>
        <button className="level__step" aria-label="More" disabled={disabled || shown === range.max} onClick={() => choose((shown ?? range.min) + 1)}>
          <svg viewBox="0 0 16 16" aria-hidden><path d="M3 8 H13 M8 3 V13" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" /></svg>
        </button>
      </div>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
