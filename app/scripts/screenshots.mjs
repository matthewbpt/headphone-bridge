// Captures the README's screenshots (docs/screenshots/): serves
// screenshots.html, the window against the fake core, and has headless
// Chrome screenshot each scene at the window's size, in light and dark.
// Set CHROME to Chrome's binary if it isn't at the macOS default.
import { spawn } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "vite";

const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const OUT = new URL("../../docs/screenshots/", import.meta.url);
// The window's size, from app/src-tauri/tauri.conf.json.
const WIDTH = 420;
const HEIGHT = 560;
const SHOTS = [
  { scene: "buds4Pro", scheme: "light" },
  { scene: "xm5", scheme: "dark" },
  { scene: "twoDevices", scheme: "light" },
];

const server = await createServer({
  configFile: new URL("../vite.screenshots.config.ts", import.meta.url).pathname,
  logLevel: "error",
});
await server.listen();
const base = server.resolvedUrls.local[0];

const profile = await mkdtemp(join(tmpdir(), "headphone-bridge-screenshots-"));
const chrome = spawn(CHROME, [
  "--headless",
  "--remote-debugging-port=0",
  `--user-data-dir=${profile}`,
  "--hide-scrollbars",
  "about:blank",
]);

try {
  const browserUrl = await new Promise((resolve, reject) => {
    chrome.on("error", reject);
    chrome.stderr.on("data", (data) => {
      const match = /DevTools listening on (ws:\/\/\S+)/.exec(data.toString());
      if (match) resolve(match[1]);
    });
  });
  const port = new URL(browserUrl).port;
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const target = targets.find(({ type }) => type === "page");
  const page = await connect(target.webSocketDebuggerUrl);

  await page.send("Page.enable");
  await page.send("Emulation.setDeviceMetricsOverride", {
    width: WIDTH,
    height: HEIGHT,
    deviceScaleFactor: 2,
    mobile: false,
  });
  for (const { scene, scheme } of SHOTS) {
    await page.send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: scheme }],
    });
    const loaded = page.once("Page.loadEventFired");
    await page.send("Page.navigate", { url: `${base}screenshots.html?scene=${scene}` });
    await loaded;
    // Let the window take its first Device list snapshot and labels.
    await new Promise((resolve) => setTimeout(resolve, 500));
    const { data } = await page.send("Page.captureScreenshot", { format: "png" });
    const file = new URL(`${scene}-${scheme}.png`, OUT);
    await writeFile(file, Buffer.from(data, "base64"));
    console.log(file.pathname);
  }
  page.close();
} finally {
  chrome.kill();
  await server.close();
  await rm(profile, { recursive: true, force: true });
}

/** A DevTools protocol session over a target's WebSocket. */
async function connect(url) {
  const socket = new WebSocket(url);
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = reject;
  });
  let nextId = 1;
  const replies = new Map();
  const waiters = new Map();
  socket.onmessage = ({ data }) => {
    const message = JSON.parse(data);
    if (message.id) {
      const { resolve, reject } = replies.get(message.id);
      replies.delete(message.id);
      if (message.error) reject(new Error(message.error.message));
      else resolve(message.result);
    } else {
      waiters.get(message.method)?.(message.params);
      waiters.delete(message.method);
    }
  };
  return {
    send(method, params = {}) {
      const id = nextId++;
      socket.send(JSON.stringify({ id, method, params }));
      return new Promise((resolve, reject) => replies.set(id, { resolve, reject }));
    },
    once(method) {
      return new Promise((resolve) => waiters.set(method, resolve));
    },
    close() {
      socket.close();
    },
  };
}
