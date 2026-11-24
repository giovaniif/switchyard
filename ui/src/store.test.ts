import { describe, expect, test } from "bun:test";
import type { AgentEvent, SessionMeta } from "./protocol";
import { cleanGroupName, go, historyLoaded, hostErrorText, hostName, initial, mcpAuthKey, mcpKey, parseHash, reduce, routeHash, sidebarLayout, type State } from "./store";

const server = (state: State, msg: any) => reduce(state, { t: "server", msg });
const event = (sessionId: string, seq: number, ev: AgentEvent) => ({ op: "event", sessionId, seq, event: ev });
const history = (sessionId: string, events: AgentEvent[]) => ({ op: "history", sessionId, events });

const log: AgentEvent[] = [
  { type: "user_message", text: "hi" },
  { type: "turn_started" },
  { type: "assistant_delta", itemId: "m", text: "Hel" },
  { type: "assistant_delta", itemId: "m", text: "lo" },
  { type: "assistant_message", itemId: "m", text: "Hello" },
  { type: "turn_completed", status: "completed" },
];

const meta = (id: string, host = "local", status: SessionMeta["status"] = "idle"): SessionMeta => ({
  id,
  title: id,
  engine: "codex",
  model: "m",
  effort: "low",
  cwd: "/w",
  status,
  host,
  createdAt: 1,
  updatedAt: 1,
});

const texts = (state: State, sid: string) =>
  state.threads[sid].items
    .filter((i) => i.kind !== "turn")
    .map((i) => (i.kind === "user" || i.kind === "note" ? i.text : i.kind === "assistant" ? i.text : i.kind));

describe("thread loading", () => {
  test("history alone builds the thread", () => {
    const s = server(initial, history("s1", log));
    expect(s.threads.s1.next).toBe(6);
    expect(texts(s, "s1")).toEqual(["hi", "Hello"]);
    expect(s.threads.s1.items[2]).toMatchObject({ streaming: false });
  });

  test("events before the history wait, then only the new ones apply, in order", () => {
    let s = initial;
    // Arrive out of order and before the history; seq 1 is also in the history.
    s = server(s, event("s1", 3, { type: "note", text: "third" }));
    s = server(s, event("s1", 1, { type: "turn_started" }));
    s = server(s, event("s1", 2, { type: "note", text: "second" }));
    expect(s.threads.s1.next).toBe(-1);
    expect(texts(s, "s1")).toEqual([]);
    expect(s.threads.s1.early).toHaveLength(3);

    s = server(s, history("s1", [{ type: "user_message", text: "hi" }, { type: "turn_started" }]));
    expect(texts(s, "s1")).toEqual(["hi", "second", "third"]);
    expect(s.threads.s1.next).toBe(4);
    expect(s.threads.s1.early).toEqual([]);
  });

  test("an early event already covered by the history is not applied twice", () => {
    let s = server(initial, event("s1", 0, { type: "user_message", text: "hi" }));
    s = server(s, history("s1", [{ type: "user_message", text: "hi" }]));
    expect(texts(s, "s1")).toEqual(["hi"]);
    expect(s.threads.s1.next).toBe(1);
  });

  // A running session keeps streaming while its host relinks: its first event
  // after the snapshot buffers into a thread that has no history yet, and
  // that must still count as unloaded or the conversation stays blank.
  test("a thread holding only early events still needs its history", () => {
    let s = server(initial, { op: "host.snapshot", hostId: "h1", sessions: [meta("h1~a", "h1", "running")] });
    s = server(s, history("h1~a", log));
    expect(historyLoaded(s, "h1~a")).toBe(true);

    s = server(s, { op: "host.snapshot", hostId: "h1", sessions: [meta("h1~a", "h1", "running")] });
    s = server(s, event("h1~a", 6, { type: "turn_started" }));
    expect(s.threads["h1~a"]).toBeDefined();
    expect(historyLoaded(s, "h1~a")).toBe(false);

    s = server(s, history("h1~a", log));
    expect(historyLoaded(s, "h1~a")).toBe(true);
    expect(texts(s, "h1~a")).toEqual(["hi", "Hello"]);
  });

  test("live events apply once each; repeats and old seqs are dropped", () => {
    let s = server(initial, history("s1", log.slice(0, 2)));
    s = server(s, event("s1", 2, log[2]));
    s = server(s, event("s1", 2, log[2])); // duplicate
    s = server(s, event("s1", 1, log[1])); // already in history
    s = server(s, event("s1", 3, log[3]));
    expect(texts(s, "s1")).toEqual(["hi", "Hello"]);
    expect(s.threads.s1.items[2]).toMatchObject({ streaming: true });
    expect(s.threads.s1.next).toBe(4);

    const same = server(s, event("s1", 3, log[3]));
    expect(same).toBe(s);
  });

  test("an event from the future is not applied out of order", () => {
    let s = server(initial, history("s1", log.slice(0, 2)));
    const skipped = server(s, event("s1", 5, log[5]));
    expect(skipped).toBe(s);
  });

  test("threads are per session", () => {
    let s = server(initial, history("a", log));
    s = server(s, event("b", 0, { type: "user_message", text: "other" }));
    expect(s.threads.a.next).toBe(6);
    expect(s.threads.b.next).toBe(-1);
  });

  test("losing the socket drops threads so they reload", () => {
    let s = server(initial, history("s1", log));
    s = reduce(s, { t: "socket", connected: false });
    expect(s.threads).toEqual({});
    expect(s.connected).toBe(false);
    s = reduce(s, { t: "socket", connected: true });
    expect(s.connected).toBe(true);
  });
});

