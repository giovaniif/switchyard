import { describe, expect, test } from "bun:test";
import type { Automation, SessionMeta } from "./protocol";
import { describeSchedule, describeNext, describeLast, runsOf, hiddenFromSidebar, runsAttention, scheduleProblem, runTime, runOutcome, RUN_LABEL, runSummary, runTag, runLabel, stripRuns, scheduleLine, scheduleParts, cardStatus } from "./automations";
import * as A from "./automations";
import { initial, reduce } from "./store";

const MIN = 60_000;
const at = (h: number, m = 0, day = 16) => new Date(2026, 10, day, h, m).getTime(); // 16 Nov 2026 is a Monday

describe("the schedule in words", () => {
  test("intervals", () => {
    expect(describeSchedule({ kind: "interval", every: 1, unit: "minutes" })).toBe("Every minute");
    expect(describeSchedule({ kind: "interval", every: 30, unit: "minutes" })).toBe("Every 30 minutes");
    expect(describeSchedule({ kind: "interval", every: 1, unit: "hours" })).toBe("Every hour");
    expect(describeSchedule({ kind: "interval", every: 6, unit: "hours" })).toBe("Every 6 hours");
  });
  test("active windows name hours and start days compactly", () => {
    const s = { kind: "interval", every: 1, unit: "hours" } as const;
    expect(describeSchedule(s, { start: "21:00", end: "09:00" })).toBe("Every hour · 21:00–09:00");
    expect(describeSchedule(s, { start: "21:00", end: "09:00", days: [5, 3, 1, 4, 2] })).toBe("Every hour · 21:00–09:00, Mon–Fri");
    expect(describeSchedule(s, { start: "09:00", end: "17:00", days: [1, 3, 5] })).toBe("Every hour · 09:00–17:00, Mon, Wed, Fri");
    expect(describeSchedule(s, { start: "21:00", end: "09:00", days: [0, 6] })).toBe("Every hour · 21:00–09:00, Sat–Sun");
    expect(describeSchedule(s, { start: "21:00", end: "09:00", days: [0,1,2,3,4,5,6] })).toBe("Every hour · 21:00–09:00");
    expect(describeSchedule(s, null)).toBe("Every hour");
  });
  test("window validation agrees with the daemon", () => {
    const s = { kind: "interval", every: 1, unit: "hours" } as const;
    for (const window of [{ start: "9:00", end: "17:00" }, { start: "09:00", end: "09:00" }, { start: "09:00", end: "17:00", days: [1,1] }, { start: "09:00", end: "17:00", days: [7] }, { start: "09:00", end: "17:00", days: [] }]) expect(scheduleProblem(s, window)).not.toBeNull();
    expect(scheduleProblem(s, { start: "21:00", end: "09:00" })).toBeNull();
    expect(scheduleProblem({ kind: "manual" }, { start: "21:00", end: "09:00" })).toContain("interval");
  });
  test("daily and weekly name the time, and weekly the day", () => {
    expect(describeSchedule({ kind: "daily", time: "09:30" })).toBe("Daily at 09:30");
    expect(describeSchedule({ kind: "weekly", day: 1, time: "08:00" })).toBe("Mondays at 08:00");
    expect(describeSchedule({ kind: "weekly", day: 0, time: "18:15" })).toBe("Sundays at 18:15");
  });
  test("a schedule that cannot run is explained", () => {
    expect(scheduleProblem({ kind: "interval", every: 0, unit: "minutes" })).toContain("at least 1");
    expect(scheduleProblem({ kind: "daily", time: "25:00" })).toContain("time");
    expect(scheduleProblem({ kind: "weekly", day: 9, time: "10:00" })).toContain("day");
    expect(scheduleProblem({ kind: "daily", time: "07:05" })).toBeNull();
  });
  test("a manual schedule runs only when started", () => {
    expect(describeSchedule({ kind: "manual" })).toBe("Manual");
    expect(scheduleProblem({ kind: "manual" })).toBeNull();
  });
});

