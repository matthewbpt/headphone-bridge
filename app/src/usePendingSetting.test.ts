import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { usePendingSetting } from "./usePendingSetting";

/** A compound setting value, shaped like an Equalizer. */
type Curve = { preset: number; bands: number[] | null; clearBass: number | null };

const bright = (): Curve => ({ preset: 0x10, bands: [1, 2, 3, 2, 1], clearBass: 0 });
const manual = (): Curve => ({ preset: 0xa0, bands: [4, 0, -2, 0, 4], clearBass: 3 });

/** Renders the hook with `reported`, returning it and a way to report anew. */
function renderCurve(reported: Curve | null, command = vi.fn(async (_: Curve) => {})) {
  const hook = renderHook(
    ({ reported }) => usePendingSetting(reported, command, () => "the curve"),
    { initialProps: { reported } },
  );
  return { hook, command, report: (reported: Curve | null) => hook.rerender({ reported }) };
}

describe("a Pending compound value", () => {
  it("is settled by a reported value with equal contents, even as a different object", () => {
    const { hook, report } = renderCurve(bright());

    act(() => hook.result.current.request(manual()));
    expect(hook.result.current.pending).toEqual(manual());

    report(manual());

    expect(hook.result.current.pending).toBeNull();
    expect(hook.result.current.shown).toEqual(manual());
  });

  it("settles every value asked up to an equal reported value", () => {
    const { hook, report } = renderCurve(bright());
    const louder = { ...manual(), clearBass: 6 };

    act(() => hook.result.current.request(manual()));
    act(() => hook.result.current.request(louder));
    report(manual());

    expect(hook.result.current.pending).toEqual(louder);
  });

  it("is dropped by a reported value with different contents", () => {
    const { hook, report } = renderCurve(bright());

    act(() => hook.result.current.request(manual()));
    report({ ...manual(), bands: [4, 0, -2, 0, 5] });

    expect(hook.result.current.pending).toBeNull();
    expect(hook.result.current.shown).toEqual({ ...manual(), bands: [4, 0, -2, 0, 5] });
  });

  it("isn't asked for when it equals the value shown", () => {
    const { hook, command } = renderCurve(bright());

    act(() => hook.result.current.request(bright()));

    expect(command).not.toHaveBeenCalled();
    expect(hook.result.current.pending).toBeNull();
  });

  it("stays Pending through a snapshot still reporting an equal old value", () => {
    const { hook, report } = renderCurve(bright());

    act(() => hook.result.current.request(manual()));
    report(bright());

    expect(hook.result.current.pending).toEqual(manual());
  });
});
