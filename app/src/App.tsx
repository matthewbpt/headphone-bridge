import { useEffect, useState } from "react";
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

export function App() {
  return (
    <>
      <DeviceListView />
      <LaunchAtLoginSetting />
      <FrameLogSetting />
    </>
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
  if (deviceList.devices.length === 0) return <p>{labels.noDevice}</p>;
  const { devices } = deviceList;
  const selected =
    devices.find((device) => device.address === picked) ??
    devices.find((device) => device.address === primary) ??
    devices[0];
  return (
    <>
      <div role="tablist" aria-label="Devices">
        {devices.map((device) => (
          <button
            key={device.address}
            role="tab"
            aria-selected={device === selected}
            onClick={() => setPicked(device.address)}
            style={{ fontWeight: device === selected ? "bold" : "normal" }}
          >
            {device.name}
            {device.address === primary && " (Primary)"}
          </button>
        ))}
      </div>
      <div role="tabpanel">
        {/* Keyed, so pending state doesn't carry over to another Device. */}
        <DeviceView key={selected.address} device={selected} labels={labels} />
      </div>
    </>
  );
}

function DeviceView({ device, labels }: { device: Device; labels: Labels }) {
  return (
    <>
      <strong>{device.name}</strong>
      {device.model && <div>{labels.models[device.model]}</div>}
      <div>{stateText(device)}</div>
      {device.state.kind === "notAttached" && (
        <div>{reasonText(labels, device.state.reason)}</div>
      )}
      {device.state.kind === "notAttached" && device.state.reason.kind !== "unsupportedModel" && (
        <RetryButton address={device.address} />
      )}
      {device.state.kind === "attached" && device.capabilities.noiseControlModes.length > 0 && (
        <NoiseControlModeButtons device={device} labels={labels} />
      )}
      {device.state.kind === "attached" && device.capabilities.ambientLevel && (
        <AmbientLevelSlider device={device} range={device.capabilities.ambientLevel} />
      )}
      {device.state.kind === "attached" && device.effectiveMode && (
        <div>Applying: {labels.modes[device.effectiveMode]}</div>
      )}
      {device.battery && <BatteryView battery={device.battery} />}
    </>
  );
}

/** How long a pressed button stays pending if the Device never confirms. */
const PENDING_TIMEOUT_MS = 10_000;

function NoiseControlModeButtons({ device, labels }: { device: Device; labels: Labels }) {
  // The mode asked for, until the next snapshot moves the Chosen Mode.
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

  return (
    <div>
      <div role="radiogroup" aria-label="Noise Control Mode">
        {device.capabilities.noiseControlModes.map((mode) => (
          <button
            key={mode}
            role="radio"
            aria-checked={device.chosenMode === mode}
            aria-busy={pending === mode}
            disabled={pending !== null || device.chosenMode === mode}
            onClick={() => choose(mode)}
            style={{ fontWeight: device.chosenMode === mode ? "bold" : "normal" }}
          >
            {labels.modes[mode]}
            {pending === mode && " …"}
          </button>
        ))}
      </div>
      {device.effectiveModeReason && <div>{labels.hints[device.effectiveModeReason]}</div>}
      {error && <div role="alert">{error}</div>}
    </div>
  );
}

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

  const choose = (level: number) => {
    setError(null);
    setPending(level === device.ambientLevel ? null : level);
    if (level === device.ambientLevel && pending === null) return;
    setAmbientLevel(device.address, level).catch((e) => {
      setPending(null);
      setError(`Couldn't set Ambient Level ${level}: ${e}`);
    });
  };

  const shown = pending ?? device.ambientLevel;
  const listId = `ambient-level-steps-${device.address}`;
  return (
    <div>
      <label>
        Ambient Level{" "}
        {/* One notch per step of the Model's own range. */}
        <input
          type="range"
          min={range.min}
          max={range.max}
          step={1}
          list={listId}
          value={shown ?? range.min}
          disabled={shown === null}
          aria-busy={pending !== null}
          onChange={(e) => choose(Number(e.target.value))}
        />
        <datalist id={listId}>
          {Array.from({ length: range.max - range.min + 1 }, (_, i) => (
            <option key={i} value={range.min + i} />
          ))}
        </datalist>{" "}
        {shown ?? "–"}
        {pending !== null && " …"}
      </label>
      {error && <div role="alert">{error}</div>}
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
