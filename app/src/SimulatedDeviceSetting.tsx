import { useState } from "react";
import { useLabels, useSimulatedDevice, whyRejected } from "./core";
import type { Model } from "./deviceList";

/**
 * The Simulated Device developer setting, in debug Android builds alone: a
 * Buds4 Pro or an XM5 that answers the core without Bluetooth, so the app
 * can be tried on the emulator. It's Attached and Detached like a real
 * Device. Shown only where the shell has one, and left out of release
 * bundles.
 */
export function SimulatedDeviceSetting() {
  const { model, simulate } = useSimulatedDevice();
  const labels = useLabels();
  const [error, setError] = useState<string | null>(null);

  if (model === undefined || labels === null) return null;
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
          {Object.entries(labels.models).map(([model, name]) => (
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
