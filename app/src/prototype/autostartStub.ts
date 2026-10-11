// PROTOTYPE: stands in for @tauri-apps/plugin-autostart under `npm run prototype`.
let enabled = false;
const later = <T,>(value: T) => new Promise<T>((resolve) => setTimeout(() => resolve(value), 150));

export const isEnabled = () => later(enabled);
export const enable = () => later((enabled = true)).then(() => undefined);
export const disable = () => later((enabled = false)).then(() => undefined);
