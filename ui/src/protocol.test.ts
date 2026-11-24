import { describe, expect, test } from "bun:test";
import { applyEvent, type AgentEvent, type Item } from "./protocol";

/** Fold events in order, numbering them as yardd does. */
const fold = (events: AgentEvent[], items: Item[] = [], first = 0): Item[] =>
  events.reduce((acc, ev, i) => applyEvent(acc, ev, first + i), items);

describe("applyEvent", () => {
  test("a command that started before a steered message still completes after it", () => {
    // Steering happens at a step's start, so the completion arrives after the steered message.
    const items = fold([
      { type: "user_message", text: "run the steps" },
      { type: "turn_started", at: 1_000 },
      { type: "tool_started", itemId: "c1", kind: "command", title: "Ran", detail: "step 1" },
      { type: "user_message", text: "also this", steered: true, at: 1_500 },
      { type: "tool_completed", itemId: "c1", ok: true, output: "one" },
      { type: "turn_completed", status: "completed", at: 2_000 },
    ]);
    const tool = items.find((it) => it.kind === "tool");
    expect(tool).toMatchObject({ state: "ok", output: "one" });
  });

  test("a message's origin rides on its item; the owner's own carries none", () => {
    const origin = { kind: "agent", agentId: "g1", name: "Builder", hops: 1 } as const;
    const items = fold([
      { type: "user_message", text: "check #12", origin },
      { type: "user_message", text: "mine", origin: { kind: "user" } },
      { type: "user_message", text: "also mine" },
    ]);
    expect(items[0]).toEqual({ kind: "user", key: "e0", text: "check #12", origin });
    expect(items[1]).toEqual({ kind: "user", key: "e1", text: "mine" });
    expect(items[2]).toEqual({ kind: "user", key: "e2", text: "also mine" });
  });

  test("an id reused in a later turn makes a new item under its own question", () => {
    // A restarted Claude sidecar used to count its ids from 1 again.
    const items = fold([
      { type: "user_message", text: "first" },
      { type: "assistant_message", itemId: "text-1", text: "Not logged in" },
      { type: "turn_completed", status: "completed" },
      { type: "user_message", text: "second" },
      { type: "assistant_delta", itemId: "text-1", text: "Hello" },
      { type: "assistant_message", itemId: "text-1", text: "Hello there" },
    ]);
    expect(items.map((it) => (it.kind === "user" || it.kind === "assistant" ? `${it.kind}:${it.text}` : it.kind))).toEqual([
      "user:first",
      "assistant:Not logged in",
      "user:second",
      "assistant:Hello there",
    ]);
  });

  test("thinking streams into one item and settles with the turn", () => {
    const items = fold([
      { type: "user_message", text: "why" },
      { type: "reasoning_delta", itemId: "r1", text: "Looking at " },
      { type: "reasoning_delta", itemId: "r1", text: "the code" },
      { type: "assistant_delta", itemId: "m1", text: "Because" },
      { type: "turn_completed", status: "completed" },
    ]);
    expect(items[1]).toEqual({ kind: "reasoning", key: "e1", itemId: "r1", text: "Looking at the code", streaming: false });
    // An empty final block (thinking omitted) adds nothing.
    expect(fold([{ type: "reasoning", itemId: "r2", text: "" }])).toEqual([]);
  });

  test("deltas accumulate on one item, then the final message settles it", () => {
    const items = fold([
      { type: "user_message", text: "hi" },
      { type: "turn_started" },
      { type: "assistant_delta", itemId: "m1", text: "Hel" },
      { type: "assistant_delta", itemId: "m1", text: "lo" },
    ]);
    expect(items).toEqual([
      { kind: "user", key: "e0", text: "hi" },
      { kind: "turn", key: "e1", start: undefined, status: "running" },
      { kind: "assistant", key: "e2", itemId: "m1", text: "Hello", streaming: true },
    ]);

    // The final text wins over what was streamed, and the item keeps its key.
    const done = applyEvent(items, { type: "assistant_message", itemId: "m1", text: "Hello!" }, 4);
    expect(done[2]).toEqual({ kind: "assistant", key: "e2", itemId: "m1", text: "Hello!", streaming: false });
    expect(done).toHaveLength(3);
  });

  test("a final message with no deltas is its own item", () => {
    const items = fold([{ type: "assistant_message", itemId: "m9", text: "Done." }]);
    expect(items).toEqual([{ kind: "assistant", key: "e0", itemId: "m9", text: "Done.", streaming: false }]);
  });

  test("deltas of two items stay apart", () => {
    const items = fold([
      { type: "assistant_delta", itemId: "a", text: "one" },
      { type: "assistant_delta", itemId: "b", text: "two" },
      { type: "assistant_delta", itemId: "a", text: "+" },
    ]);
    expect(items.map((i) => (i.kind === "assistant" ? i.text : ""))).toEqual(["one+", "two"]);
  });

  test("does not mutate the list it was given", () => {
    const before = fold([{ type: "assistant_delta", itemId: "m", text: "a" }]);
    const snapshot = structuredClone(before);
    applyEvent(before, { type: "assistant_delta", itemId: "m", text: "b" }, 1);
    applyEvent(before, { type: "turn_completed", status: "completed" }, 1);
    expect(before).toEqual(snapshot);
  });

  test("a tool starts running and completes ok or failed", () => {
    const started = fold([
      { type: "tool_started", itemId: "t1", kind: "command", title: "Ran", detail: "ls" },
      { type: "tool_started", itemId: "t2", kind: "command", title: "Ran", detail: "false" },
    ]);
    expect(started[0]).toMatchObject({ kind: "tool", itemId: "t1", toolKind: "command", detail: "ls", state: "running" });

    const done = fold(
      [
        { type: "tool_completed", itemId: "t1", ok: true, output: "a\nb\n" },
        { type: "tool_completed", itemId: "t2", ok: false },
      ],
      started,
      2,
    );
    expect(done[0]).toMatchObject({ state: "ok", output: "a\nb\n" });
    expect(done[1]).toMatchObject({ state: "failed", output: null });
  });

  test("an approval is asked, then resolved", () => {
    const asked = fold([{ type: "approval_request", requestId: "r1", kind: "command", title: "rm x", detail: "why" }]);
    expect(asked).toEqual([
      { kind: "approval", key: "e0", requestId: "r1", approvalKind: "command", title: "rm x", detail: "why", decision: null },
    ]);
    const resolved = applyEvent(asked, { type: "approval_resolved", requestId: "r1", decision: "allow_always" }, 1);
    expect(resolved[0]).toMatchObject({ decision: "allow_always" });
    // A resolve for another request changes nothing.
    expect(applyEvent(asked, { type: "approval_resolved", requestId: "r2", decision: "deny" }, 1)).toEqual(asked);
  });

  test("turn end closes whatever is still live", () => {
    const live = fold([
      { type: "assistant_delta", itemId: "m", text: "part" },
      { type: "tool_started", itemId: "t", kind: "command", title: "Ran", detail: "sleep 9" },
      { type: "approval_request", requestId: "r", kind: "edit", title: "Apply", detail: "" },
      { type: "tool_started", itemId: "t2", kind: "command", title: "Ran", detail: "true" },
      { type: "tool_completed", itemId: "t2", ok: true },
    ]);
    const ended = applyEvent(live, { type: "turn_completed", status: "interrupted" }, 5);
    expect(ended[0]).toMatchObject({ kind: "assistant", text: "part", streaming: false });
    expect(ended[1]).toMatchObject({ kind: "tool", state: "failed" });
    // Nobody answered it: closed, but not recorded as a denial.
    expect(ended[2]).toMatchObject({ kind: "approval", decision: null, unanswered: true });
    // A tool that already finished stays ok.
    expect(ended[3]).toMatchObject({ kind: "tool", state: "ok" });
    expect(ended).toHaveLength(4);
  });

  test("file changes become a diff item, unless there are none", () => {
    const files = [{ path: "src/lib.rs", diff: "+x", kind: "update" }];
    expect(fold([{ type: "file_change", itemId: "f", files }])).toEqual([{ kind: "diff", key: "e0", files }]);
    expect(fold([{ type: "file_change", itemId: "f", files: [] }])).toEqual([]);
  });

  test("queued, note and error lines", () => {
    const items = fold([
      { type: "user_message", text: "task" },
      { type: "queued", reason: "load 9.0 is above 8 cores", text: "task" },
      { type: "note", text: "Handed off from Codex." },
      { type: "error", message: "boom" },
    ]);
    expect(items.slice(1)).toEqual([
      { kind: "queued", key: "e1", reason: "load 9.0 is above 8 cores" },
      { kind: "note", key: "e2", text: "Handed off from Codex." },
      { kind: "error", key: "e3", message: "boom" },
    ]);
  });

  test("a prompt keeps its attachments, and may have no text", () => {
    const shot = { id: "a1", name: "shot.png", mime: "image/png", size: 300, path: "/d/attachments/a1/shot.png" };
    const items = fold([
      { type: "user_message", text: "look", attachments: [shot] },
      { type: "user_message", text: "", attachments: [shot] },
      { type: "user_message", text: "plain", attachments: [] },
    ]);
    expect(items).toEqual([
      { kind: "user", key: "e0", text: "look", attachments: [shot] },
      { kind: "user", key: "e1", text: "", attachments: [shot] },
      { kind: "user", key: "e2", text: "plain" },
    ]);
  });

  test("a turn's span is kept; unknown events add nothing", () => {
    expect(fold([{ type: "turn_started", at: 1000 }, { type: "turn_completed", status: "completed", at: 4000 }])).toEqual([
      { kind: "turn", key: "e0", start: 1000, end: 4000, status: "completed" },
    ]);
    expect(applyEvent([], { type: "mystery" } as unknown as AgentEvent, 0)).toEqual([]);
  });

  test("a subagent's tool call carries its Agent call's id; its end carries a duration", () => {
    const items = fold([
      { type: "user_message", text: "go" },
      { type: "tool_started", itemId: "a", kind: "other", title: "Agent", detail: "d", agent: "Explore" },
      { type: "tool_started", itemId: "c", parent: "a", kind: "read", title: "Read", detail: "x" },
      { type: "tool_completed", itemId: "c", ok: true },
      { type: "tool_completed", itemId: "a", ok: true, output: "report", ms: 3568 },
    ]);
    expect(items.filter((i) => i.kind === "tool").map((i) => (i.kind === "tool" ? [i.itemId, i.agent, i.parent, i.ms, i.state] : []))).toEqual([
      ["a", "Explore", undefined, 3568, "ok"],
      ["c", undefined, "a", undefined, "ok"],
    ]);
  });

  test("a background subagent outlives a completed turn; Stop or a failure ends it", () => {
    const run: AgentEvent[] = [
      { type: "user_message", text: "go" },
      { type: "tool_started", itemId: "a", kind: "other", title: "Agent", detail: "d", agent: "Explore" },
      { type: "tool_started", itemId: "c", parent: "a", kind: "command", title: "Ran", detail: "ls" },
      { type: "tool_started", itemId: "other", kind: "command", title: "Ran", detail: "ls" },
    ];
    const states = (events: AgentEvent[]) => fold(events).flatMap((i) => (i.kind === "tool" ? [`${i.itemId}:${i.state}`] : []));
    expect(states([...run, { type: "turn_completed", status: "completed" }])).toEqual(["a:running", "c:running", "other:failed"]);
    expect(states([...run, { type: "turn_completed", status: "interrupted" }])).toEqual(["a:failed", "c:failed", "other:failed"]);
    expect(states([...run, { type: "turn_completed", status: "failed" }])).toEqual(["a:failed", "c:failed", "other:failed"]);
  });

  test("an agent's end is matched after a later prompt too: its notification can come late", () => {
    const items = fold([
      { type: "user_message", text: "go" },
      { type: "tool_started", itemId: "a", kind: "other", title: "Agent", detail: "d", agent: "Explore" },
      { type: "turn_completed", status: "completed" },
      { type: "user_message", text: "and meanwhile?" },
      { type: "tool_completed", itemId: "a", ok: true, output: "found it", ms: 9000 },
    ]);
    const agent = items.find((i) => i.kind === "tool");
    expect(agent).toMatchObject({ state: "ok", output: "found it", ms: 9000 });
  });
});

