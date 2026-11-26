import { beforeEach, afterEach, expect, test } from "bun:test";
import { loadDefaults, saveDefaults } from "./launchDefaults";
import { loadChoice, loadHost, rememberSession } from "./sessionDefaults";
import type { Models } from "./protocol";
const original = globalThis.localStorage;
const choice = { engine: "codex" as const, model: "fast", effort: "high" };
const models: Models = {
  codex: [
    {
      id: "fast",
      name: "Fast",
      efforts: ["low", "high"],
      defaultEffort: "low",
    },
    { id: "other", name: "Other", efforts: ["low"] },
  ],
};
beforeEach(() => {
  const values = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => values.get(k) ?? null,
    setItem: (k: string, v: string) => {
      values.set(k, v);
    },
    removeItem: (k: string) => {
      values.delete(k);
    },
  } as Storage;
});
afterEach(() => {
  globalThis.localStorage = original;
});
test("explicit session defaults survive different one-off host/model selections", () => {
  saveDefaults("sessions", { host: "server", choice });
  rememberSession({
    host: "local",
    engine: "codex",
    model: "other",
    effort: "low",
    cwd: "/test",
  });
  expect(loadHost()).toBe("server");
  expect(loadChoice(models, "server")).toEqual(choice);
  expect(loadChoice(models, "local").model).toBe("other");
  expect(loadChoice({}, "server")).toEqual(choice);
  saveDefaults("sessions", null);
  expect(loadHost()).toBe("local");
  expect(loadChoice(models, "server").model).toBe("other");
});
test("invalid preferences and unavailable storage do not break launch", () => {
  localStorage.setItem(
    "yard.defaults.sessions",
    '{"host":"all","choice":{"engine":"codex","model":"fast","effort":"low"}}',
  );
  expect(loadDefaults("sessions")).toBeNull();
  globalThis.localStorage = {
    getItem() {
      throw Error("blocked");
    },
    setItem() {
      throw Error("blocked");
    },
  } as unknown as Storage;
  expect(loadDefaults("sessions")).toBeNull();
  expect(saveDefaults("sessions", { host: "local", choice })).toBe(false);
});
test("a removed default model falls back without overwriting the saved preference", () => {
  saveDefaults("sessions", {
    host: "local",
    choice: { ...choice, model: "gone" },
  });
  expect(loadChoice(models).model).toBe("fast");
  expect(loadDefaults("sessions")?.choice.model).toBe("gone");
});
test("a removed default host falls back without dropping the saved model", () => {
  saveDefaults("sessions", { host: "gone", choice });
  rememberSession({
    host: "local",
    engine: "codex",
    model: "other",
    effort: "low",
    cwd: "/test",
  });
  // New session asked for "gone" and runs on local because it was removed.
  expect(loadChoice(models, "local", "gone")).toEqual(choice);
  // Picking local on purpose still restores what was last used there.
  expect(loadChoice(models, "local", "local").model).toBe("other");
});
