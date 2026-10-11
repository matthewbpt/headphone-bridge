import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and must see Rust errors, so don't clear the screen.
// PROTOTYPE: `npm run prototype` swaps the Tauri APIs for in-memory fakes,
// so the window runs in a plain browser (src/prototype/).
export default defineConfig(({ mode }) => ({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  resolve:
    mode === "prototype"
      ? {
          alias: {
            "@tauri-apps/api/core": "/src/prototype/tauriCoreStub.ts",
            "@tauri-apps/api/event": "/src/prototype/tauriEventStub.ts",
            "@tauri-apps/plugin-autostart": "/src/prototype/autostartStub.ts",
          },
        }
      : undefined,
}));
