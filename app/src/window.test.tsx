import { act, fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { BUDS_ADDRESS, XM5_ADDRESS, buds4Pro, notAttached, only, xm5 } from "./testing/devices";
import { FakeCore } from "./testing/fakeCore";
import { renderWindow } from "./testing/renderWindow";

const mode = (name: string) => screen.findByRole("radio", { name: new RegExp(`^${name}`) });

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
});