describe("sessions", () => {
  test("snapshot, update and delete", () => {
    let s = server(initial, {
      op: "snapshot",
      sessions: [meta("s1"), meta("s2")],
      models: { claude: [], codex: [{ id: "x", name: "X", efforts: ["low"] }] },
      usage: {},
      host: null,
      home: "/home/me",
      hosts: [],
    });
    expect(Object.keys(s.sessions).sort()).toEqual(["s1", "s2"]);
    expect(s.models.local.codex[0].id).toBe("x");
    expect(s.home.local).toBe("/home/me");

    s = server(s, { op: "session", session: meta("s1", "local", "running") });
    expect(s.sessions.s1.status).toBe("running");

    s = server(s, { op: "created", sessionId: "s2" });
    expect(s.route).toEqual({ page: "session", id: "s2" });
    s = server(s, history("s2", log));
    s = server(s, { op: "session.deleted", sessionId: "s2" });
    expect(s.sessions.s2).toBeUndefined();
    expect(s.threads.s2).toBeUndefined();
    expect(s.route).toEqual({ page: "new" });
  });

  test("a host snapshot replaces only that host's sessions and drops their threads", () => {
    let s = server(initial, { op: "snapshot", sessions: [meta("s1")], hosts: [{ id: "h1", name: "box", kind: "direct", status: "connected" }] });
    s = server(s, { op: "host.snapshot", hostId: "h1", sessions: [meta("h1~a", "h1")], models: null, usage: {}, host: null, home: "/r" });
    s = server(s, history("h1~a", log));
    s = server(s, history("s1", log));
    s = server(s, { op: "host.snapshot", hostId: "h1", sessions: [meta("h1~b", "h1")], home: "/r" });
    expect(Object.keys(s.sessions).sort()).toEqual(["h1~b", "s1"]);
    expect(s.threads["h1~a"]).toBeUndefined();
    expect(s.threads.s1).toBeDefined();

    // The host removed: its sessions go too.
    s = server(s, { op: "hosts", hosts: [] });
    expect(Object.keys(s.sessions)).toEqual(["s1"]);
  });

  test("errors count up even when the message repeats", () => {
    let s = server(initial, { op: "error", message: "nope" });
    s = server(s, { op: "error", message: "nope" });
    expect(s.error).toBe("nope");
    expect(s.errorCount).toBe(2);
    s = reduce(s, { t: "dismiss" });
    expect(s.error).toBeNull();
  });

  test("an op a host's older yardd does not know says to update that host", () => {
    let s = server(initial, { op: "snapshot", sessions: [], hosts: [{ id: "h1", name: "server", kind: "ssh", status: "connected" }] });
    s = server(s, { op: "error", for: "account.add", hostId: "h1", message: "unknown op `account.add`" });
    expect(s.error).toBe("server runs an older Switchyard. Use Update Switchyard on its card in Hosts, then try again.");
    // This machine's own yardd is the app's, so its message stays as it is.
    s = server(s, { op: "error", for: "account.add", message: "unknown op `account.add`" });
    expect(s.error).toBe("unknown op `account.add`");
  });
});

