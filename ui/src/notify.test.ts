import { describe, expect, test } from "bun:test";
import { HINT, MIN_TURN_MS, StatusWatcher, plainLine, shouldNotify, statusNotice, noticeText, switchState } from "./notify";
import type { SessionMeta } from "./protocol";

const on = { enabled: true, focused: false, openSessionId: null as string | null };

describe("statusNotice", () => {
  test("a session that now waits for an approval needs the user", () => {
    expect(statusNotice("running", "needs_approval")).toBe("approval");
  });
  test("a turn that ends is finished", () => {
    expect(statusNotice("running", "idle")).toBe("finished");
    expect(statusNotice("needs_approval", "idle")).toBe("finished");
  });
  test("a failure is reported once", () => {
    expect(statusNotice("running", "error")).toBe("failed");
    expect(statusNotice("error", "error")).toBeNull();
  });
  test("nothing else is worth interrupting for", () => {
    expect(statusNotice("idle", "running")).toBeNull();
    expect(statusNotice("queued", "running")).toBeNull();
    expect(statusNotice("idle", "idle")).toBeNull();
    expect(statusNotice("error", "idle")).toBeNull();
    expect(statusNotice("needs_approval", "needs_approval")).toBeNull();
  });
  test("a session seen for the first time is not news", () => {
    expect(statusNotice(undefined, "idle")).toBeNull();
    expect(statusNotice(undefined, "needs_approval")).toBeNull();
  });
});

describe("shouldNotify", () => {
  test("off means never", () => {
    expect(shouldNotify({ ...on, enabled: false }, "s1")).toBe(false);
  });
  test("an unfocused window always gets it, even on that session", () => {
    expect(shouldNotify({ ...on, focused: false, openSessionId: "s1" }, "s1")).toBe(true);
  });
  test("a focused window showing that session does not", () => {
    expect(shouldNotify({ ...on, focused: true, openSessionId: "s1" }, "s1")).toBe(false);
  });
  test("a focused window on another page or session does", () => {
    expect(shouldNotify({ ...on, focused: true, openSessionId: "s2" }, "s1")).toBe(true);
    expect(shouldNotify({ ...on, focused: true, openSessionId: null }, "s1")).toBe(true);
  });
  test("a note that names no session is never hidden by the open one", () => {
    expect(shouldNotify({ ...on, focused: true, openSessionId: "s1" }, null)).toBe(true);
  });
});

const meta = (id: string, status: SessionMeta["status"]): SessionMeta =>
  ({ id, title: `Task ${id}`, engine: "codex", model: "m", effort: "low", cwd: "/x", status, host: "local", createdAt: 0, updatedAt: 0 }) as SessionMeta;

describe("StatusWatcher", () => {
  test("a snapshot sets the baseline without notifying", () => {
    const w = new StatusWatcher();
    w.seed([meta("a", "running"), meta("b", "needs_approval")]);
    expect(w.change(meta("b", "needs_approval"))).toBeNull();
    expect(w.change(meta("a", "idle"))).toBe("finished");
  });
  test("each change is judged against the one before", () => {
    const w = new StatusWatcher();
    w.seed([meta("a", "idle")]);
    expect(w.change(meta("a", "running"))).toBeNull();
    expect(w.change(meta("a", "needs_approval"))).toBe("approval");
    expect(w.change(meta("a", "running"))).toBeNull();
    expect(w.change(meta("a", "error"))).toBe("failed");
  });
  test("a host reload replaces only that host's baseline", () => {
    const w = new StatusWatcher();
    w.seed([meta("a", "running")]);
    w.seed([{ ...meta("h:1", "running"), host: "h" }], "h");
    // `a` survives the reload of host h; h:1 restarts from its new snapshot.
    expect(w.change(meta("a", "idle"))).toBe("finished");
    expect(w.change({ ...meta("h:1", "running"), host: "h" })).toBeNull();
  });
  test("a failed turn ends idle but is reported as a failure", () => {
    const w = new StatusWatcher();
    w.seed([meta("a", "running")]);
    w.turnEnded("a", "failed");
    expect(w.change(meta("a", "idle"))).toBe("failed");
    // The verdict is for that turn only.
    expect(w.change(meta("a", "running"))).toBeNull();
    w.turnEnded("a", "completed");
    expect(w.change(meta("a", "idle"))).toBe("finished");
  });
  test("a turn the user stopped is not reported", () => {
    const w = new StatusWatcher();
    w.seed([meta("a", "running")]);
    w.turnEnded("a", "interrupted");
    expect(w.change(meta("a", "idle"))).toBeNull();
  });
  test("a quick turn is not worth a banner; a long one is", () => {
    let t = 0;
    const w = new StatusWatcher(() => t);
    w.seed([meta("a", "idle")]);
    w.event("a", { type: "turn_started" });
    w.change(meta("a", "running"));
    t = MIN_TURN_MS - 1;
    w.event("a", { type: "turn_completed", status: "completed" });
    expect(w.change(meta("a", "idle"))).toBeNull();
    w.event("a", { type: "turn_started" });
    w.change(meta("a", "running"));
    t += MIN_TURN_MS;
    w.event("a", { type: "assistant_message", itemId: "m", text: "All **green**." });
    w.event("a", { type: "turn_completed", status: "completed" });
    expect(w.change(meta("a", "idle"))).toBe("finished");
    expect(w.info("a")).toEqual({ startedAt: MIN_TURN_MS - 1, ms: MIN_TURN_MS, answer: "All **green**." });
  });
  test("approvals and failures are reported however quick", () => {
    const w = new StatusWatcher(() => 0);
    w.seed([meta("a", "idle")]);
    w.event("a", { type: "turn_started" });
    w.change(meta("a", "running"));
    w.event("a", { type: "approval_request", requestId: "r", kind: "command", title: "Run a command", detail: "npm test" });
    expect(w.change(meta("a", "needs_approval"))).toBe("approval");
    expect(w.info("a").approval).toEqual({ title: "Run a command", detail: "npm test" });
    w.change(meta("a", "running"));
    w.event("a", { type: "error", message: "rate limited" });
    w.event("a", { type: "turn_completed", status: "failed" });
    expect(w.change(meta("a", "idle"))).toBe("failed");
    expect(w.info("a").error).toBe("rate limited");
  });
  test("an automation's run reports a failure or an approval, not a finish", () => {
    const run = (status: SessionMeta["status"]) => ({ ...meta("r", status), automation: "auto1" });
    const w = new StatusWatcher(() => 0);
    w.seed([run("running")]);
    expect(w.change(run("idle"))).toBeNull();
    w.change(run("running"));
    expect(w.change(run("needs_approval"))).toBe("approval");
    w.change(run("running"));
    w.turnEnded("r", "failed");
    expect(w.change(run("idle"))).toBe("failed");
  });
  test("a deleted session is forgotten", () => {
    const w = new StatusWatcher();
    w.seed([meta("a", "running")]);
    w.forget("a");
    expect(w.change(meta("a", "idle"))).toBeNull();
  });
});

