import { act, fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Battery, Device } from "./deviceList";
import { BUDS_ADDRESS, XM5_ADDRESS, buds4Pro, notAttached, only, xm5 } from "./testing/devices";
import { FakeCore } from "./testing/fakeCore";
import { renderWindow } from "./testing/renderWindow";

const mode = (name: string) => screen.findByRole("radio", { name: new RegExp(`^${name}`) });

/**
 * Fakes the clock from here on, frozen: only the test moves it. Called once
 * the window is up, as `findBy…` queries and `userEvent` wait on timers;
 * click with `fireEvent` after it. (With `shouldAdvanceTime` the clock also
 * follows real time, so on a slow runner a 10 s timeout fired before the
 * test had advanced 10 s.)
 */
function freezeClock() {
  vi.useFakeTimers();
}

describe("Noise Control Mode", () => {
  const modeNames = async () =>
    (await screen.findAllByRole("radio")).map((radio) => radio.textContent);

  it("lists the Buds4 Pro's modes from hearing everything to hearing nothing", async () => {
    renderWindow(new FakeCore(only(buds4Pro())));

    expect(await modeNames()).toEqual(["Off", "Ambient", "Adaptive", "Noise Cancelling"]);
  });

  it("lists only the modes the XM5 supports, in the same order", async () => {
    renderWindow(new FakeCore(only(xm5())));

    expect(await modeNames()).toEqual(["Off", "Ambient", "Noise Cancelling"]);
  });

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

    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't set Ambient: the Device isn't Attached");
    expect(await mode("Ambient")).toHaveAttribute("aria-busy", "false");
    expect(await mode("Ambient")).not.toBeChecked();
    expect(await mode("Noise Cancelling")).toBeChecked();
  });

  it("clears a pending mode and shows an error when the Device doesn't confirm it within 10 s", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    renderWindow(core);
    const ambient = await mode("Ambient");
    freezeClock();

    fireEvent.click(ambient);
    act(() => vi.advanceTimersByTime(9_900));
    expect(ambient).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    act(() => vi.advanceTimersByTime(100));

    expect(screen.getByRole("alert")).toHaveTextContent("Ambient wasn't confirmed by the Device");
    expect(ambient).toHaveAttribute("aria-busy", "false");
    expect(screen.getByRole("radio", { name: /^Noise Cancelling/ })).toBeChecked();
  });

  it("moves the selection when the mode changes on the Device", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    renderWindow(core);
    expect(await mode("Noise Cancelling")).toBeChecked();

    core.emit(only(buds4Pro({ chosenMode: "adaptive", effectiveMode: "adaptive" })));

    expect(await mode("Adaptive")).toBeChecked();
    expect(await mode("Noise Cancelling")).not.toBeChecked();
  });

  it("drops a pending mode when the Device moves to another mode first", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    renderWindow(core);

    await userEvent.click(await mode("Ambient"));
    expect(await mode("Ambient")).toHaveAttribute("aria-busy", "true");

    core.emit(only(buds4Pro({ chosenMode: "off", effectiveMode: "off" })));

    expect(await mode("Off")).toBeChecked();
    expect(await mode("Ambient")).toHaveAttribute("aria-busy", "false");
    expect(await mode("Ambient")).toBeEnabled();
  });

  it("explains under the selector why the Device isn't applying the Chosen Mode", async () => {
    const core = new FakeCore(
      only(
        buds4Pro({
          chosenMode: "noiseCancelling",
          effectiveMode: "off",
          effectiveModeReason: "budOutOfEar",
        }),
      ),
    );
    renderWindow(core);

    expect(await screen.findByText("Paused: an earbud is out of the ear")).toBeVisible();
    expect(screen.queryByText(/Applying/)).not.toBeInTheDocument();

    core.emit(only(buds4Pro({ chosenMode: "noiseCancelling", effectiveMode: "noiseCancelling" })));

    expect(screen.queryByText("Paused: an earbud is out of the ear")).not.toBeInTheDocument();
  });
});

