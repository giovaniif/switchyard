import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { isAgentHome, loadChoice, loadFolder, loadHost, readRecent, rememberSession, validChoice } from "./sessionDefaults";
import type { Models } from "./protocol";

const original = globalThis.localStorage;
const models: Models = {
  claude: [{ id: "opus", name: "Opus", efforts: ["low", "high"], defaultEffort: "high" }],
  codex: [{ id: "fast", name: "Fast", efforts: ["low", "medium"], defaultEffort: "medium" }],
};
beforeEach(() => {
  const values = new Map<string, string>();
  globalThis.localStorage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } } as Storage;
});
afterEach(() => { globalThis.localStorage = original; });

describe("session defaults", () => {
  test("remembers a remote host, source repository, model and effort together", () => {
    rememberSession({ host: "server", cwd: "/worktrees/task", repo: "/projects/app", engine: "codex", model: "fast", effort: "low" });
    expect(loadHost()).toBe("server");
    expect(loadFolder("server")).toBe("/projects/app");
    expect(loadChoice(models, "server")).toEqual({ engine: "codex", model: "fast", effort: "low" });
    expect(readRecent("server")).toEqual(["/projects/app"]);
  });
  test("switching hosts retains each host's own project and choice", () => {
    rememberSession({ cwd: "/local", engine: "claude", model: "opus", effort: "high" });
    rememberSession({ host: "server", cwd: "/remote", engine: "codex", model: "fast", effort: "low" });
    expect(loadFolder("local")).toBe("/local");
    expect(loadChoice(models, "local").model).toBe("opus");
    expect(loadFolder("server")).toBe("/remote");
    expect(loadChoice(models, "server").model).toBe("fast");
  });
  test("keeps saved choices while model catalogs are loading", () => {
    rememberSession({ host: "server", cwd: "/remote", engine: "codex", model: "fast", effort: "low" });
    expect(loadChoice({}, "server")).toEqual({ engine: "codex", model: "fast", effort: "low" });
  });
  test("repairs removed models and unsupported efforts on the selected host", () => {
    expect(validChoice(models, { engine: "codex", model: "gone", effort: "ultra" })).toEqual({ engine: "codex", model: "fast", effort: "medium" });
    expect(validChoice({ codex: models.codex }, { engine: "claude", model: "opus", effort: "low" })).toEqual({ engine: "codex", model: "fast", effort: "low" });
    expect(validChoice({ grok: [{ id: "g", name: "G", efforts: [] }] }, { engine: "grok", model: "g", effort: "high" }).effort).toBe("");
  });
  test("migrates the older global choice and survives malformed storage", () => {
    localStorage.setItem("yard.choice", JSON.stringify({ engine: "codex", model: "fast", effort: "low" }));
    localStorage.setItem("yard.choice.server", "{broken");
    expect(loadChoice(models, "server").model).toBe("fast");
    localStorage.setItem("yard.choice", "null");
    expect(loadChoice(models).model).toBe("opus");
  });
});

test("an agent's own folder is never a recent folder or the next session's", () => {
  rememberSession({ engine: "codex", model: "m", effort: "low", cwd: "/home/me/.yard/agents/g1a2103dc55c1", agent: "g1a2103dc55c1" });
  expect(readRecent("local")).toEqual([]);
  expect(loadFolder("local")).toBe("");
  rememberSession({ engine: "codex", model: "m", effort: "low", cwd: "/projects/app" });
  // What an older build remembered is ignored.
  localStorage.setItem("yard.recent.local", JSON.stringify(["/home/me/.yard/agents/g9", "/projects/app"]));
  expect(readRecent("local")).toEqual(["/projects/app"]);
  expect(isAgentHome("/home/me/.yard/agents/g9/")).toBe(true);
  expect(isAgentHome("/home/me/www/agents/tools")).toBe(false);
});
