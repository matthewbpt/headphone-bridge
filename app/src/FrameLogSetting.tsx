import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

/**
 * The frame log developer setting: while it's on, every frame sent to or
 * received from an Attached Device is appended to the file shown, in the test
 * fixture format.
 */
export function FrameLogSetting() {
  // The log file while the frame log is on; undefined until it's known.
  const [path, setPath] = useState<string | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<string | null>("frame_log").then(setPath);
  }, []);

  const toggle = (enabled: boolean) => {
    setError(null);
    invoke<string | null>("set_frame_log", { enabled }).then(setPath, (e) =>
      setError(typeof e === "object" && e?.frameLogUnavailable ? e.frameLogUnavailable : String(e)),
    );
  };

  if (path === undefined) return null;
  return (
    <details>
      <summary>Developer</summary>
      <label>
        <input type="checkbox" checked={path !== null} onChange={(e) => toggle(e.target.checked)} />{" "}
        Frame log
      </label>
      {path && <div>Logging frames to {path}</div>}
      {error && <div>Couldn't open the frame log: {error}</div>}
    </details>
  );
}