describe("Ambient Level", () => {
  const slider = () => screen.queryByRole("slider", { name: "Ambient Level" });

  it("is shown only while Ambient is the Chosen Mode", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    renderWindow(core);

    expect(await mode("Noise Cancelling")).toBeChecked();
    expect(slider()).not.toBeInTheDocument();

    core.emit(only(buds4Pro({ chosenMode: "ambient" })));
    expect(slider()).toBeInTheDocument();
  });

  it("appears the moment Ambient is clicked, before the Device confirms", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    core.answer("set_noise_control_mode", "hold");
    renderWindow(core);

    await userEvent.click(await mode("Ambient"));

    expect(await mode("Ambient")).not.toBeChecked();
    expect(slider()).toBeInTheDocument();
  });

  it("disappears the moment another mode is clicked, before the Device confirms", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient" })));
    core.answer("set_noise_control_mode", "hold");
    renderWindow(core);
    expect(await screen.findByRole("slider", { name: "Ambient Level" })).toBeInTheDocument();

    await userEvent.click(await mode("Off"));

    expect(await mode("Ambient")).toBeChecked();
    expect(slider()).not.toBeInTheDocument();
  });

  it("goes back to the confirmed Chosen Mode when the core rejects the change", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient" })));
    core.answer("set_noise_control_mode", "hold");
    renderWindow(core);

    await userEvent.click(await mode("Noise Cancelling"));
    expect(slider()).not.toBeInTheDocument();

    await core.release("set_noise_control_mode", { reject: "notAttached" });
    expect(slider()).toBeInTheDocument();
  });

  it("goes back to the confirmed Chosen Mode when the Device doesn't confirm within 10 s", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    renderWindow(core);
    const ambient = await mode("Ambient");
    freezeClock();

    fireEvent.click(ambient);
    act(() => vi.advanceTimersByTime(9_900));
    expect(slider()).toBeInTheDocument();

    act(() => vi.advanceTimersByTime(100));
    expect(slider()).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("Ambient wasn't confirmed by the Device");
  });

  it("follows a switch to or from Ambient made on the Device", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "off", effectiveMode: "off" })));
    renderWindow(core);
    expect(await mode("Off")).toBeChecked();

    core.emit(only(buds4Pro({ chosenMode: "ambient", effectiveMode: "ambient" })));
    expect(slider()).toBeInTheDocument();

    core.emit(only(buds4Pro({ chosenMode: "adaptive", effectiveMode: "adaptive" })));
    expect(slider()).not.toBeInTheDocument();
  });

  it("shows a notch per step of the Buds4 Pro's 5 steps, and no value", async () => {
    renderWindow(new FakeCore(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 3 }))));

    expect(await screen.findByRole("slider", { name: "Ambient Level" })).toHaveValue("3");
    expect(screen.getAllByTestId("notch")).toHaveLength(5);
    expect(screen.queryByText("3")).not.toBeInTheDocument();
  });

  it("shows the XM5's 20 steps as a plain track with the current value", async () => {
    renderWindow(new FakeCore(only(xm5({ chosenMode: "ambient", ambientLevel: 12 }))));

    expect(await screen.findByRole("slider", { name: "Ambient Level" })).toHaveValue("12");
    expect(screen.queryByTestId("notch")).not.toBeInTheDocument();
    expect(screen.getByText("12")).toBeVisible();
  });

  it("moves one step on − and +", async () => {
    const core = new FakeCore(only(xm5({ chosenMode: "ambient", ambientLevel: 12 })));
    renderWindow(core);

    await userEvent.click(await screen.findByRole("button", { name: "More" }));
    core.emit(only(xm5({ chosenMode: "ambient", ambientLevel: 13 })));
    await userEvent.click(screen.getByRole("button", { name: "Less" }));
    await userEvent.click(screen.getByRole("button", { name: "Less" }));

    expect(core.sent()).toEqual([
      { command: "set_ambient_level", args: { address: XM5_ADDRESS, level: 13 } },
      { command: "set_ambient_level", args: { address: XM5_ADDRESS, level: 12 } },
      { command: "set_ambient_level", args: { address: XM5_ADDRESS, level: 11 } },
    ]);
  });

  it("stops − and + at the Model's range limits", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 0 })));
    renderWindow(core);

    expect(await screen.findByRole("button", { name: "Less" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "More" })).toBeEnabled();

    core.emit(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 4 })));
    expect(screen.getByRole("button", { name: "More" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Less" })).toBeEnabled();
    expect(core.sent()).toEqual([]);
  });

  it("shows a level pending until a snapshot confirms it", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 1 })));
    renderWindow(core);

    await userEvent.click(await screen.findByRole("button", { name: "More" }));
    expect(slider()).toHaveValue("2");
    expect(slider()).toHaveAttribute("aria-busy", "true");

    core.emit(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 2 })));
    expect(slider()).toHaveValue("2");
    expect(slider()).toHaveAttribute("aria-busy", "false");
  });

  it("keeps a return to the confirmed level pending until the level asked before it has landed", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 1 })));
    renderWindow(core);

    await userEvent.click(await screen.findByRole("button", { name: "More" }));
    await userEvent.click(screen.getByRole("button", { name: "Less" }));
    expect(core.sent()).toEqual([
      { command: "set_ambient_level", args: { address: BUDS_ADDRESS, level: 2 } },
      { command: "set_ambient_level", args: { address: BUDS_ADDRESS, level: 1 } },
    ]);
    expect(slider()).toHaveValue("1");
    expect(slider()).toHaveAttribute("aria-busy", "true");

    // A snapshot from before the Device applied either level confirms nothing.
    core.emit(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 1, battery: { kind: "headphones", level: 50 } })));
    expect(slider()).toHaveAttribute("aria-busy", "true");

    core.emit(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 2 })));
    expect(slider()).toHaveValue("1");
    expect(slider()).toHaveAttribute("aria-busy", "true");

    core.emit(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 1 })));
    expect(slider()).toHaveValue("1");
    expect(slider()).toHaveAttribute("aria-busy", "false");
  });

  it("drops a pending level when the Device reports a level that wasn't asked for", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 1 })));
    renderWindow(core);
    const slider = await screen.findByRole("slider", { name: "Ambient Level" });
    freezeClock();

    fireEvent.change(slider, { target: { value: "3" } });
    expect(slider).toHaveAttribute("aria-busy", "true");

    core.emit(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 0 })));
    expect(slider).toHaveValue("0");
    expect(slider).toHaveAttribute("aria-busy", "false");

    act(() => vi.advanceTimersByTime(10_000));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows an error when a return to the confirmed level isn't confirmed within 10 s", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 1 })));
    renderWindow(core);
    const more = await screen.findByRole("button", { name: "More" });
    freezeClock();

    fireEvent.click(more);
    fireEvent.click(screen.getByRole("button", { name: "Less" }));
    core.emit(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 2 })));

    act(() => vi.advanceTimersByTime(10_000));
    expect(screen.getByRole("alert")).toHaveTextContent("Ambient Level 1 wasn't confirmed by the Device");
    expect(slider()).toHaveValue("2");
    expect(slider()).toHaveAttribute("aria-busy", "false");
  });

  it("shows an error when the Device doesn't confirm a level within 10 s", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 1 })));
    renderWindow(core);
    const more = await screen.findByRole("button", { name: "More" });
    freezeClock();

    fireEvent.click(more);
    act(() => vi.advanceTimersByTime(9_900));
    expect(slider()).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    act(() => vi.advanceTimersByTime(100));
    expect(screen.getByRole("alert")).toHaveTextContent("Ambient Level 2 wasn't confirmed by the Device");
    expect(slider()).toHaveValue("1");
    expect(slider()).toHaveAttribute("aria-busy", "false");
  });

  it("shows an error when the core rejects a level", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient", ambientLevel: 1 })));
    core.answer("set_ambient_level", "hold");
    renderWindow(core);

    await userEvent.click(await screen.findByRole("button", { name: "Less" }));
    await core.release("set_ambient_level", { reject: "notAttached" });

    expect(screen.getByRole("alert")).toHaveTextContent("Couldn't set Ambient Level 0: the Device isn't Attached");
    expect(slider()).toHaveValue("1");
    expect(slider()).toHaveAttribute("aria-busy", "false");
  });

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

