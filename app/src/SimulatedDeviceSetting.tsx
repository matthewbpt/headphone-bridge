import { useState } from "react";
import { useSimulatedDevice, whyRejected } from "./core";
import type { Model } from "./deviceList";

const MODELS: { model: Model; name: string }[] = [
  { model: "galaxyBuds4Pro", name: "Galaxy Buds4 Pro" },
  { model: "wh1000xm5", name: "WH-1000XM5" },
];

/**
 * The Simulated Device developer setting, in debug Android builds alone: a
 * Buds4 Pro or an XM5 that answers the core without Bluetooth, so the app
 * can be tried on the emulator. It's Attached and Detached like a real
 * Device. Shown only where the shell has one.
 */
export function SimulatedDeviceSetting() {
  const { model, simulate } = useSimulatedDevice();
  const [error, setError] = useState<string | null>(null);

  if (model === undefined) return null;
  const choose = (chosen: string) => {
    setError(null);
    simulate((chosen || null) as Model | null).catch((e) => setError(whyRejected(e)));
  };
  return (
    <div>
      <label>
        Simulated Device{" "}
        <select value={model ?? ""} onChange={(e) => choose(e.target.value)}>
          <option value="">None</option>
          {MODELS.map(({ model, name }) => (
            <option key={model} value={model}>
              {name}
            </option>
          ))}
        </select>
      </label>
      {error && <div>Couldn't switch the Simulated Device: {error}</div>}
    </div>
  );
}
