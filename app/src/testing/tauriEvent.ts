// Stands in for @tauri-apps/api/event under Vitest (see vitest.config.ts).
import { currentCore } from "./fakeCore";

export function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  const unlisten = currentCore().listen(event, handler as (event: { payload: unknown }) => void);
  return Promise.resolve(unlisten);
}