describe("Noise Cancelling Level", () => {
  const slider = () => screen.queryByRole("slider", { name: "Noise Cancelling Level" });

  it("is shown only while Noise Cancelling is the Chosen Mode", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient" })));
    renderWindow(core);

    expect(await mode("Ambient")).toBeChecked();
    expect(slider()).not.toBeInTheDocument();

    core.emit(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    expect(slider()).toBeInTheDocument();
  });

  it("appears the moment Noise Cancelling is clicked, before the Device confirms", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient" })));
    core.answer("set_noise_control_mode", "hold");
    renderWindow(core);

    await userEvent.click(await mode("Noise Cancelling"));

    expect(await mode("Noise Cancelling")).not.toBeChecked();
    expect(slider()).toBeInTheDocument();
  });

  it("disappears the moment another mode is clicked, before the Device confirms", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    core.answer("set_noise_control_mode", "hold");
    renderWindow(core);
    expect(
      await screen.findByRole("slider", { name: "Noise Cancelling Level" }),
    ).toBeInTheDocument();

    await userEvent.click(await mode("Adaptive"));

    expect(await mode("Noise Cancelling")).toBeChecked();
    expect(slider()).not.toBeInTheDocument();
  });

  it("goes back to the confirmed Chosen Mode when the core rejects the change", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling" })));
    core.answer("set_noise_control_mode", "hold");
    renderWindow(core);

    await userEvent.click(await mode("Ambient"));
    expect(slider()).not.toBeInTheDocument();

    await core.release("set_noise_control_mode", { reject: "notAttached" });
    expect(slider()).toBeInTheDocument();
  });

  it("goes back to the confirmed Chosen Mode when the Device doesn't confirm within 10 s", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "ambient" })));
    renderWindow(core);
    const noiseCancelling = await mode("Noise Cancelling");
    freezeClock();

    fireEvent.click(noiseCancelling);
    act(() => vi.advanceTimersByTime(9_900));
    expect(slider()).toBeInTheDocument();

    act(() => vi.advanceTimersByTime(100));
    expect(slider()).not.toBeInTheDocument();
  });

  it("follows a switch to or from Noise Cancelling made on the Device, and isn't shown in Adaptive", async () => {
    const core = new FakeCore(only(buds4Pro({ chosenMode: "off", effectiveMode: "off" })));
    renderWindow(core);
    expect(await mode("Off")).toBeChecked();
    expect(slider()).not.toBeInTheDocument();

    core.emit(
      only(buds4Pro({ chosenMode: "noiseCancelling", effectiveMode: "noiseCancelling" })),
    );
    expect(slider()).toBeInTheDocument();

    core.emit(only(buds4Pro({ chosenMode: "adaptive", effectiveMode: "adaptive" })));
    expect(slider()).not.toBeInTheDocument();
  });

  it("shows a notch per step of the Buds4 Pro's 5 steps, and no value", async () => {
    renderWindow(
      new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling", noiseCancellingLevel: 3 }))),
    );

    expect(
      await screen.findByRole("slider", { name: "Noise Cancelling Level" }),
    ).toHaveValue("3");
    expect(screen.getAllByTestId("notch")).toHaveLength(5);
    expect(screen.queryByText("3")).not.toBeInTheDocument();
  });

  it("is disabled until the Device reports the level", async () => {
    renderWindow(
      new FakeCore(only(buds4Pro({ chosenMode: "noiseCancelling", noiseCancellingLevel: null }))),
    );

    expect(
      await screen.findByRole("slider", { name: "Noise Cancelling Level" }),
    ).toBeDisabled();
    expect(screen.getByRole("button", { name: "Less" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "More" })).toBeDisabled();
  });

  it("stays while an earbud is out and the Effective Mode has dropped to Off", async () => {
    renderWindow(
      new FakeCore(
        only(
          buds4Pro({
            chosenMode: "noiseCancelling",
            effectiveMode: "off",
            effectiveModeReason: "budOutOfEar",
            noiseCancellingLevel: 2,
          }),
        ),
      ),
    );

    expect(await screen.findByText("Paused: an earbud is out of the ear")).toBeVisible();
    expect(slider()).toHaveValue("2");
    expect(slider()).toBeEnabled();
  });

  it("isn't shown for a WH-1000XM5", async () => {
    renderWindow(new FakeCore(only(xm5({ chosenMode: "noiseCancelling" }))));

    expect(await mode("Noise Cancelling")).toBeChecked();
    expect(slider()).not.toBeInTheDocument();
  });

  const at = (noiseCancellingLevel: number | null, battery?: Battery) =>
    only(
      buds4Pro({ chosenMode: "noiseCancelling", noiseCancellingLevel, ...(battery && { battery }) }),
    );
  const set = (level: number) => ({
    command: "set_noise_cancelling_level",
    args: { address: BUDS_ADDRESS, level },
  });

  it("moves one step on − and +", async () => {
    const core = new FakeCore(at(2));
    renderWindow(core);

    await userEvent.click(await screen.findByRole("button", { name: "More" }));
    core.emit(at(3));
    await userEvent.click(screen.getByRole("button", { name: "Less" }));
    await userEvent.click(screen.getByRole("button", { name: "Less" }));

    expect(core.sent()).toEqual([set(3), set(2), set(1)]);
  });

  it("stops − and + at the Model's range limits", async () => {
    const core = new FakeCore(at(0));
    renderWindow(core);

    expect(await screen.findByRole("button", { name: "Less" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "More" })).toBeEnabled();

    core.emit(at(4));
    expect(screen.getByRole("button", { name: "More" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Less" })).toBeEnabled();
    expect(core.sent()).toEqual([]);
  });

  it("asks for each level in turn while dragging, pending until the last one lands", async () => {
    const core = new FakeCore(at(0));
    renderWindow(core);
    const track = await screen.findByRole("slider", { name: "Noise Cancelling Level" });

    for (const value of ["1", "2", "3"]) fireEvent.change(track, { target: { value } });
    expect(core.sent()).toEqual([set(1), set(2), set(3)]);

    core.emit(at(1));
    core.emit(at(2));
    expect(slider()).toHaveValue("3");
    expect(slider()).toHaveAttribute("aria-busy", "true");

    core.emit(at(3));
    expect(slider()).toHaveAttribute("aria-busy", "false");
  });

  it("shows a level pending until a snapshot confirms it", async () => {
    const core = new FakeCore(at(1));
    renderWindow(core);

    await userEvent.click(await screen.findByRole("button", { name: "More" }));
    expect(slider()).toHaveValue("2");
    expect(slider()).toHaveAttribute("aria-busy", "true");

    core.emit(at(2));
    expect(slider()).toHaveValue("2");
    expect(slider()).toHaveAttribute("aria-busy", "false");
  });

  it("keeps a return to the confirmed level pending until the level asked before it has landed", async () => {
    const core = new FakeCore(at(1));
    renderWindow(core);

    await userEvent.click(await screen.findByRole("button", { name: "More" }));
    await userEvent.click(screen.getByRole("button", { name: "Less" }));
    expect(core.sent()).toEqual([set(2), set(1)]);
    expect(slider()).toHaveValue("1");
    expect(slider()).toHaveAttribute("aria-busy", "true");

    // A snapshot from before the Device applied either level confirms nothing.
    core.emit(at(1, { kind: "headphones", level: 50 }));
    expect(slider()).toHaveAttribute("aria-busy", "true");

    core.emit(at(2));
    expect(slider()).toHaveValue("1");
    expect(slider()).toHaveAttribute("aria-busy", "true");

    core.emit(at(1));
    expect(slider()).toHaveValue("1");
    expect(slider()).toHaveAttribute("aria-busy", "false");
  });

  it("drops a pending level when the Device reports a level that wasn't asked for", async () => {
    const core = new FakeCore(at(1));
    renderWindow(core);
    const slider = await screen.findByRole("slider", { name: "Noise Cancelling Level" });
    freezeClock();

    fireEvent.change(slider, { target: { value: "3" } });
    expect(slider).toHaveAttribute("aria-busy", "true");

    core.emit(at(0));
    expect(slider).toHaveValue("0");
    expect(slider).toHaveAttribute("aria-busy", "false");

    act(() => vi.advanceTimersByTime(10_000));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows an error when the Device doesn't confirm a level within 10 s", async () => {
    const core = new FakeCore(at(1));
    renderWindow(core);
    const more = await screen.findByRole("button", { name: "More" });
    freezeClock();

    fireEvent.click(more);
    act(() => vi.advanceTimersByTime(9_900));
    expect(slider()).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    act(() => vi.advanceTimersByTime(100));
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Noise Cancelling Level 2 wasn't confirmed by the Device",
    );
    expect(slider()).toHaveValue("1");
    expect(slider()).toHaveAttribute("aria-busy", "false");
  });

  it("shows an error when the core rejects a level", async () => {
    const core = new FakeCore(at(1));
    core.answer("set_noise_cancelling_level", "hold");
    renderWindow(core);

    await userEvent.click(await screen.findByRole("button", { name: "Less" }));
    await core.release("set_noise_cancelling_level", { reject: "notAttached" });

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Couldn't set Noise Cancelling Level 0: the Device isn't Attached",
    );
    expect(slider()).toHaveValue("1");
    expect(slider()).toHaveAttribute("aria-busy", "false");
  });

  it("shows and sets each Device's own level with two Connected", async () => {
    const otherAddress = "AA:BB:CC:00:00:03";
    const home = buds4Pro({ chosenMode: "noiseCancelling", noiseCancellingLevel: 1 });
    const work = buds4Pro({
      address: otherAddress,
      name: "Work Buds4 Pro",
      chosenMode: "noiseCancelling",
      noiseCancellingLevel: 3,
    });
    const core = new FakeCore({ devices: [home, work], primary: BUDS_ADDRESS });
    renderWindow(core);

    expect(
      await screen.findByRole("slider", { name: "Noise Cancelling Level" }),
    ).toHaveValue("1");
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    expect(slider()).toHaveValue("2");

    await userEvent.click(screen.getByRole("tab", { name: /Work Buds4 Pro/ }));
    expect(slider()).toHaveValue("3");
    expect(slider()).toHaveAttribute("aria-busy", "false");
    await userEvent.click(screen.getByRole("button", { name: "More" }));

    expect(core.sent()).toEqual([
      set(2),
      { command: "set_noise_cancelling_level", args: { address: otherAddress, level: 4 } },
    ]);
  });

  it("sends the chosen level to the Device", async () => {
    const core = new FakeCore(
      only(buds4Pro({ chosenMode: "noiseCancelling", noiseCancellingLevel: 1 })),
    );
    renderWindow(core);

    const slider = await screen.findByRole("slider", { name: /Noise Cancelling Level/ });
    fireEvent.change(slider, { target: { value: "3" } });

    expect(core.sent()).toEqual([
      { command: "set_noise_cancelling_level", args: { address: BUDS_ADDRESS, level: 3 } },
    ]);
  });
});

