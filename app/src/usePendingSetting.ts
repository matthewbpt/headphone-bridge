import { useEffect, useRef, useState } from "react";
import { whyRejected } from "./core";

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
 * says nothing about those asked. Values are compared by their contents, so
 * a compound value such as an Equalizer settles on a reported copy, unless
 * `settles` says when a reported value settles one asked.
 *
 * `reportedNow` is the Device's latest value, null until it reports one.
 * `command` asks the Device for a value and resolves once the core accepts
 * it. `name` names a value in the errors: "<name> wasn't confirmed by the
 * Device" after 10 s, "Couldn't set <name>: <why>" when the core rejects
 * it, with the rejection in words as the why. The latest `command` and
 * `name` are always used, so neither has to be a stable reference.
 */
export function usePendingSetting<T>(
  reportedNow: T | null,
  command: (value: T) => Promise<void>,
  name: (value: T) => string,
  settles: (asked: T, reported: T) => boolean = sameValue,
) {
  const reported = useStable(reportedNow);
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

  // Settles against a change of the reported value only. React runs this
  // effect a moment after the first render, so without the check a value
  // asked in that moment would be dropped as if the Device had moved.
  const settledAgainst = useRef(reported);
  useEffect(() => {
    if (Object.is(settledAgainst.current, reported)) return;
    settledAgainst.current = reported;
    setAsked((asked) => {
      const i = reported === null ? -1 : asked.findIndex((v) => settles(v, reported));
      return i < 0 ? [] : asked.slice(i + 1);
    });
  }, [reported]);

  useEffect(() => {
    if (pending === null) return;
    const timer = setTimeout(() => {
      fail(`${nameRef.current(pending)} wasn't confirmed by the Device`);
    }, PENDING_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [pending]);

  /** Asks the Device for `value`, Pending until it reports it. */
  const request = (value: T) => {
    if (sameValue(value, shown)) return;
    setError(null);
    setAsked((asked) => [...asked, value]);
    commandRef.current(value).catch((e) => {
      fail(`Couldn't set ${nameRef.current(value)}: ${whyRejected(e)}`);
    });
  };

  return { shown, pending, error, request };
}

/**
 * `value`, kept as the earlier reference while its contents stay the same, so
 * a snapshot reporting an equal copy doesn't count as a change.
 */
function useStable<T>(value: T): T {
  const ref = useRef(value);
  if (!sameValue(ref.current, value)) ref.current = value;
  return ref.current;
}

/** Whether two setting values have the same contents. */
export function sameValue(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const keys = Object.keys(a);
  if (keys.length !== Object.keys(b).length) return false;
  return keys.every(
    (key) =>
      key in b &&
      sameValue((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]),
  );
}
