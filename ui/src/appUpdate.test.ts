import { expect, test } from "bun:test";
import { readAppUpdate, sendUpdateAction, updateKey } from "./appUpdate";

test("only the app's update state shows the notice", () => {
  expect(readAppUpdate({ version: "0.21.0", ready: true })).toEqual({ version: "0.21.0", ready: true });
  expect(readAppUpdate({ version: "0.21.0" })).toEqual({ version: "0.21.0", ready: false });
  // A browser has no state; the app clears it with null.
  for (const value of [undefined, null, "0.21.0", {}, { version: 21 }, { version: "<b>x</b>" }, { version: "1.2.3".padEnd(50, "9") }]) {
    expect(readAppUpdate(value)).toBeNull();
  }
});

test("dismissing a found release does not hide it once it is installed", () => {
  expect(updateKey({ version: "0.21.0", ready: false })).not.toBe(updateKey({ version: "0.21.0", ready: true }));
});

test("actions go to the app as a Tauri event, and nowhere in a browser", () => {
  const sent: unknown[][] = [];
  const app = { __TAURI__: { event: { emit: (...args: unknown[]) => { sent.push(args); return Promise.resolve(); } } } };
  expect(sendUpdateAction("notes", app)).toBe(true);
  expect(sendUpdateAction("restart", app)).toBe(true);
  expect(sent).toEqual([["switchyard:update-action", "notes"], ["switchyard:update-action", "restart"]]);
  expect(sendUpdateAction("notes", {})).toBe(false);
  expect(sendUpdateAction("notes", { __TAURI__: {} })).toBe(false);
});
