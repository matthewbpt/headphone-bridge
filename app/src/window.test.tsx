import { act, fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { BUDS_ADDRESS, XM5_ADDRESS, buds4Pro, notAttached, only, xm5 } from "./testing/devices";
import { FakeCore } from "./testing/fakeCore";
import { renderWindow } from "./testing/renderWindow";

const mode = (name: string) => screen.findByRole("radio", { name: new RegExp(`^${name}`) });

describe("Noise Control Mode", () => {
  it("shows a clicked mode pending until a snapshot confirms it", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    renderWindow(core);

    await userEvent.click(await mode("Ambient"));

    expect(core.sent()).toEqual([
      { command: "set_noise_control_mode", args: { address: BUDS_ADDRESS, mode: "ambient" } },
    ]);
    expect(await mode("Ambient")).toHaveAttribute("aria-busy", "true");
    expect(await mode("Ambient")).not.toBeChecked();

    core.emit(only(buds4Pro({ chosenMode: "ambient" })));

    expect(await mode("Ambient")).toBeChecked();
    expect(await mode("Ambient")).toHaveAttribute("aria-busy", "false");
    expect(await mode("Noise Cancelling")).not.toBeChecked();
  });

  it("clears a pending mode and shows an error when the core rejects it", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    core.answer("set_noise_control_mode", "hold");
    renderWindow(core);

    await userEvent.click(await mode("Ambient"));
    expect(await mode("Ambient")).toHaveAttribute("aria-busy", "true");

    await core.release("set_noise_control_mode", { reject: "notAttached" });

    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't set Ambient: notAttached");
    expect(await mode("Ambient")).toHaveAttribute("aria-busy", "false");
    expect(await mode("Ambient")).not.toBeChecked();
    expect(await mode("Noise Cancelling")).toBeChecked();
  });

  it("clears a pending mode and shows an error when the Device doesn't confirm it within 10 s", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const core = new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    renderWindow(core);
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });

    await user.click(await mode("Ambient"));
    act(() => vi.advanceTimersByTime(9_900));
    expect(await mode("Ambient")).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    act(() => vi.advanceTimersByTime(100));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Ambient wasn't confirmed by the Device",
    );
    expect(await mode("Ambient")).toHaveAttribute("aria-busy", "false");
    expect(await mode("Noise Cancelling")).toBeChecked();
  });
});

describe("Ambient Level", () => {
  it("sends the chosen level to the Device", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 1 })));
    renderWindow(core);

    const slider = await screen.findByRole("slider", { name: /Ambient Level/ });
    fireEvent.change(slider, { target: { value: "3" } });

    expect(core.sent()).toEqual([
      { command: "set_ambient_level", args: { address: BUDS_ADDRESS, level: 3 } },
    ]);
  });
});

describe("a Device that isn't Attached", () => {
  it("tries to Attach again on Retry", async () => {
    const core = new FakeCore(
      only(notAttached(xm5(), { kind: "notAttached", reason: { kind: "inUseByAnotherProcess" } })),
    );
    renderWindow(core);

    expect(await screen.findByText("In use by another app")).toBeInTheDocument();
    await userEvent.click(await screen.findByRole("button", { name: "Retry" }));

    expect(core.sent()).toEqual([{ command: "retry_attach", args: { address: XM5_ADDRESS } }]);
  });
});
