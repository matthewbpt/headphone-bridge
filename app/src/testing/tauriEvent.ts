// Stands in for @tauri-apps/api/event under Vitest (see vitest.config.ts).
import type { DeviceList } from "../deviceList";
import { currentCore } from "./fakeCore";

export function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  const unlisten = currentCore().listen(
    event,
    handler as unknown as (event: { payload: DeviceList }) => void,
  );
  return Promise.resolve(unlisten);
}