describe("next run", () => {
  const now = at(10, 0);
  test("soon is relative", () => {
    expect(describeNext(now + 30_000, true, now)).toBe("in under a minute");
    expect(describeNext(now + 12 * MIN, true, now)).toBe("in 12 min");
    expect(describeNext(now + 60 * MIN, true, now)).toBe("in 1 h");
    expect(describeNext(now + 125 * MIN, true, now)).toBe("in 2 h 5 min");
  });
  test("later today, tomorrow and further out say when", () => {
    expect(describeNext(at(18, 30), true, now)).toBe("today 18:30");
    expect(describeNext(at(9, 0, 17), true, now)).toBe("tomorrow 09:00");
    expect(describeNext(at(9, 0, 19), true, now)).toBe("Thu 09:00");
    expect(describeNext(at(9, 0, 30), true, now)).toBe("30 Nov 09:00");
  });
  test("a late run and a disabled automation", () => {
    expect(describeNext(now - 5 * MIN, true, now)).toBe("due now");
    expect(describeNext(null, false, now)).toBe("off");
    expect(describeNext(now + 5 * MIN, false, now)).toBe("off");
  });
});

describe("last run", () => {
  const now = at(10, 0);
  test("never, started, skipped", () => {
    expect(describeLast(undefined, now)).toBe("never");
    expect(describeLast({ at: now - 5 * MIN, outcome: "started", sessionId: "s1" }, now)).toBe("5 min ago");
    expect(describeLast({ at: now - 3 * 3600_000, outcome: "skipped", reason: "a run is still going" }, now)).toBe("skipped 3 h ago");
    expect(describeLast({ at: now - 20_000, outcome: "failed", reason: "x" }, now)).toBe("failed just now");
  });
});

const run = (id: string, automation: string | null, over: Partial<SessionMeta> = {}): SessionMeta => ({
  id,
  title: id,
  engine: "codex",
  model: "m",
  effort: "low",
  cwd: "/w",
  status: "idle",
  host: "local",
  automation,
  createdAt: 1,
  updatedAt: 1,
  ...over,
});

describe("runs", () => {
  test("an automation's runs are newest first and stay on their host", () => {
    const sessions = {
      a: run("a", "x", { createdAt: 10 }),
      b: run("b", "x", { createdAt: 30 }),
      c: run("c", "y", { createdAt: 20 }),
      d: run("h~d", "x", { createdAt: 40, host: "h" }),
      e: run("e", null, { createdAt: 50 }),
    };
    expect(runsOf(sessions, "local", "x").map((s) => s.id)).toEqual(["b", "a"]);
    expect(runsOf(sessions, "h", "x").map((s) => s.id)).toEqual(["h~d"]);
  });
  test("runs stay out of the sidebar unless pinned", () => {
    expect(hiddenFromSidebar(run("a", "x"))).toBe(true);
    expect(hiddenFromSidebar(run("a", "x", { archived: true }))).toBe(true);
    expect(hiddenFromSidebar(run("a", "x", { pinned: true }))).toBe(false);
    expect(hiddenFromSidebar(run("a", null))).toBe(false);
  });
  test("the Automations button's dot: a run waiting for approval outranks a working one", () => {
    expect(runsAttention([])).toBe(null);
    expect(runsAttention([run("a", "x", { status: "idle" }), run("b", "x", { status: "queued" })])).toBe(null);
    expect(runsAttention([run("a", "x", { status: "running" })])).toBe("working");
    expect(runsAttention([run("a", "x", { status: "needs_approval" })])).toBe("approval");
    expect(runsAttention([run("a", "x", { status: "running" }), run("b", "y", { status: "needs_approval" })])).toBe("approval");
  });
});

const auto = (id: string): Automation => ({
  id,
  name: id,
  prompt: "p",
  cwd: "/w",
  engine: "codex",
  model: "m",
  effort: "low",
  permission: "ask",
  base: "",
  schedule: { kind: "daily", time: "09:00" },
  skipWhileRunning: true,
  maxFiveHour: null,
  maxWeek: null,
  enabled: true,
  createdAt: 1,
  nextRun: 5,
});