describe("route in the URL", () => {
  const snapshot = (...ids: string[]) => ({ op: "snapshot", sessions: ids.map((id) => meta(id)), hosts: [] });

  test("hash round trip", () => {
    for (const route of [
      { page: "new" },
      { page: "hosts" },
      { page: "hosts", host: "h/1 space", tab: "engines" },
      { page: "subscriptions" },
      { page: "sessions" },
      { page: "settings" },
      { page: "session", id: "s1a1e19674866" },
      { page: "automations" },
      { page: "automation", host: "local", id: "new" },
      { page: "automation", host: "h1", id: "a/7 b" },
    ] as const)
      expect(parseHash(routeHash(route))).toEqual(route);
    expect(parseHash("")).toEqual({ page: "new" });
    expect(parseHash("#/s/%E0%A4%A")).toEqual({ page: "new" });
    expect(routeHash({ page: "settings" })).toBe("#/settings");
    expect(routeHash({ page: "connectors" })).toBe("#/connectors");
    expect(parseHash("#/connectors")).toEqual({ page: "connectors" });
  });

  test("a reload opens the session the URL names once the snapshot lists it", () => {
    let s = go(initial, parseHash("#/s/s2"));
    expect(s.route).toEqual({ page: "new" });
    s = server(s, snapshot("s1", "s2"));
    expect(s.route).toEqual({ page: "session", id: "s2" });
    expect(s.restore).toBeNull();
  });

  test("a remote session opens when its host's snapshot arrives", () => {
    let s = go(initial, parseHash("#/s/r1"));
    s = server(s, snapshot("s1"));
    expect(s.route).toEqual({ page: "new" });
    s = server(s, { op: "host.snapshot", hostId: "h1", sessions: [meta("r1", "h1")] });
    expect(s.route).toEqual({ page: "session", id: "r1" });
  });

  test("going somewhere else first cancels the pending session", () => {
    let s = go(initial, parseHash("#/s/r1"));
    s = server(s, snapshot("s1"));
    s = reduce(s, { t: "route", route: { page: "session", id: "s1" } });
    s = server(s, { op: "host.snapshot", hostId: "h1", sessions: [meta("r1", "h1")] });
    expect(s.route).toEqual({ page: "session", id: "s1" });
  });
});

describe("sidebar layout", () => {
  const at = (id: string, over: Partial<SessionMeta>): SessionMeta => ({ ...meta(id), ...over });
  const ids = (list: SessionMeta[]) => list.map((s) => s.id);

  test("pinned, then ungrouped, then one section per group", () => {
    const l = sidebarLayout([
      at("a", { createdAt: 1 }),
      at("b", { createdAt: 2, pinned: true }),
      at("c", { createdAt: 3, group: "Release" }),
      at("d", { createdAt: 4 }),
      at("e", { createdAt: 5, archived: true }),
    ]);
    expect(ids(l.pinned)).toEqual(["b"]);
    expect(ids(l.ungrouped)).toEqual(["d", "a"]);
    expect(l.groups.map((g) => g.name)).toEqual(["Release"]);
    expect(ids(l.groups[0].sessions)).toEqual(["c"]);
    expect(ids(l.archived)).toEqual(["e"]);
  });

  test("a group's sessions are newest activity first and archived ones are left out", () => {
    const l = sidebarLayout([
      at("old", { group: "G", updatedAt: 10 }),
      at("new", { group: "G", updatedAt: 30 }),
      at("gone", { group: "G", updatedAt: 40, archived: true }),
    ]);
    expect(ids(l.groups[0].sessions)).toEqual(["new", "old"]);
    expect(ids(l.archived)).toEqual(["gone"]);
  });

  test("a group with only archived members does not exist; groups sort by name", () => {
    const l = sidebarLayout([
      at("a", { group: "beta" }),
      at("b", { group: "Alpha" }),
      at("c", { group: "Ghost", archived: true }),
    ]);
    expect(l.groups.map((g) => g.name)).toEqual(["Alpha", "beta"]);
  });

  test("a pinned grouped session shows under Pinned only", () => {
    const l = sidebarLayout([at("a", { group: "G", pinned: true })]);
    expect(ids(l.pinned)).toEqual(["a"]);
    expect(l.groups).toEqual([]);
  });

  test("group names are trimmed and capped", () => {
    expect(cleanGroupName("  Release  ")).toBe("Release");
    expect(cleanGroupName("   ")).toBe("");
    expect(cleanGroupName("x".repeat(100))).toHaveLength(60);
  });
});

describe("approval decisions", () => {
  const pending = (): State => {
    const s = server(initial, { op: "snapshot", sessions: [meta("s1", "local", "needs_approval")] });
    return server(
      server(s, history("s1", [{ type: "user_message", text: "[cmd] go" }, { type: "turn_started" }])),
      event("s1", 2, { type: "approval_request", requestId: "r1", kind: "command", title: "ls", detail: "" }),
    );
  };
  const card = (state: State) => state.threads.s1.items.find((i) => i.kind === "approval") as any;

  test("a sent decision holds the card until yardd resolves it", () => {
    // A second click before `approval_resolved` came back used to send a
    // second decision, which yardd refuses: "approval is no longer pending".
    const s = reduce(pending(), { t: "deciding", sessionId: "s1", requestId: "r1" });
    expect(card(s).sending).toBe(true);
    const done = server(s, event("s1", 3, { type: "approval_resolved", requestId: "r1", decision: "allow" }));
    expect(card(done).decision).toBe("allow");
  });

  test("a refused decision gives the buttons back", () => {
    const s = reduce(pending(), { t: "deciding", sessionId: "s1", requestId: "r1" });
    const refused = server(s, { op: "error", for: "session.approve", message: "the engine is gone" });
    expect(card(refused).sending).toBeFalsy();
    expect(refused.error).toBe("the engine is gone");
  });
});

