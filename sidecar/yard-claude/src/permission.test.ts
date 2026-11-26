import { expect, test } from "bun:test";
import { modeFor } from "./permission";

test("each turn switches to the mode yardd names for it", () => {
  // The first turn already runs in the mode it started with.
  expect(modeFor("full", "full", true)).toBeNull();
  // A watched channel's notice: Plan, even on the first turn of a full-access session.
  expect(modeFor("plan", "full", true)).toBe("plan");
  // Later turns always switch: an approved plan may have changed the mode.
  expect(modeFor("plan", "plan", false)).toBe("plan");
  expect(modeFor("full", "plan", false)).toBe("bypassPermissions");
  expect(modeFor("ask", "full", false)).toBe("default");
  // An older yardd sends none: the mode is left alone.
  expect(modeFor(undefined, "full", false)).toBeNull();
  expect(modeFor("", "full", false)).toBeNull();
});
