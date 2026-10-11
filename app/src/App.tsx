import { useDeviceList } from "./deviceList";

export function App() {
  const deviceList = useDeviceList();

  if (deviceList === null) return null;
  if (deviceList.devices.length === 0) return <p>No device</p>;
  return (
    <ul>
      {deviceList.devices.map((device) => (
        <li key={device.address}>{device.name}</li>
      ))}
    </ul>
  );
}
