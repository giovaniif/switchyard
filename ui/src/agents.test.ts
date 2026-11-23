import { describe, expect, test } from "bun:test";
import type { Agent, SessionMeta } from "./protocol";
import { agentOf, agentStatus, chatsOf, initial, insertMention, leadingMention, matchAgents, mentionAt, nameProblem, runsOn } from "./agents";
import { initial as empty, parseHash, reduce, routeHash } from "./store";

const agent = (id: string, name: string, extra: Partial<Agent> = {}): Agent => ({
  id, name, color: "blue", about: "", engine: "codex", model: "", effort: "", account: "default", cwd: "", permission: "ask",
  instructions: "", memory: "", builtin: false, scope: "all", chat: null, createdAt: 0, updatedAt: 0, ...extra,
});
const agents = [agent("scout", "Scout", { builtin: true }), agent("g1", "Reviewer"), agent("g2", "Release-bot")];
const session = (id: string, extra: Partial<SessionMeta> = {}): SessionMeta => ({
  id, title: id, engine: "codex", model: "fake-fast", effort: "low", cwd: "/w", status: "idle", host: "local", createdAt: 0, updatedAt: 0, ...extra,
});

describe("mentions", () => {
  test("the @word under the caret, at the start or after a space", () => {
    expect(mentionAt("@sc", 3)).toEqual({ start: 0, query: "sc" });
    expect(mentionAt("ask @", 5)).toEqual({ start: 4, query: "" });
    expect(mentionAt("mail@host", 9)).toBeNull();
    expect(mentionAt("@scout is", 9)).toBeNull();
  });
  test("names that start with the query come first", () => {
    expect(matchAgents(agents, "re").map((a) => a.name)).toEqual(["Reviewer", "Release-bot"]);
    expect(matchAgents(agents, "bot").map((a) => a.name)).toEqual(["Release-bot"]);
    expect(matchAgents(agents, "").length).toBe(3);
  });
  test("picking one replaces the query and leaves the caret after a space", () => {
    expect(insertMention("@sc what runs?", 0, 3, "Scout")).toEqual({ text: "@Scout what runs?", caret: 7 });
    expect(insertMention("hi @re", 3, 6, "Reviewer")).toEqual({ text: "hi @Reviewer ", caret: 13 });
  });
  test("a message that starts with an agent's @Name goes to it, without the mention", () => {
    expect(leadingMention("@scout what is running?", agents)).toEqual({ agent: agents[0], rest: "what is running?" });
    expect(leadingMention("  @Reviewer\nlook at #12", agents)?.rest).toBe("look at #12");
    expect(leadingMention("@nobody hi", agents)).toBeNull();
    expect(leadingMention("ask @scout later", agents)).toBeNull();
    expect(leadingMention("@scouting", agents)).toBeNull();
  });
});

test("names: one mentionable word, unique, the rule said before yardd refuses", () => {
  expect(nameProblem("Helper", agents)).toBeNull();
  expect(nameProblem("Reviewer", agents, "g1")).toBeNull();
  expect(nameProblem("reviewer", agents)).toContain("already called");
  expect(nameProblem("two words", agents)).toContain("One word");
  expect(nameProblem("", agents)).toBe("Give it a name.");
  expect(nameProblem("x".repeat(25), agents)).toContain("24");
  expect(initial(" scout")).toBe("S");
});

test("a chat's agent is the hub's; a remote session's tag is not looked up", () => {
  expect(agentOf(agents, session("s1", { agent: "g1" }))?.name).toBe("Reviewer");
  expect(agentOf(agents, session("s2", { agent: "g1", host: "box" }))).toBeUndefined();
  expect(agentOf(agents, session("s3"))).toBeUndefined();
  const sessions = { a: session("a", { agent: "g1", updatedAt: 1 }), b: session("b", { agent: "g1", updatedAt: 5 }), c: session("c") };
  expect(chatsOf(sessions, "g1").map((s) => s.id)).toEqual(["b", "a"]);
});

test("the card's status line follows its ongoing chat", () => {
  const now = 10 * 60_000;
  const a = agent("g1", "Reviewer", { chat: "s1" });
  expect(agentStatus(agent("g1", "Reviewer"), {}, now)).toEqual({ tone: "quiet", text: "No chat yet", sessionId: null });
  expect(agentStatus(a, { s1: session("s1", { status: "running" }) }, now).text).toBe("Working…");
  expect(agentStatus(a, { s1: session("s1", { status: "needs_approval" }) }, now).tone).toBe("approval");
  expect(agentStatus(a, { s1: session("s1", { turnEndedAt: 8 * 60_000, lastTurn: "completed" }) }, now)).toEqual({ tone: "ok", text: "Last chat 2 min ago", sessionId: "s1" });
  expect(agentStatus(a, { s1: session("s1", { turnEndedAt: now, lastTurn: "failed" }) }, now).text).toBe("Failed just now");
  // A deleted chat leaves the agent with none.
  expect(agentStatus(a, {}, now).text).toBe("No chat yet");
});

test("how it runs, in the pickers' words", () => {
  const models = { codex: [{ id: "fake-fast", name: "Fake Fast", efforts: ["low"] }] };
  expect(runsOn(agent("g", "A", { model: "fake-fast", effort: "low" }), models, "Ask first")).toBe("Fake Fast · Low · Ask first");
  // No model of its own: the host's default, named; none listed yet, the engine's.
  expect(runsOn(agent("g", "A"), models, "Plan only")).toBe("Fake Fast · Plan only");
  expect(runsOn(agent("g", "A"), {}, "Plan only")).toBe("Codex default · Plan only");
});

test("routes and the agents broadcast", () => {
  for (const route of [{ page: "agents" }, { page: "agent", id: "g1" }, { page: "agentEdit", id: "new" }] as const) {
    expect(parseHash(routeHash(route))).toEqual(route);
  }
  // Switchy's old address lands on the agents.
  expect(parseHash("#/assistant")).toEqual({ page: "agents" });
  const s = reduce(empty, { t: "server", msg: { op: "agents", agents } });
  expect(s.agents.map((a) => a.id)).toEqual(["scout", "g1", "g2"]);
  // A remote host's own agents never replace the hub's.
  expect(reduce(s, { t: "server", msg: { op: "agents", hostId: "box", agents: [] } }).agents.length).toBe(3);
});