describe("automations in the store", () => {
  test("each host keeps its own list", () => {
    let s = reduce(initial, { t: "server", msg: { op: "snapshot", sessions: [], automations: [auto("a")] } });
    expect(s.automations.local.map((a) => a.id)).toEqual(["a"]);
    s = reduce(s, { t: "server", msg: { op: "automations", hostId: "h", automations: [auto("b")] } });
    expect(s.automations.h.map((a) => a.id)).toEqual(["b"]);
    expect(s.automations.local.map((a) => a.id)).toEqual(["a"]);
    s = reduce(s, { t: "server", msg: { op: "automations", automations: [] } });
    expect(s.automations.local).toEqual([]);
    expect(s.automations.h).toHaveLength(1);
  });
  test("a host snapshot carries the host's list, and an old host has none", () => {
    let s = reduce(initial, { t: "server", msg: { op: "host.snapshot", hostId: "h", sessions: [], automations: [auto("b")] } });
    expect(s.automations.h).toHaveLength(1);
    s = reduce(s, { t: "server", msg: { op: "host.snapshot", hostId: "h", sessions: [] } });
    expect(s.automations.h).toEqual([]);
  });
});

describe("the card at a glance", () => {
  const now = at(10, 0);
  const a = (over: Partial<Automation> = {}): Automation => ({ ...auto("x"), name: "Loop patrol", ...over });
  const r = (id: string, over: Partial<SessionMeta> = {}) => run(id, "x", { title: `Loop patrol · ${id}`, ...over });

  test("a run is named by its time, with the day once it is not today, and the date after a week", () => {
    expect(runTime(at(4, 38), now)).toBe("04:38");
    expect(runTime(at(3, 38, 15), now)).toBe("Sun 03:38");
    expect(runTime(at(3, 38, 10), now)).toBe("Tue 03:38");
    expect(runTime(at(3, 38, 9), now)).toBe("9 Nov 03:38");
  });

  test("a run's outcome, from its status and how its turn ended", () => {
    expect(runOutcome(r("a", { status: "running" }))).toBe("working");
    expect(runOutcome(r("a", { status: "needs_approval" }))).toBe("approval");
    expect(runOutcome(r("a", { status: "queued" }))).toBe("queued");
    expect(runOutcome(r("a", { status: "error" }))).toBe("failed");
    expect(runOutcome(r("a", { lastTurn: "failed" }))).toBe("failed");
    expect(runOutcome(r("a", { lastTurn: "interrupted" }))).toBe("stopped");
    expect(runOutcome(r("a", { lastTurn: "completed" }))).toBe("done");
    expect(runOutcome(r("a"))).toBe("done");
    expect(RUN_LABEL.done).toBe("Done");
  });

  test("a run's tooltip says when, how long and how it ended; a live one how long so far", () => {
    const done = r("a", { createdAt: at(4, 38), finishedAt: at(4, 38) + 22 * MIN + 33_000, updatedAt: at(9, 0) });
    expect(runSummary(done, now)).toBe("04:38 · 22m 33s · Done");
    const live = r("b", { status: "running", createdAt: now - 3 * MIN - 12_000, updatedAt: now - 3 * MIN });
    expect(runSummary(live, now)).toBe("09:56 · 3m 12s · Running");
    // A run saved before yardd kept finishedAt falls back to its last change.
    expect(runSummary(r("c", { createdAt: at(8, 0), updatedAt: at(8, 1), lastTurn: "failed" }), now)).toBe("08:00 · 1m 0s · Failed");
  });

  test("the strip holds the newest runs, oldest first so the newest is on the right", () => {
    const runs = [r("c", { createdAt: 3 }), r("b", { createdAt: 2 }), r("a", { createdAt: 1 })];
    expect(stripRuns(runs, 2).map((s) => s.id)).toEqual(["b", "c"]);
    expect(stripRuns(runs, 10).map((s) => s.id)).toEqual(["a", "b", "c"]);
  });

  test("the schedule line: when it runs and when next, or off", () => {
    expect(scheduleLine(a({ schedule: { kind: "interval", every: 1, unit: "hours" }, nextRun: now + 30 * MIN }), now)).toBe("Every hour · next in 30 min");
    expect(scheduleLine(a({ schedule: { kind: "interval", every: 1, unit: "hours" }, window: { start: "21:00", end: "09:00" }, nextRun: at(21) }), now)).toBe("Every hour · 21:00–09:00 · next today 21:00");
    expect(scheduleLine(a({ schedule: { kind: "daily", time: "03:30" }, nextRun: at(3, 30, 17) }), now)).toBe("Daily at 03:30 · next tomorrow 03:30");
    expect(scheduleLine(a({ enabled: false, nextRun: null }), now)).toBe("Daily at 09:00 · off");
  });

  test("a manual automation has no next run: it runs when started", () => {
    expect(scheduleLine(a({ schedule: { kind: "manual" }, nextRun: null }), now)).toBe("Manual · runs when started");
    expect(scheduleLine(a({ schedule: { kind: "manual" }, enabled: false, nextRun: null }), now)).toBe("Manual · off");
    // The card cuts the first part when the line is too long, so the next run is always the second.
    expect(scheduleParts(a({ schedule: { kind: "interval", every: 5, unit: "minutes" }, window: { start: "21:00", end: "09:00", days: [1, 3, 5] }, nextRun: at(21) }), now)).toEqual(["Every 5 minutes · 21:00–09:00, Mon, Wed, Fri", "next today 21:00"]);
    expect(scheduleParts(a({ schedule: { kind: "manual" }, nextRun: null }), now)).toEqual(["Manual", "runs when started"]);
  });

  test("a run started for a pull request is labelled with it, a plain run by its time alone", () => {
    // Built, not written out: the design check reads "#" and digits as a colour.
    const tag = `#${161}`;
    const pr = r("p", { title: `Loop verify ${tag} · 2026-11-16 04:38`, createdAt: at(4, 38), updatedAt: at(5, 0), finishedAt: at(5, 0) });
    expect(runTag(pr, "Loop verify")).toBe(tag);
    expect(runLabel(pr, "Loop verify", now)).toBe(`04:38 ${tag}`);
    expect(runSummary(pr, now, "Loop verify")).toBe(`04:38 ${tag} · 22m 0s · Done`);
    const plain = r("q", { title: "Loop patrol · 2026-11-16 04:38", createdAt: at(4, 38) });
    expect(runTag(plain, "Loop patrol")).toBe("");
    expect(runLabel(plain, "Loop patrol", now)).toBe("04:38");
    // A run renamed by hand keeps no tag.
    expect(runTag(r("x", { title: "my notes" }), "Loop verify")).toBe("");
  });

  test("a working run is what the card says first, with how long it has run, and it opens that run", () => {
    const running = r("b", { status: "running", createdAt: now - 3 * MIN - 12_000 });
    const s = cardStatus(a({ lastRun: { at: now - 3 * MIN, outcome: "skipped", reason: "the previous run is still going: Loop patrol · b" } }), [running, r("a", { createdAt: 1 })], now);
    expect(s).toEqual({ tone: "live", text: "Running · 3m 12s", title: "Running since 09:56", runId: "b" });
    expect(cardStatus(a(), [r("b", { status: "needs_approval", createdAt: now - 60_000 })], now)).toMatchObject({ tone: "approval", text: "Needs approval · 1m 0s", runId: "b" });
    expect(cardStatus(a(), [r("b", { status: "queued", createdAt: now - 60_000 })], now)).toMatchObject({ tone: "live", text: "Queued", runId: "b" });
  });

  test("the last run: how long it took and when, without the automation's name", () => {
    const last = r("a", { createdAt: now - 60 * MIN, finishedAt: now - 60 * MIN + 22 * MIN + 33_000, lastTurn: "completed" });
    expect(cardStatus(a({ lastRun: { at: now - 60 * MIN, outcome: "started", sessionId: "a" } }), [last], now)).toEqual({
      tone: "ok",
      text: "Last: 22m 33s · 1 h ago",
      title: "Last run 09:00 · 22m 33s · Done",
      runId: "a",
    });
    const failed = { ...last, lastTurn: "failed" as const };
    expect(cardStatus(a({ lastRun: { at: now - 60 * MIN, outcome: "started", sessionId: "a" } }), [failed], now)).toMatchObject({ tone: "bad", text: "Failed · 22m 33s · 1 h ago", runId: "a" });
    const stopped = { ...last, lastTurn: "interrupted" as const };
    expect(cardStatus(a(), [stopped], now)).toMatchObject({ tone: "quiet", text: "Stopped · 22m 33s · 1 h ago", runId: "a" });
    // Its session was deleted: only when is known.
    expect(cardStatus(a({ lastRun: { at: now - 5 * MIN, outcome: "started", sessionId: "gone" } }), [], now)).toEqual({ tone: "ok", text: "Last: 5 min ago", title: "Last run 09:55", runId: null });
  });

  test("a skip or a failure to start says why on one line, the whole reason in the tooltip", () => {
    const skipped = cardStatus(a({ lastRun: { at: now - 29 * MIN, outcome: "skipped", reason: "the previous run is still going: Loop patrol · 2026-11-17 04:38" } }), [], now);
    expect(skipped).toEqual({
      tone: "warn",
      text: "Skipped 29 min ago · previous run busy",
      title: "Skipped 29 min ago: the previous run is still going: Loop patrol · 2026-11-17 04:38",
      runId: null,
    });
    expect(cardStatus(a({ lastRun: { at: now - 3 * 3600_000, outcome: "skipped", reason: "Claude 5-hour usage 47% is above 10%" } }), [], now).text).toBe("Skipped 3 h ago · 5-hour usage 47%");
    // Grok's allowance is weekly, monthly or for a billing period.
    expect(cardStatus(a({ lastRun: { at: now - 3 * 3600_000, outcome: "skipped", reason: "Grok monthly usage 60% is above 50%" } }), [], now).text).toBe("Skipped 3 h ago · monthly usage 60%");
    expect(cardStatus(a({ lastRun: { at: now - 3 * 3600_000, outcome: "skipped", reason: "Grok billing-period usage 60% is above 50%" } }), [], now).text).toBe("Skipped 3 h ago · billing-period usage 60%");
    const failed = cardStatus(a({ lastRun: { at: now - 20_000, outcome: "failed", reason: "/w/moved is not a folder" } }), [], now);
    expect(failed).toEqual({ tone: "bad", text: "Failed · /w/moved is not a folder", title: "Failed just now: /w/moved is not a folder", runId: null });
  });

  test("never run", () => {
    expect(cardStatus(a(), [], now)).toEqual({ tone: "quiet", text: "No runs yet", title: "No runs yet", runId: null });
  });
});

