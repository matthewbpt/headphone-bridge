// PROTOTYPE: stands in for @tauri-apps/api/event under `npm run prototype`.
import { fake } from "./fakeBackend";

export function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  return Promise.resolve(fake.listen(event, handler as (event: { payload: unknown }) => void));
}