describe("the Device picture", () => {
  const picture = () => screen.findByRole("img", { name: /./ });

  it("shows the Galaxy Buds4 Pro for a Buds4 Pro", async () => {
    renderWindow(new FakeCore(only(buds4Pro())));
    expect(await picture()).toHaveAccessibleName("Galaxy Buds4 Pro");
  });

  it("shows the WH-1000XM5 for an XM5", async () => {
    renderWindow(new FakeCore(only(xm5())));
    expect(await picture()).toHaveAccessibleName("WH-1000XM5");
  });

  it("shows a generic headphone for an unidentified Model", async () => {
    renderWindow(new FakeCore(only(notAttached(xm5({ model: null }), { kind: "attaching" }))));
    expect(await picture()).toHaveAccessibleName("Headphones");
  });

  it("shows a generic headphone for an unsupported Model", async () => {
    renderWindow(
      new FakeCore(
        only(notAttached(xm5({ model: null }), { kind: "notAttached", reason: { kind: "unsupportedModel" } })),
      ),
    );
    expect(await picture()).toHaveAccessibleName("Headphones");
  });
});

describe("the battery", () => {
  const battery = (device: Device) => {
    renderWindow(new FakeCore(only(device)));
    return screen.findByRole("group", { name: "Battery" });
  };
  const earbuds = (left: number | null, right: number | null, case_: number | null = null) =>
    buds4Pro({ battery: { kind: "buds", left, right, case: case_ } });

  it("merges equal earbuds into one L·R value", async () => {
    expect(await battery(earbuds(80, 80))).toHaveTextContent(/^L·R 80%$/);
  });

  it("shows unequal earbuds separately", async () => {
    expect(await battery(earbuds(80, 65))).toHaveTextContent(/^L 80% · R 65%$/);
  });

  it("shows an unknown earbud as a dash", async () => {
    expect(await battery(earbuds(null, 65))).toHaveTextContent(/^L – · R 65%$/);
  });

  it("shows two unknown earbuds separately", async () => {
    expect(await battery(earbuds(null, null))).toHaveTextContent(/^L – · R –$/);
  });

  it("shows the case while its level is reported", async () => {
    expect(await battery(earbuds(80, 80, 45))).toHaveTextContent(/^L·R 80% · Case 45%$/);
  });

  it("shows one value for headphones", async () => {
    expect(await battery(xm5({ battery: { kind: "headphones", level: 70 } }))).toHaveTextContent(/^70%$/);
  });
});

