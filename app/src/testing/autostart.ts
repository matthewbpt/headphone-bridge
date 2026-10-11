// Stands in for @tauri-apps/plugin-autostart under Vitest (see vitest.config.ts).
import { currentCore } from "./fakeCore";

export const isEnabled = () => currentCore().autostart.isEnabled();
export const enable = () => currentCore().autostart.enable();
export const disable = () => currentCore().autostart.disable();
