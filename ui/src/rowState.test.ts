import { describe, expect, test } from "bun:test";
import type { SessionMeta } from "./protocol";
import { ROW_LABEL, markSeen, rowState } from "./rowState";

const meta = (patch: Partial<SessionMeta>): SessionMeta =>
  ({ id: "s", title: "t", engine: "codex", model: "m", effort: "low", cwd: "/x", status: "idle", host: "local", createdAt: 0, updatedAt: 0, ...patch }) as SessionMeta;

describe("rowState", () => {
  test("live states come from the status", () => {
    expect(rowState(meta({ status: "running" }), 0, true)).toBe("working");
    expect(rowState(meta({ status: "needs_approval" }), 0, true)).toBe("approval");
    expect(rowState(meta({ status: "queued" }), 0, true)).toBe("queued");
    // Its turn ended on a request for a service: it waits on the owner, like an approval.
    const asked = { requestId: "r1", service: "Linear", at: 1 };
    expect(rowState(meta({ needsConnector: asked }), 0, true)).toBe("connector");
    expect(rowState(meta({ status: "running", needsConnector: asked }), 0, true)).toBe("working");
    expect(rowState(meta({ needsConnector: asked }), 0, false)).toBe("offline");
    expect(ROW_LABEL.connector).toBe("Needs connector");
    expect(rowState(meta({ status: "error" }), 0, true)).toBe("failed");
  });
  test("a turn that ended after the user last looked is answered until opened", () => {
    const s = meta({ turnEndedAt: 200, lastTurn: "completed" });
    expect(rowState(s, 100, true)).toBe("answered");
    expect(rowState(s, 200, true)).toBe("idle");
  });
  test("an unseen failed turn shows as failed; a stopped one does not stand out", () => {
    expect(rowState(meta({ turnEndedAt: 200, lastTurn: "failed" }), 100, true)).toBe("failed");
    expect(rowState(meta({ turnEndedAt: 200, lastTurn: "interrupted" }), 100, true)).toBe("idle");
  });
  test("a session from an older yardd has no turn end: idle", () => {
    expect(rowState(meta({}), 0, true)).toBe("idle");
  });
  test("offline wins over everything", () => {
    expect(rowState(meta({ status: "running" }), 0, false)).toBe("offline");
  });
  test("every state has a label", () => {
    for (const k of ["working", "approval", "answered", "failed", "queued", "idle", "offline"] as const) expect(ROW_LABEL[k]).toBeTruthy();
  });
});

describe("markSeen", () => {
  test("records the turn end the user has seen, dropping sessions that are gone", () => {
    const seen = markSeen({ old: 5, s: 1 }, meta({ turnEndedAt: 200 }), new Set(["s"]));
    expect(seen).toEqual({ s: 200 });
  });
  test("nothing to record leaves the map as it is", () => {
    const before = { s: 200 };
    expect(markSeen(before, meta({ turnEndedAt: 200 }), new Set(["s"]))).toBe(before);
    expect(markSeen(before, meta({}), new Set(["s"]))).toBe(before);
  });
});


describe("manual unread", () => {
  test("marks stopped, failed and older chats unread independently of the turn end", () => {
    for (const patch of [{}, { turnEndedAt: 200, lastTurn: "completed" }, { turnEndedAt: 200, lastTurn: "interrupted" }, { status: "error" }] as Partial<SessionMeta>[]) {
      expect(rowState(meta(patch), 500, true, true)).toBe("unread");
    }
  });
  test("live states and offline status retain priority", () => {
    expect(rowState(meta({ status: "running" }), 500, true, true)).toBe("working");
    expect(rowState(meta({ status: "needs_approval" }), 500, true, true)).toBe("approval");
    expect(rowState(meta({ status: "queued" }), 500, true, true)).toBe("queued");
    expect(rowState(meta({}), 500, false, true)).toBe("offline");
  });
  test("opening a manually unread chat clears the marker, even without a turn end", () => {
    expect(markSeen({ s: -1 }, meta({ turnEndedAt: 200 }), new Set(["s"]))).toEqual({ s: 200 });
    expect(markSeen({ s: -1 }, meta({}), new Set(["s"]))).toEqual({ s: 0 });
    expect(ROW_LABEL.unread).toBe("Unread");
  });
});

test("a session whose turn ended with a subagent still at work reads as working", () => {
  const s = { id: "s", title: "t", engine: "claude", model: "m", effort: "", cwd: "/w", status: "idle", host: "local", createdAt: 0, updatedAt: 0, turnEndedAt: 5, lastTurn: "completed", background: ["tool-1"] } as any;
  expect(rowState(s, 10, true)).toBe("working");
  expect(rowState({ ...s, background: [] }, 10, true)).not.toBe("working");
});
