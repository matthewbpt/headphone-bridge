import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Node's, without its types.
declare const process: { env: Record<string, string | undefined> };

// Tauri expects a fixed port and must see Rust errors, so don't clear the screen.
export default defineConfig(({ mode }) => ({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  define: {
    // Whether this is a debug build: `vite dev`, the tests, or the production
    // build `tauri build --debug` runs with TAURI_ENV_DEBUG set. Vite drops
    // what's gated on it from release builds.
    __DEBUG_BUILD__: JSON.stringify(
      mode !== "production" || process.env.TAURI_ENV_DEBUG === "true",
    ),
  },
}));
