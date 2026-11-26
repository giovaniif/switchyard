import { describe, expect, test } from "bun:test";
import type { SessionMeta } from "./protocol";
import { navigationEntries, navigationShortcut, selectedIndex } from "./navigation";

const session = (id: string, patch: Partial<SessionMeta> = {}): SessionMeta => ({
  id, title: id, engine: "codex", model: "m", effort: "low", cwd: "/work/app", status: "idle", host: "local", createdAt: 0, updatedAt: 0, ...patch,
});
const context = { localName: "This Mac", hosts: [{ id: "remote", name: "Dev box" }] };

describe("navigationEntries", () => {
  test("recent sessions come first, followed by every page and Import", () => {
    const entries = navigationEntries([session("old"), session("new", { updatedAt: 5 })], context, "");
    expect(entries.slice(0, 2).map((e) => e.title)).toEqual(["new", "old"]);
    expect(entries.filter((e) => e.group === "Pages").map((e) => e.title)).toEqual(["New session", "Agents", "Automations", "Connectors", "Settings", "Hosts", "Subscriptions", "Sessions", "Import"]);
  });
  test("search includes archived sessions and runs, matching every word across metadata", () => {
    const s = session("investigation", { host: "remote", archived: true, automation: "a", branch: "fix/login", group: "Backend" });
    expect(navigationEntries([s], context, "DEV login backend")[0]?.route).toEqual({ page: "session", id: "investigation" });
    expect(navigationEntries([s], context, "login absent")).toEqual([]);
    expect(navigationEntries([s], context, "archived").find(e => e.group === "Sessions")?.title).toBe("investigation");
  });
  test("a session's detail names its repository and branch, not the worktree path under the data dir", () => {
    const s = session("fix login", { cwd: "/home/me/.yard/wt/app/yard-fix-login-1a2b", repo: "/work/app", branch: "yard/fix-login-1a2b" });
    const [row] = navigationEntries([s], context, "");
    expect(row.detail).toBe("Codex · This Mac · app · yard/fix-login-1a2b");
    // The full paths still match a search.
    expect(navigationEntries([s], context, "/work/app")[0]?.title).toBe("fix login");
    expect(navigationEntries([s], context, ".yard/wt")[0]?.title).toBe("fix login");
    // A session in place shows its folder's name.
    expect(navigationEntries([session("plain", { cwd: "/work/notes/" })], context, "")[0].detail).toBe("Codex · This Mac · notes");
  });
  test("page searches put matching destinations ahead of similarly named sessions", () => {
    const entries = navigationEntries([session("Settings work")], context, "settings");
    expect(entries[0].route).toEqual({ page: "settings" });
    expect(entries[1].route).toEqual({ page: "session", id: "Settings work" });
    expect(navigationEntries([], context, "import")[0].action).toBe("import");
  });
  test("agents are destinations; their conversations are not listed apart", () => {
    const withAgents = { ...context, agents: [{ id: "g0", name: "Scribe", about: "" }, { id: "g1", name: "Reviewer", about: "Reviews" }] };
    const rows = navigationEntries([session("chat", { agent: "g1" }), session("mine")], withAgents, "");
    expect(rows.filter((e) => e.group === "Sessions").map((e) => e.title)).toEqual(["mine"]);
    expect(rows.filter((e) => e.group === "Agents").map((e) => e.route)).toEqual([{ page: "agent", id: "g0" }, { page: "agent", id: "g1" }]);
    expect(navigationEntries([session("chat", { agent: "g1" })], withAgents, "review").map((e) => e.title)).toEqual(["Reviewer"]);
  });
  test("every sidebar page is a destination; Switchy's old name finds the agents", () => {
    expect(navigationEntries([], context, "agents")[0]?.route).toEqual({ page: "agents" });
    expect(navigationEntries([], context, "switchy")[0]?.route).toEqual({ page: "agents" });
  });
  test("the recent list leaves out archived sessions and runs, as the sidebar does, but search finds them", () => {
    const runs = Array.from({ length: 9 }, (_, i) => session(`Loop patrol · ${i}`, { automation: "patrol", archived: true, updatedAt: 100 + i }));
    const live = session("Loop patrol · live", { automation: "patrol", updatedAt: 200 });
    const pinned = session("pinned run", { automation: "patrol", pinned: true, updatedAt: 50 });
    const archived = session("put away", { archived: true, updatedAt: 300 });
    const work = [session("fix login", { updatedAt: 2 }), session("plan refactor", { updatedAt: 1 })];
    const recent = navigationEntries([...runs, live, pinned, archived, ...work], context, "").filter((e) => e.group === "Sessions");
    expect(recent.map((e) => e.title)).toEqual(["pinned run", "fix login", "plan refactor"]);
    expect(navigationEntries([...runs, archived, ...work], context, "loop patrol 3")[0]?.title).toBe("Loop patrol · 3");
    expect(navigationEntries([...runs, archived, ...work], context, "put away")[0]?.title).toBe("put away");
  });
  test("the recent list is bounded, but search finds older sessions", () => {
    const sessions = Array.from({ length: 20 }, (_, i) => session(`session-${i}`, { updatedAt: i }));
    expect(navigationEntries(sessions, context, "").filter((e) => e.group === "Sessions")).toHaveLength(8);
    expect(navigationEntries(sessions, context, "session-0")[0].title).toBe("session-0");
  });
});

test("navigation shortcut preserves Mac Ctrl+K, modifiers, repeats and composition", () => {
  const key = { key: "k", metaKey: true, ctrlKey: false, shiftKey: false, altKey: false, repeat: false, isComposing: false };
  expect(navigationShortcut(key, true)).toBe(true);
  expect(navigationShortcut({ ...key, metaKey: false, ctrlKey: true }, false)).toBe(true);
  expect(navigationShortcut({ ...key, metaKey: false, ctrlKey: true }, true)).toBe(false);
  for (const patch of [{ shiftKey: true }, { altKey: true }, { repeat: true }, { isComposing: true }, { key: "j" }]) {
    expect(navigationShortcut({ ...key, ...patch }, true)).toBe(false);
  }
});

test("the highlight follows its entry when the list re-sorts", () => {
  const before = [{ id: "session-a" }, { id: "session-b" }, { id: "session-c" }];
  // c's turn started: it moves to the top while b stays highlighted.
  const after = [{ id: "session-c" }, { id: "session-a" }, { id: "session-b" }];
  expect(selectedIndex(before, "session-b")).toBe(1);
  expect(selectedIndex(after, "session-b")).toBe(2);
  expect(selectedIndex(after, null)).toBe(0);
  expect(selectedIndex(after, "session-gone")).toBe(0);
  expect(selectedIndex([], "session-b")).toBe(0);
});