describe("session.procs", () => {
  test("the latest list replaces the last, per session; the reply for an unknown op is no toast", () => {
    const rows = [{ pid: 1, ppid: 0, name: "bun", cmd: "bun main.ts", cpu: 1, memBytes: 2, ageSecs: 3, origin: "agent" }];
    let s = server(initial, { op: "session.procs", sessionId: "s1", procs: rows });
    expect(s.procs.s1).toEqual(rows);
    s = server(s, { op: "session.procs", sessionId: "s1", procs: [] });
    expect(s.procs.s1).toEqual([]);
    // An older remote yardd does not know the op.
    const e = server(s, { op: "error", for: "session.procs", message: "unknown op `session.procs`" });
    expect(e.error).toBeNull();
  });

  test("deleting a session forgets its list", () => {
    let s = server(initial, { op: "session.procs", sessionId: "s1", procs: [] });
    s = server(s, { op: "session.deleted", sessionId: "s1" });
    expect(s.procs.s1).toBeUndefined();
  });

  test("a remote yardd that does not know session.kill reads as a host to update", () => {
    const s = server(initial, { op: "hosts", hosts: [{ id: "h1", name: "TEST-box", kind: "direct", status: "connected" }] });
    expect(hostErrorText(s, "h1", "unknown op `session.kill`")).toBe(
      "TEST-box runs an older Switchyard. Use Update Switchyard on its card in Hosts, then try again.",
    );
    // This machine's own refusals pass through.
    expect(hostErrorText(s, undefined, "no such session")).toBe("no such session");
    expect(hostErrorText(s, "local", "unknown op `session.kill`")).toBe("unknown op `session.kill`");
  });
});

describe("the local host's name", () => {
  test("comes from the snapshot before the first stats sample", () => {
    // A Linux box or container must not read "This Mac" for its first seconds.
    const s = server(initial, { op: "snapshot", sessions: [], hosts: [], host: null, hostName: "TEST-box" });
    expect(hostName(s, "local")).toBe("TEST-box");
  });
  test("the stats sample's name still wins once it arrives", () => {
    const s = server(server(initial, { op: "snapshot", sessions: [], hosts: [], host: null, hostName: "TEST-box" }), {
      op: "host",
      host: { id: "local", name: "TEST-renamed", cpu: 1, cores: 2, load: 0, memUsed: 1, memTotal: 2, diskUsed: 1, diskTotal: 2, cpuHistory: [] },
    });
    expect(hostName(s, "local")).toBe("TEST-renamed");
  });
});

describe("connectors", () => {
  test("a listing replaces the loading mark; an older host's refusal ends it with what to do", () => {
    let s = server(initial, { op: "mcp.loading", host: "h1", engine: "claude", account: "default" });
    s = server(s, { op: "mcp.loading", host: "local", engine: "codex", account: "default" });
    expect(s.mcp[mcpKey("h1", "claude")].loading).toBe(true);
    s = server(s, { op: "mcp", engine: "codex", account: "default", servers: [{ name: "chart", status: "connected", source: "config", tools: 3, appTools: 2 }] });
    expect(s.mcp[mcpKey("local", "codex")]).toEqual({ engine: "codex", account: "default", servers: [{ name: "chart", status: "connected", source: "config", tools: 3, appTools: 2 }] });
    s = server(s, { op: "error", for: "mcp.status", hostId: "h1", message: "unknown op `mcp.status`" });
    expect(s.mcp[mcpKey("h1", "claude")]).toMatchObject({ loading: false, error: expect.stringContaining("older") });
    expect(s.error).toBeNull();
  });

  test("sign-in states are kept per server and cleared on request", () => {
    let s = server(initial, { op: "mcp.auth", hostId: "h1", engine: "claude", account: "default", server: "claude.ai Datadog", state: "open", url: "https://claude.ai/x", paste: false });
    const key = mcpAuthKey("h1", "claude", "default", "claude.ai Datadog");
    expect(s.mcpAuth[key]).toEqual({ state: "open", url: "https://claude.ai/x", paste: false });
    s = server(s, { op: "mcp.auth", host: "h1", engine: "claude", account: "default", server: "claude.ai Datadog", state: null });
    expect(s.mcpAuth[key]).toBeUndefined();
  });
});

