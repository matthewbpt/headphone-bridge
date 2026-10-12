import { createRoot } from "react-dom/client";
import { App } from "../App";
import type { DeviceList } from "../deviceList";
import "../window.css";
import { buds4Pro, only, xm5 } from "./devices";
import { FakeCore, useFakeCore } from "./fakeCore";

// The README's screenshots: the window against a fake core, one scene per
// `?scene=` (see scripts/screenshots.sh, which captures each of them).

const SCENES: Record<string, DeviceList> = {
  buds4Pro: only(buds4Pro({ chosenMode: "noiseCancelling", effectiveMode: "noiseCancelling" })),
  xm5: only(
    xm5({
      chosenMode: "ambient",
      effectiveMode: "ambient",
      battery: { kind: "headphones", level: 80 },
    }),
  ),
  twoDevices: {
    devices: [
      buds4Pro({
        chosenMode: "adaptive",
        effectiveMode: "adaptive",
        battery: { kind: "buds", left: 90, right: 85, case: 60 },
      }),
      xm5(),
    ],
    primary: buds4Pro().address,
  },
};

const scene = new URLSearchParams(location.search).get("scene") ?? "buds4Pro";
useFakeCore(new FakeCore(SCENES[scene]));
createRoot(document.getElementById("root")!).render(<App />);
