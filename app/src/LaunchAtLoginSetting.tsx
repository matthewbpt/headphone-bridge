import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { useEffect, useState } from "react";

/**
 * The opt-in "Launch at login" setting. The OS's login item is the setting
 * itself, so it's read back rather than stored. A login launch starts in the
 * tray.
 */
export function LaunchAtLoginSetting() {
  // Undefined until it's known.
  const [enabled, setEnabled] = useState<boolean | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    isEnabled().then(setEnabled, (e) => setError(String(e)));
  }, []);

  const toggle = (on: boolean) => {
    setError(null);
    (on ? enable() : disable()).then(isEnabled).then(setEnabled, (e) => setError(String(e)));
  };

  return (
    <div>
      <label>
        <input
          type="checkbox"
          checked={enabled ?? false}
          disabled={enabled === undefined}
          onChange={(e) => toggle(e.target.checked)}
        />{" "}
        Launch at login
      </label>
      {error && <div role="alert">Couldn't change Launch at login: {error}</div>}
    </div>
  );
}
