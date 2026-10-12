import { useEffect, useRef, useState } from "react";

/** How long a value asked of the Device stays Pending if it never reports it. */
const PENDING_TIMEOUT_MS = 10_000;

/**
 * A setting value the app has asked the Device for, Pending until the Device
 * reports it. Values asked in turn queue up, oldest first, and the newest is
 * the one shown. The Device applies them in order, so a reported value
 * settles every value asked up to it; an earlier value landing late can't
 * clear a later one. A reported value that wasn't asked for means the Device
 * has moved somewhere else, so it drops every Pending value. Only a change of
 * the reported value settles them: a snapshot still reporting the old value
 * says nothing about those asked.
 *
 * `reported` is the Device's latest value, null until it reports one.
 * `command` asks the Device for a value and resolves once the core accepts
 * it. `name` names a value in the errors: "<name> wasn't confirmed by the
 * Device" after 10 s, "Couldn't set <name>: <why>" when the core rejects
 * it, with the rejection's message as the why. The latest `command` and
 * `name` are always used, so neither has to be a stable reference.
 */
export function usePendingSetting<T>(
  reported: T | null,
  command: (value: T) => Promise<void>,
  name: (value: T) => string,
) {
  const [asked, setAsked] = useState<T[]>([]);
  const [error, setError] = useState<string | null>(null);
  const commandRef = useRef(command);
  commandRef.current = command;
  const nameRef = useRef(name);
  nameRef.current = name;
  const pending = asked.length > 0 ? asked[asked.length - 1] : null;
  const shown = pending ?? reported;

  /** Drops every Pending value and shows `message`. */
  const fail = (message: string) => {
    setAsked([]);
    setError(message);
  };

  useEffect(
    () =>
      setAsked((asked) => {
        const i = asked.findIndex((v) => v === reported);
        return i < 0 ? [] : asked.slice(i + 1);
      }),
    [reported],
  );

  useEffect(() => {
    if (pending === null) return;
    const timer = setTimeout(() => {
      fail(`${nameRef.current(pending)} wasn't confirmed by the Device`);
    }, PENDING_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [pending]);

  /** Asks the Device for `value`, Pending until it reports it. */
  const request = (value: T) => {
    if (value === shown) return;
    setError(null);
    setAsked((asked) => [...asked, value]);
    commandRef.current(value).catch((e: Error) => {
      fail(`Couldn't set ${nameRef.current(value)}: ${e.message}`);
    });
  };

  return { shown, pending, error, request };
}
