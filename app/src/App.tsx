import { useEffect, useState } from "react";
import type { Battery, Device, Model, NoiseControlMode, NotAttachedReason } from "./deviceList";
import { retryAttach, setAmbientLevel, setNoiseControlMode, useDeviceList } from "./deviceList";
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

  if (deviceList === null) return null;
  if (deviceList.devices.length === 0) return <p>No device</p>;
  return (
    <ul>
      {deviceList.devices.map((device) => (
        <li key={device.address}>
          <DeviceView device={device} />
        </li>
      ))}
    </ul>
  );
}

const MODEL_NAMES: Record<Model, string> = {
  galaxyBuds4Pro: "Galaxy Buds4 Pro",
  wh1000xm5: "WH-1000XM5",
};

function DeviceView({ device }: { device: Device }) {
  return (
    <>
      <strong>{device.name}</strong>
      {device.model && <div>{MODEL_NAMES[device.model]}</div>}
      <div>{stateText(device)}</div>
      {device.state.kind === "notAttached" && device.state.reason.kind !== "unsupportedModel" && (
        <RetryButton address={device.address} />
      )}
      {device.state.kind === "attached" && device.capabilities.noiseControlModes.length > 0 && (
        <NoiseControlModeButtons device={device} />
      )}
      {device.state.kind === "attached" && device.capabilities.ambientLevel && (
        <AmbientLevelSlider device={device} range={device.capabilities.ambientLevel} />
      )}
      {device.battery && <BatteryView battery={device.battery} />}
    </>
  );
}

const MODE_NAMES: Record<NoiseControlMode, string> = {
  off: "Off",
  noiseCancelling: "Noise Cancelling",
  ambient: "Ambient",
  adaptive: "Adaptive",
};

/** How long a pressed button stays pending if the Device never confirms. */
const PENDING_TIMEOUT_MS = 10_000;

function NoiseControlModeButtons({ device }: { device: Device }) {
  // The mode asked for, until the next snapshot moves the Chosen Mode.
  const [pending, setPending] = useState<NoiseControlMode | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => setPending(null), [device.chosenMode]);
  useEffect(() => {
    if (pending === null) return;
    const timer = setTimeout(() => {
      setPending(null);
      setError(`${MODE_NAMES[pending]} wasn't confirmed by the device`);
    }, PENDING_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [pending]);

  const choose = (mode: NoiseControlMode) => {
    setError(null);
    setPending(mode);
    setNoiseControlMode(device.address, mode).catch((e) => {
      setPending(null);
      setError(`Couldn't set ${MODE_NAMES[mode]}: ${e}`);
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
            {MODE_NAMES[mode]}
            {pending === mode && " …"}
          </button>
        ))}
      </div>
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
      setError(`Ambient Level ${pending} wasn't confirmed by the device`);
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
      return "Connected, attaching…";
    case "attached":
      return "Attached";
    case "notAttached":
      return `Connected, not attached: ${reasonText(device.state.reason)}`;
  }
}

function reasonText(reason: NotAttachedReason): string {
  switch (reason.kind) {
    case "unsupportedModel":
      return "unsupported";
    case "inUseByAnotherProcess":
      return "in use by another app";
    case "bluetoothPermissionDenied":
      return "Bluetooth permission denied. Enable it in System Settings › Privacy & Security › Bluetooth";
    case "serviceNotFound":
      return "control service not found";
    case "deviceNotConnected":
      return "device not connected";
    case "linkDropped":
      return "control link dropped";
    case "noResponse":
      return "the device didn't respond";
    case "other":
      return reason.message;
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
