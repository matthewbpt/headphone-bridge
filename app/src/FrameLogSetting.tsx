import { useState } from "react";
import { type Platform, shareFrameLog, useFrameLog, whyRejected } from "./core";
import { SimulatedDeviceSetting } from "./SimulatedDeviceSetting";

/**
 * The frame log developer setting: while it's on, every frame sent to or
 * received from an Attached Device is appended to the file shown, in the test
 * fixture format. It stays on across launches (or set
 * HEADPHONE_BRIDGE_FRAME_LOG), so Devices Attached at startup are logged too.
 * On Android the file is in app-private storage, so it's shared from here.
 */
export function FrameLogSetting({ platform }: { platform: Platform | null }) {
  const { file, setEnabled } = useFrameLog();
  const [error, setError] = useState<string | null>(null);

  const toggle = (enabled: boolean) => {
    setError(null);
    setEnabled(enabled).catch((e) => setError(whyRejected(e)));
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
      {platform === "android" && <ShareFrameLog on={file !== null} />}
      <SimulatedDeviceSetting />
    </details>
  );
}

/** Hands the frame log file to Android's share sheet, to save or send it from the phone. */
function ShareFrameLog({ on }: { on: boolean }) {
  const [error, setError] = useState<string | null>(null);
  const share = () => {
    setError(null);
    shareFrameLog().catch((e) => setError(`Couldn't share the frame log: ${whyRejected(e)}`));
  };
  return (
    <div>
      <button className="button" disabled={!on} onClick={share}>
        Share frame log
      </button>
      {!on && <div>Turn the frame log on to share it.</div>}
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}
