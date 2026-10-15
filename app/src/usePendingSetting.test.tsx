import { createRoot } from "react-dom/client";
import { afterEach, expect, it } from "vitest";
import { usePendingSetting } from "./usePendingSetting";

/** A level the Device reports as 1, stepped up by clicking. */
function Level() {
  const { shown, pending, request } = usePendingSetting<number>(1, () => new Promise(() => {}), String);
  return (
    <button aria-busy={pending !== null} onClick={() => request((shown ?? 0) + 1)}>
      {shown}
    </button>
  );
}

const reactGlobals = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
const actEnvironment = reactGlobals.IS_REACT_ACT_ENVIRONMENT;
afterEach(() => {
  reactGlobals.IS_REACT_ACT_ENVIRONMENT = actEnvironment;
  document.body.replaceChildren();
});

// Rendered outside act, as the window is when the core's first Device list
// arrives: React runs the effects a moment after the first render, and a
// click in that moment once lost its value to them.
it("keeps a value asked before the first render's effects have run Pending", async () => {
  reactGlobals.IS_REACT_ACT_ENVIRONMENT = false;
  const container = document.body.appendChild(document.createElement("div"));
  const rendered = new Promise((resolve) =>
    new MutationObserver(resolve).observe(container, { childList: true, subtree: true }),
  );
  createRoot(container).render(<Level />);
  await rendered;

  const button = container.querySelector("button")!;
  button.click();
  await new Promise((resolve) => setTimeout(resolve, 50));

  expect(button).toHaveTextContent("2");
  expect(button).toHaveAttribute("aria-busy", "true");
});