describe("a call that waits for an approval", () => {
  const asked: AgentEvent[] = [
    { type: "user_message", text: "[cmd] run it" },
    { type: "turn_started", at: 1_000 },
    { type: "tool_started", itemId: "c1", kind: "command", title: "Ran", detail: "echo hi" },
    { type: "approval_request", requestId: "r1", kind: "command", title: "echo hi", detail: "", itemId: "c1" },
  ];
  const tool = (items: Item[]) => items.find((it) => it.kind === "tool");

  test("is waiting, not running, until someone answers", () => {
    expect(tool(fold(asked))).toMatchObject({ state: "running", asked: "waiting" });
    const allowed = fold([...asked, { type: "approval_resolved", requestId: "r1", decision: "allow" }]);
    expect(tool(allowed)?.kind === "tool" && tool(allowed)).not.toHaveProperty("asked");
  });

  test("denied, it did not run: denied, not failed", () => {
    const items = fold([
      ...asked,
      { type: "approval_resolved", requestId: "r1", decision: "deny" },
      { type: "tool_completed", itemId: "c1", ok: false, note: "declined" },
      { type: "turn_completed", status: "completed", at: 2_000 },
    ]);
    expect(tool(items)).toMatchObject({ state: "failed", asked: "denied" });
  });

  test("an approval that comes before its call still links to it", () => {
    const items = fold([
      { type: "user_message", text: "[cmd] run it" },
      { type: "approval_request", requestId: "r1", kind: "command", title: "echo hi", detail: "", itemId: "c1" },
      { type: "tool_started", itemId: "c1", kind: "command", title: "Ran", detail: "echo hi" },
    ]);
    expect(tool(items)).toMatchObject({ asked: "waiting" });
  });

  test("a turn that ends before anyone answers leaves it unanswered", () => {
    const items = fold([...asked, { type: "turn_completed", status: "interrupted", at: 2_000 }]);
    expect(tool(items)).toMatchObject({ state: "failed", asked: "unanswered" });
  });

  // ACP engines fail the asked call on Stop before the turn ends, so the call
  // is no longer running when turn_completed arrives.
  test("a call the engine failed while it waited is still unanswered at turn end", () => {
    const items = fold([
      ...asked,
      { type: "tool_completed", itemId: "c1", ok: false },
      { type: "turn_completed", status: "interrupted", at: 2_000 },
    ]);
    expect(tool(items)).toMatchObject({ state: "failed", asked: "unanswered" });
  });

  test("an approval without an itemId changes no call", () => {
    const items = fold([...asked.slice(0, 3), { type: "approval_request", requestId: "r1", kind: "command", title: "echo hi", detail: "" }]);
    expect(tool(items)).not.toHaveProperty("asked");
  });
});
