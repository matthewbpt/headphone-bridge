import type { Battery, Device, Model, NotAttachedReason } from "./deviceList";
import { useDeviceList } from "./deviceList";

export function App() {
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
};

function DeviceView({ device }: { device: Device }) {
  return (
    <>
      <strong>{device.name}</strong>
      {device.model && <div>{MODEL_NAMES[device.model]}</div>}
      <div>{stateText(device)}</div>
      {device.battery && <BatteryView battery={device.battery} />}
    </>
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
    case "other":
      return reason.message;
  }
}

function BatteryView({ battery }: { battery: Battery }) {
  const percent = (level: number | null) => (level === null ? "–" : `${level}%`);
  return (
    <div>
      Battery: L {percent(battery.left)} · R {percent(battery.right)} · Case {percent(battery.case)}
    </div>
  );
}