describe("a Device that isn't Attached", () => {
  it("shows Attaching… while the app Attaches, with no controls", async () => {
    renderWindow(new FakeCore(only(notAttached(buds4Pro(), { kind: "attaching" }))));

    expect(await screen.findByText("Attaching…")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Battery" })).not.toBeInTheDocument();
  });

  it("shows the reason and tries to Attach again on Retry", async () => {
    const core = new FakeCore(
      only(notAttached(xm5(), { kind: "notAttached", reason: { kind: "inUseByAnotherProcess" } })),
    );
    renderWindow(core);

    expect(await screen.findByText("In use by another app")).toBeVisible();
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    await userEvent.click(await screen.findByRole("button", { name: "Retry" }));

    expect(core.sent()).toEqual([{ command: "retry_attach", args: { address: XM5_ADDRESS } }]);
  });

  it("shows why the core rejected Retry", async () => {
    const core = new FakeCore(
      only(notAttached(xm5(), { kind: "notAttached", reason: { kind: "inUseByAnotherProcess" } })),
    );
    core.answer("retry_attach", "hold");
    renderWindow(core);

    await userEvent.click(await screen.findByRole("button", { name: "Retry" }));
    await core.release("retry_attach", { reject: "unknownDevice" });

    expect(screen.getByRole("alert")).toHaveTextContent("Couldn't retry: the Device isn't Connected");
  });

  it("shows a rejection that isn't one of the core's reasons as it is", async () => {
    const core = new FakeCore(
      only(notAttached(xm5(), { kind: "notAttached", reason: { kind: "inUseByAnotherProcess" } })),
    );
    core.answer("retry_attach", "hold");
    renderWindow(core);

    await userEvent.click(await screen.findByRole("button", { name: "Retry" }));
    await core.release("retry_attach", { reject: "the shell went away" });

    expect(screen.getByRole("alert")).toHaveTextContent("Couldn't retry: the shell went away");
  });

  it("shows an unsupported Model's reason without Retry", async () => {
    renderWindow(
      new FakeCore(
        only(notAttached(xm5({ model: null }), { kind: "notAttached", reason: { kind: "unsupportedModel" } })),
      ),
    );

    expect(await screen.findByText("Unsupported Model")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
  });
});

describe("the header", () => {
  it("titles the window with the Device's name and shows its Model when the name differs", async () => {
    const core = new FakeCore(only(xm5({ name: "Matthew's XM5" })));
    renderWindow(core);

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(/^Matthew's XM5$/);
    expect(screen.getByText("WH-1000XM5")).toBeVisible();
  });

  it("shows no Model subtitle when the Device's name is the Model name", async () => {
    const core = new FakeCore(only(xm5({ name: "WH-1000XM5" })));
    renderWindow(core);

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(/^WH-1000XM5$/);
    expect(screen.getAllByText("WH-1000XM5")).toHaveLength(1);
  });
});

describe("Power Off", () => {
  const powerOff = () => screen.findByRole("button", { name: "Power Off" });

  it("is a button in an XM5's header, with a tooltip", async () => {
    renderWindow(new FakeCore(only(xm5())));

    expect(await powerOff()).toBeEnabled();
    expect(await powerOff()).toHaveAttribute("title", "Power Off");
    expect((await powerOff()).closest("header")).toHaveTextContent("WH-1000XM5");
  });

  it("is disabled unless the Device is Attached", async () => {
    const core = new FakeCore(only(xm5({ state: { kind: "attaching" } })));
    renderWindow(core);

    expect(await powerOff()).toBeDisabled();

    core.emit(only(xm5()));
    expect(await powerOff()).toBeEnabled();
  });

  it("sends Power Off for the Device shown, with no confirmation", async () => {
    const core = new FakeCore({ devices: [buds4Pro(), xm5()], primary: XM5_ADDRESS });
    renderWindow(core);

    await userEvent.click(await powerOff());

    expect(core.sent()).toEqual([{ command: "power_off", args: { address: XM5_ADDRESS } }]);
  });

  it("disables itself and every control of the Device while it is Powering Off", async () => {
    const core = new FakeCore(only(xm5({ chosenMode: "ambient" })));
    renderWindow(core);
    await userEvent.click(await powerOff());

    core.emit(only(xm5({ chosenMode: "ambient", state: { kind: "poweringOff" } })));

    expect(await screen.findByText("Powering Off…")).toBeVisible();
    expect(await powerOff()).toBeDisabled();
    const radios = screen.getAllByRole("radio");
    expect(radios).toHaveLength(3);
    for (const radio of radios) expect(radio).toBeDisabled();
    expect(screen.getByRole("button", { name: "Less" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "More" })).toBeDisabled();
    expect(screen.getByRole("slider")).toBeDisabled();
  });

  it("shows why the core refused Power Off", async () => {
    const core = new FakeCore(only(xm5()));
    core.answer("power_off", "hold");
    renderWindow(core);

    await userEvent.click(await powerOff());
    await core.release("power_off", { reject: "poweringOff" });

    expect(screen.getByRole("alert")).toHaveTextContent("Couldn't Power Off: the Device is Powering Off");
  });

  it("isn't shown for a Buds4 Pro", async () => {
    renderWindow(new FakeCore(only(buds4Pro())));

    expect(await screen.findByRole("heading", { level: 1 })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Power Off" })).not.toBeInTheDocument();
  });
});

describe("the Device switcher", () => {
  it("isn't shown with one Device Connected", async () => {
    const core = new FakeCore(only(buds4Pro()));
    renderWindow(core);

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent("Matthew's Buds4 Pro");
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
  });

  it("shows a pill per Device with two Connected, marks the Primary and switches on a pick", async () => {
    const core = new FakeCore({ devices: [buds4Pro(), xm5()], primary: XM5_ADDRESS });
    renderWindow(core);

    const pills = await screen.findAllByRole("tab");
    expect(pills.map((pill) => pill.textContent)).toEqual([
      "Matthew's Buds4 Pro",
      expect.stringMatching(/^WH-1000XM5.*Primary/),
    ]);
    expect(screen.getByRole("tab", { name: /WH-1000XM5/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/^WH-1000XM5$/);

    await userEvent.click(screen.getByRole("tab", { name: /Buds4 Pro/ }));

    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/^Matthew's Buds4 Pro$/);
    expect(screen.getByRole("tab", { name: /Buds4 Pro/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: /WH-1000XM5/ })).toHaveAttribute("aria-selected", "false");
  });

  it("follows the Primary Device when the OS switches audio output, over a pick", async () => {
    const core = new FakeCore({ devices: [buds4Pro(), xm5()], primary: BUDS_ADDRESS });
    renderWindow(core);
    const title = () => screen.getByRole("heading", { level: 1 });

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(/^Matthew's Buds4 Pro$/);
    await userEvent.click(screen.getByRole("tab", { name: /WH-1000XM5/ }));
    expect(title()).toHaveTextContent(/^WH-1000XM5$/);

    core.emit({ devices: [buds4Pro(), xm5()], primary: XM5_ADDRESS });
    expect(title()).toHaveTextContent(/^WH-1000XM5$/);
    expect(screen.getByRole("tab", { name: /WH-1000XM5/ })).toHaveTextContent("Primary");

    core.emit({ devices: [buds4Pro(), xm5()], primary: BUDS_ADDRESS });
    expect(title()).toHaveTextContent(/^Matthew's Buds4 Pro$/);
    expect(screen.getByRole("tab", { name: /Buds4 Pro/ })).toHaveAttribute("aria-selected", "true");
  });
});

describe("Settings", () => {
  it("is collapsed, with the frame log in a further collapsed Developer section", async () => {
    const core = new FakeCore(only(buds4Pro()));
    renderWindow(core);

    const settings = await screen.findByText("Settings");
    expect(await screen.findByRole("checkbox", { name: "Launch at login" })).not.toBeVisible();
    expect(await screen.findByText("Developer")).not.toBeVisible();

    await userEvent.click(settings);
    expect(screen.getByRole("checkbox", { name: "Launch at login" })).toBeVisible();
    expect(screen.getByRole("checkbox", { name: "Frame log" })).not.toBeVisible();

    await userEvent.click(screen.getByText("Developer"));
    expect(screen.getByRole("checkbox", { name: "Frame log" })).toBeVisible();
  });

  it("shows why the frame log couldn't be opened", async () => {
    const core = new FakeCore(only(buds4Pro()));
    core.answer("set_frame_log", { reject: { frameLogUnavailable: "Permission denied (os error 13)" } });
    renderWindow(core);

    await userEvent.click(await screen.findByText("Settings"));
    await userEvent.click(screen.getByText("Developer"));
    await userEvent.click(screen.getByRole("checkbox", { name: "Frame log" }));

    expect(
      await screen.findByText("Couldn't open the frame log: Permission denied (os error 13)"),
    ).toBeVisible();
    expect(screen.getByRole("checkbox", { name: "Frame log" })).not.toBeChecked();
  });
});
