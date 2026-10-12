import { describe, expect, it } from "vitest";
import labelsRs from "../../src-tauri/src/labels.rs?raw";
import modelRs from "../../../crates/core/src/model.rs?raw";
import { LABELS } from "./fakeCore";

// The fake core's labels are a hand copy of the core's. Every one must still
// be a string literal in the Rust sources the `labels` command answers from,
// so renaming a label there fails here rather than passing the window's tests.
const rustSources = labelsRs + modelRs;

describe("the fake core's labels", () => {
  const labels = [
    LABELS.noDevice,
    ...Object.values(LABELS.modes),
    ...Object.values(LABELS.models),
    ...Object.values(LABELS.hints),
    ...LABELS.reasons.map(({ text }) => text),
  ];

  it.each(labels)("has %j in the core", (label) => {
    expect(rustSources).toContain(JSON.stringify(label));
  });
});
