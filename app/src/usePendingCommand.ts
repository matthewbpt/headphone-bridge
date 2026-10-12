import { useCallback, useEffect, useRef, useState } from "react";

/** How long a value asked of the Device stays pending if it never confirms it. */
const PENDING_TIMEOUT_MS = 10_000;

/**
 * A setting asked of the Device, pending until a snapshot reports it. Values
 * asked in turn queue up, oldest first, and the last is the one shown
 * pending. The Device applies them in order, so a reported value confirms
 * every value asked up to it; an earlier value landing late can't clear a
 * later one.
 *
 * `name` names a value in the errors: "<name> wasn't confirmed by the
 * Device" after 10 s, "Couldn't set <name>: <error>" when the core rejects it.
 */
export function usePendingCommand<T>(name: (value: T) => string) {
  const [asked, setAsked] = useState<T[]>([]);
  const [error, setError] = useState<string | null>(null);
  const nameRef = useRef(name);
  nameRef.current = name;
  const pending = asked.length > 0 ? asked[asked.length - 1] : null;

  useEffect(() => {
    if (pending === null) return;
    const timer = setTimeout(() => {
      setAsked([]);
      setError(`${nameRef.current(pending)} wasn't confirmed by the Device`);
    }, PENDING_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [pending]);

  /** Shows `value` pending and runs `command`, which asks the Device for it. */
  const send = (value: T, command: () => Promise<void>) => {
    setError(null);
    setAsked((asked) => [...asked, value]);
    command().catch((e) => {
      setAsked([]);
      setError(`Couldn't set ${nameRef.current(value)}: ${e}`);
    });
  };

  /** The Device now reports `value`: drops every value asked up to it. */
  const confirm = useCallback(
    (value: T | null) =>
      setAsked((asked) => {
        const i = asked.findIndex((v) => v === value);
        return i < 0 ? asked : asked.slice(i + 1);
      }),
    [],
  );

  /** Drops every pending value, as when the Device moves somewhere else. */
  const clear = useCallback(() => setAsked([]), []);

  return { pending, error, send, confirm, clear };
}
