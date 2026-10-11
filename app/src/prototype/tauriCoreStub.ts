// PROTOTYPE (window redesign, branch prototype/ui): stands in for
// @tauri-apps/api/core under `npm run prototype`, so the window runs in a
// plain browser with no Bluetooth or Tauri backend. Throw away.
import { fake } from "./fakeBackend";

export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return fake.invoke(cmd, args ?? {}) as Promise<T>;
}
