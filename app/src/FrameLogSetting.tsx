import { useState } from "react";
import { useFrameLog } from "./core";

/**
 * The frame log developer setting: while it's on, every frame sent to or
 * received from an Attached Device is appended to the file shown, in the test
 * fixture format. It stays on across launches (or set
 * HEADPHONE_BRIDGE_FRAME_LOG), so Devices Attached at startup are logged too.
 */
export function FrameLogSetting() {
  const { file, setEnabled } = useFrameLog();
  const [error, setError] = useState<string | null>(null);

  const toggle = (enabled: boolean) => {
    setError(null);
    setEnabled(enabled).catch((e: Error) => setError(e.message));
  };

  if (file === undefined) return null;
  return (
    <details>
      <summary>Developer</summary>
      <label>
        <input type="checkbox" checked={file !== null} onChange={(e) => toggle(e.target.checked)} />{" "}
        Frame log
      </label>
      {file && <div>Logging frames to {file}. Stays on across launches.</div>}
      {error && <div>Couldn't open the frame log: {error}</div>}
    </details>
  );
}
