import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";
import { useFakeCore } from "./fakeCore";

afterEach(() => {
  cleanup();
  useFakeCore(null);
  vi.useRealTimers();
});
