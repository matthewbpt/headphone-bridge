import { render } from "@testing-library/react";
import { App } from "../App";
import { FakeCore, useFakeCore } from "./fakeCore";

/** Renders the whole window against a fake core. */
export function renderWindow(core: FakeCore) {
  useFakeCore(core);
  return render(<App />);
}
