import { describe, expect, test } from "bun:test";
import { createSubagents } from "./subagents";
import { toolStarted } from "./tools";

// Shapes taken from a real Haiku turn (2026-11-15, Agent SDK 0.3.323): the
// Agent call, task_started, the subagent's own messages with
// parent_tool_use_id set, task_notification, then the call's tool_result.
const AGENT = "toolu_agent";

const assistant = (parent: string | null, ...content: unknown[]) => ({ type: "assistant", parent_tool_use_id: parent, message: { content } });
const user = (parent: string | null, ...content: unknown[]) => ({ type: "user", parent_tool_use_id: parent, message: { content } });
const started = (backgrounded: boolean, extra: Record<string, unknown> = {}) => ({
  type: "system",
  subtype: "task_started",
  task_id: "a1",
  tool_use_id: AGENT,
  description: "list files",
  subagent_type: "general-purpose",
  task_type: "local_agent",
  is_backgrounded: backgrounded,
  ...extra,
});
const notification = (extra: Record<string, unknown> = {}) => ({
  type: "system",
  subtype: "task_notification",
  task_id: "a1",
  tool_use_id: AGENT,
  status: "completed",
  summary: "a.txt b.txt",
  usage: { total_tokens: 27320, tool_uses: 1, duration_ms: 3568 },
  ...extra,
});

describe("the Agent call", () => {
  test("carries its description and the subagent's type", () => {
    const block = { id: AGENT, name: "Agent", input: { subagent_type: "general-purpose", description: "list files", prompt: "Run ls" } };
    expect(toolStarted(block, "/w")).toEqual({
      type: "tool_started",
      itemId: AGENT,
      kind: "other",
      title: "Agent",
      detail: "list files",
      agent: "general-purpose",
    });
  });

  test("the older Task name is the same call; an ordinary tool has no agent field", () => {
    expect(toolStarted({ id: "t", name: "Task", input: { description: "d", subagent_type: "Explore" } }, "/w")).toMatchObject({ title: "Agent", agent: "Explore" });
    // A call that names no type runs the general-purpose agent, and still reads as an agent.
    expect(toolStarted({ id: "t", name: "Agent", input: { description: "Live update", prompt: "…", run_in_background: true } }, "/w")).toMatchObject({ title: "Agent", agent: "general-purpose" });
    const bash = toolStarted({ id: "b", name: "Bash", input: { command: "ls" } }, "/w");
    expect(bash).toEqual({ type: "tool_started", itemId: "b", kind: "command", title: "Ran", detail: "ls" });
  });
});

describe("a subagent's own messages", () => {
  test("a tool call becomes a tool_started under its Agent call", () => {
    const s = createSubagents("/w");
    const m = assistant(AGENT, { type: "tool_use", id: "t1", name: "Read", input: { file_path: "/w/README.md" } });
    expect(s.child(m)).toEqual([{ type: "tool_started", itemId: "t1", kind: "read", title: "Read", detail: "README.md", parent: AGENT }]);
  });

  test("a subagent's edit gives the tool line only, no diff for the thread", () => {
    const s = createSubagents("/w");
    const m = assistant(AGENT, { type: "tool_use", id: "t2", name: "Edit", input: { file_path: "/w/a.ts", old_string: "a", new_string: "b" } });
    expect(s.child(m).map((e) => e.type)).toEqual(["tool_started"]);
  });

  test("a result completes its call", () => {
    const s = createSubagents("/w");
    const m = user(AGENT, { type: "tool_result", tool_use_id: "t1", content: "a.txt\nb.txt", is_error: false });
    expect(s.child(m)).toEqual([{ type: "tool_completed", itemId: "t1", ok: true, output: "a.txt\nb.txt" }]);
    const bad = user(AGENT, { type: "tool_result", tool_use_id: "t1", content: [{ type: "text", text: "boom" }], is_error: true });
    expect(s.child(bad)).toEqual([{ type: "tool_completed", itemId: "t1", ok: false, output: "boom" }]);
  });

  test("thinking, text, the prompt and stream events add nothing", () => {
    const s = createSubagents("/w");
    expect(s.child(assistant(AGENT, { type: "thinking", thinking: "hm" }))).toEqual([]);
    expect(s.child(assistant(AGENT, { type: "text", text: "working" }))).toEqual([]);
    expect(s.child(user(AGENT, { type: "text", text: "Run ls" }))).toEqual([]);
    expect(s.child({ type: "stream_event", parent_tool_use_id: AGENT, event: {} })).toEqual([]);
  });

  test("a subagent that starts one of its own nests under it", () => {
    const s = createSubagents("/w");
    const m = assistant("toolu_outer", { type: "tool_use", id: AGENT, name: "Agent", input: { subagent_type: "Explore", description: "dig" } });
    expect(s.child(m)).toEqual([
      { type: "tool_started", itemId: AGENT, kind: "other", title: "Agent", detail: "dig", agent: "Explore", parent: "toolu_outer" },
    ]);
  });
});

describe("the task's life", () => {
  test("the Agent call ends with its notification: the subagent's last words, its duration", () => {
    const s = createSubagents("/w");
    expect(s.task(started(false))).toEqual([]);
    s.child(assistant(AGENT, { type: "text", text: "The files are a.txt and b.txt." }));
    expect(s.task(notification())).toEqual([
      { type: "tool_completed", itemId: AGENT, ok: true, output: "The files are a.txt and b.txt.", ms: 3568 },
    ]);
  });

  test("without any words of its own, the summary stands in; a failure is not ok", () => {
    const s = createSubagents("/w");
    s.task(started(false));
    expect(s.task(notification({ status: "failed", summary: "ran out of turns" }))).toEqual([
      { type: "tool_completed", itemId: AGENT, ok: false, output: "ran out of turns", ms: 3568 },
    ]);
  });

  test("the call's own tool_result is left alone once the task runs it, foreground or background", () => {
    for (const background of [false, true]) {
      const s = createSubagents("/w");
      expect(s.holds(AGENT)).toBe(false);
      s.task(started(background));
      expect(s.holds(AGENT)).toBe(true);
    }
  });

  test("a background shell or monitor is no subagent: its result completes it as before", () => {
    const s = createSubagents("/w");
    expect(s.task(started(true, { task_type: "local_bash", subagent_type: undefined, tool_use_id: "toolu_sh" }))).toEqual([]);
    expect(s.holds("toolu_sh")).toBe(false);
    expect(s.task(notification({ tool_use_id: "toolu_sh" }))).toEqual([]);
  });

  test("a notification with no tool call, or for an unknown one, is dropped", () => {
    const s = createSubagents("/w");
    expect(s.task(notification({ tool_use_id: undefined }))).toEqual([]);
    expect(s.task(notification())).toEqual([]);
  });
});