// Read through the module so each claim fails on its own before the change.
const usageCaps = (...args: unknown[]) => (A as any).usageCaps(...args);

describe("usage limits name the engine's figures, or say they cannot apply", () => {
  const SKIP = "Applies to scheduled runs; Run now always starts one.";
  test("Claude and Codex: both limits apply to their plan usage", () => {
    expect(usageCaps("claude", true)).toEqual({ fiveHour: true, week: true, weekLabel: "When weekly usage is above", note: `${SKIP} The limits read the Claude plan usage.` });
    expect(usageCaps("codex", true).note).toBe(`${SKIP} The limits read the Codex plan usage.`);
  });

  test("not reported yet: they skip nothing until it is", () => {
    expect(usageCaps("codex", false).note).toBe(`${SKIP} The limits read the Codex plan usage, which it has not reported yet, so until it does they skip nothing.`);
  });

  test("Grok: only its credit allowance, no 5-hour window", () => {
    const g = usageCaps("grok", true);
    expect(g).toMatchObject({ fiveHour: false, week: true, weekLabel: "When credit usage is above" });
    expect(g.note).toBe(`${SKIP} Grok reports no 5-hour window, only its credit allowance for the billing period, which the credit limit reads.`);
    expect(usageCaps("grok", false).note).toContain("It has not reported it yet, so until it does the limit skips nothing.");
  });

  test("OpenCode and Gemini report no limits: neither applies, and the form says so", () => {
    for (const [engine, name] of [["opencode", "OpenCode"], ["gemini", "Gemini"]]) {
      const c = usageCaps(engine, false);
      expect(c).toMatchObject({ fiveHour: false, week: false });
      expect(c.note).toBe(`${name} doesn't report usage limits, so these can't skip its runs.`);
    }
  });
});
