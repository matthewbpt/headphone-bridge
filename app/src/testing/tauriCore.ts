// Stands in for @tauri-apps/api/core under Vitest (see vitest.config.ts).
import { currentCore } from "./fakeCore";

export function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return currentCore().invoke(command, args) as Promise<T>;
}