describe("noticeText", () => {
  test("names the session and what it needs", () => {
    expect(noticeText("approval", "Fix login").title).toBe("Fix login");
    expect(noticeText("approval", "Fix login").body).toBe("Needs your approval");
    expect(noticeText("finished", "Fix login").body).toBe("Done");
    // Claude's AskUserQuestion: a question, not an approval.
    const w = new StatusWatcher(() => 0);
    w.event("q", { type: "approval_request", requestId: "r", kind: "question", title: "Which **database**?", detail: "and 1 more" });
    expect(noticeText("approval", "Pick a stack", w.info("q")).body).toBe("Has a question · Which database?");
    expect(noticeText("failed", "Fix login").body).toBe("Failed");
  });
  test("says what the turn did", () => {
    expect(noticeText("finished", "Fix login", { ms: 72_000, answer: "Fixed the **login**. Tests pass." }).body).toBe(
      "Done in 1m 12s · Fixed the login. Tests pass.",
    );
    expect(noticeText("approval", "Fix login", { approval: { title: "Run a command", detail: "npm test" } }).body).toBe(
      "Needs your approval · Run a command: npm test",
    );
    expect(noticeText("failed", "Fix login", { ms: 5_000, error: "rate limited" }).body).toBe("Failed after 5s · rate limited");
  });
  test("an untitled session still gets a title", () => {
    expect(noticeText("finished", "").title).toBe("Switchyard");
  });
});

describe("plainLine", () => {
  test("Markdown becomes one plain line", () => {
    expect(plainLine("## Summary\n\n- one `x`\n- see [docs](https://x.y)\n```ts\nlet a\n```")).toBe("Summary one x see docs let a");
  });
  test("long text is cut with an ellipsis", () => {
    const line = plainLine("word ".repeat(100), 20);
    expect(line.length).toBe(20);
    expect(line.endsWith("…")).toBe(true);
  });
});

describe("switchState", () => {
  test("off when the user turned them off", () => {
    expect(switchState(false, "granted")).toEqual({ on: false, hint: null });
  });
  test("on when turned on and the browser allows them", () => {
    expect(switchState(true, "granted")).toEqual({ on: true, hint: null });
  });
  test("the desktop app answers asynchronously: trust the stored choice", () => {
    expect(switchState(true, null)).toEqual({ on: true, hint: null });
  });
  test("permission revoked in the browser after turning them on: off, and says why", () => {
    expect(switchState(true, "denied")).toEqual({ on: false, hint: HINT.blocked });
  });
  test("permission reset to ask: off, so a click asks again", () => {
    expect(switchState(true, "default")).toEqual({ on: false, hint: null });
  });
  test("a browser without notifications cannot be on", () => {
    expect(switchState(true, "unsupported")).toEqual({ on: false, hint: HINT.unsupported });
  });
});
