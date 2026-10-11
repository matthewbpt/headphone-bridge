import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config.ts";

// The window's tests swap the Tauri APIs for a scripted fake core
// (src/testing/fakeCore.ts), at the window's boundary with the core.
export default mergeConfig(
  viteConfig,
  defineConfig({
    resolve: {
      alias: {
        "@tauri-apps/api/core": "/src/testing/tauriCore.ts",
        "@tauri-apps/api/event": "/src/testing/tauriEvent.ts",
        "@tauri-apps/plugin-autostart": "/src/testing/autostart.ts",
      },
    },
    // The fake core's labels test reads the core's Rust sources, outside app/.
    server: { fs: { allow: [".."] } },
    test: {
      environment: "jsdom",
      setupFiles: ["src/testing/setup.ts"],
    },
  }),
);
