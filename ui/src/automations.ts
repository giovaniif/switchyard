// Words for automations: schedules, next and last run, and which sessions are runs.
import type { Automation, AutomationWindow, LastRun, Schedule, SessionMeta } from "./protocol";
import { duration } from "./thread";

const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const MIN = 60_000;
const HOUR = 60 * MIN;

export function describeSchedule(s: Schedule, window?: AutomationWindow | null): string {
  const schedule = describeBaseSchedule(s);
  if (!window || s.kind !== "interval") return schedule;
  const days = window.days && window.days.length < 7 ? describeWindowDays(window.days) : "";
  return `${schedule} · ${window.start}–${window.end}${days ? `, ${days}` : ""}`;
}

/** Consecutive start days read as a range; Sunday comes last in the week. */
function describeWindowDays(days: number[]): string {
  const sorted = [1, 2, 3, 4, 5, 6, 0].filter((d) => days.includes(d));
  const groups: string[] = [];
  for (let i = 0; i < sorted.length; i++) {
    const first = i;
    while (i + 1 < sorted.length && (sorted[i] + 1) % 7 === sorted[i + 1]) i++;
    const name = (d: number) => DAYS[d].slice(0, 3);
    groups.push(i === first ? name(sorted[i]) : `${name(sorted[first])}–${name(sorted[i])}`);
  }
  return groups.join(", ");
}

function describeBaseSchedule(s: Schedule): string {
  switch (s.kind) {
    case "interval": {
      const unit = s.unit === "hours" ? "hour" : "minute";
      return s.every === 1 ? `Every ${unit}` : `Every ${s.every} ${unit}s`;
    }
    case "daily":
      return `Daily at ${s.time}`;
    case "weekly":
      return `${DAYS[s.day] ?? "?"}s at ${s.time}`;
    case "manual":
      return "Manual";
  }
}

/** Why a schedule cannot be saved, or null. */
export function scheduleProblem(s: Schedule, window?: AutomationWindow | null): string | null {
  const problem = baseScheduleProblem(s);
  if (problem || !window) return problem;
  if (s.kind !== "interval") return "An active window requires an interval schedule.";
  if (badTime(window.start) || badTime(window.end)) return "Enter window times as HH:MM.";
  if (window.start === window.end) return "The window start and end must differ.";
  if (window.days && (!window.days.length || new Set(window.days).size !== window.days.length || window.days.some((d) => !Number.isInteger(d) || d < 0 || d > 6))) return "Pick unique window days from Sunday to Saturday.";
  return null;
}

const badTime = (t: string) => !/^([01]\d|2[0-3]):[0-5]\d$/.test(t);

function baseScheduleProblem(s: Schedule): string | null {
  switch (s.kind) {
    case "interval":
      return Number.isInteger(s.every) && s.every >= 1 ? null : "The interval must be at least 1.";
    case "daily":
      return badTime(s.time) ? "Enter a time as HH:MM." : null;
    case "weekly":
      if (!Number.isInteger(s.day) || s.day < 0 || s.day > 6) return "Pick a day of the week.";
      return badTime(s.time) ? "Enter a time as HH:MM." : null;
    case "manual":
      return null;
  }
}

const pad = (n: number) => String(n).padStart(2, "0");
const clock = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;
const dayStart = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** When the next run is due, relative when close and by day and time when not. */
export function describeNext(next: number | null | undefined, enabled: boolean, now: number): string {
  if (!enabled || next == null) return "off";
  const diff = next - now;
  if (diff < 0) return "due now";
  if (diff < MIN) return "in under a minute";
  if (diff < 6 * HOUR) {
    const mins = Math.floor(diff / MIN);
    const h = Math.floor(mins / 60);
    const m = mins % 60;
    if (h === 0) return `in ${m} min`;
    return m === 0 ? `in ${h} h` : `in ${h} h ${m} min`;
  }
  const when = new Date(next);
  const days = Math.round((dayStart(when) - dayStart(new Date(now))) / (24 * HOUR));
  if (days === 0) return `today ${clock(when)}`;
  if (days === 1) return `tomorrow ${clock(when)}`;
  if (days < 7) return `${DAYS[when.getDay()].slice(0, 3)} ${clock(when)}`;
  return `${when.getDate()} ${MONTHS[when.getMonth()]} ${clock(when)}`;
}

function ago(at: number, now: number): string {
  const diff = Math.max(0, now - at);
  if (diff < MIN) return "just now";
  if (diff < HOUR) return `${Math.floor(diff / MIN)} min ago`;
  if (diff < 24 * HOUR) return `${Math.floor(diff / HOUR)} h ago`;
  return `${Math.floor(diff / (24 * HOUR))} d ago`;
}

export function describeLast(last: LastRun | null | undefined, now: number): string {
  if (!last) return "never";
  return last.outcome === "started" ? ago(last.at, now) : `${last.outcome} ${ago(last.at, now)}`;
}

/** A run by when it started: the time today, the day this week, the date before that.
 *  Its title would only repeat the automation's name. */
export function runTime(at: number, now: number): string {
  const when = new Date(at);
  const days = Math.round((dayStart(new Date(now)) - dayStart(when)) / (24 * HOUR));
  if (days === 0) return clock(when);
  if (days > 0 && days < 7) return `${DAYS[when.getDay()].slice(0, 3)} ${clock(when)}`;
  return `${when.getDate()} ${MONTHS[when.getMonth()]} ${clock(when)}`;
}

export type RunOutcome = "working" | "approval" | "queued" | "failed" | "stopped" | "done";

export const RUN_LABEL: Record<RunOutcome, string> = {
  working: "Running",
  approval: "Needs approval",
  queued: "Queued",
  failed: "Failed",
  stopped: "Stopped",
  done: "Done",
};

export function runOutcome(s: SessionMeta): RunOutcome {
  if (s.status === "running") return "working";
  if (s.status === "needs_approval") return "approval";
  if (s.status === "queued") return "queued";
  if (s.status === "error" || s.lastTurn === "failed") return "failed";
  if (s.lastTurn === "interrupted") return "stopped";
  return "done";
}

const isLive = (s: SessionMeta) => s.status === "running" || s.status === "needs_approval" || s.status === "queued";

/** How long a run ran, or has run so far. Runs saved before yardd kept
 *  `finishedAt` fall back to their last change. */
export function runTook(s: SessionMeta, now: number): number {
  return Math.max(0, (isLive(s) ? now : s.finishedAt ?? s.updatedAt) - s.createdAt);
}

/**
 * What a run's title adds after the automation's name, such as the pull
 * request a `Loop verify #161 · <date>` run was started for; "" for a plain
 * `<name> · <date>` run.
 */
export function runTag(s: SessionMeta, name: string): string {
  const head = s.title.split(" · ")[0];
  return head.startsWith(`${name} `) ? head.slice(name.length + 1).trim() : "";
}

/** A run by its start time, plus its tag when it has one: `04:38 #161`. */
export const runLabel = (s: SessionMeta, name: string, now: number): string => [runTime(s.createdAt, now), runTag(s, name)].filter(Boolean).join(" ");

export const runSummary = (s: SessionMeta, now: number, name = ""): string =>
  `${name ? runLabel(s, name, now) : runTime(s.createdAt, now)} · ${duration(runTook(s, now))} · ${RUN_LABEL[runOutcome(s)]}`;

/** The newest `max` runs, oldest first, so the newest sits at the right end of the strip. */
export const stripRuns = (runs: SessionMeta[], max: number): SessionMeta[] => runs.slice(0, max).reverse();

export function scheduleLine(a: Automation, now: number): string {
  // A manual automation has no next run: it runs when someone (or a script) starts it.
  if (a.schedule.kind === "manual") return `Manual · ${a.enabled ? "runs when started" : "off"}`;
  const next = a.enabled && a.nextRun != null ? `next ${describeNext(a.nextRun, true, now)}` : "off";
  return `${describeSchedule(a.schedule, a.window)} · ${next}`;
}

/** yardd's skip reasons, cut to what fits a card's line; the tooltip keeps the whole one. */
function shortReason(reason: string): string {
  if (reason.startsWith("the previous run is still going")) return "previous run busy";
  const usage = reason.match(/^\S+ (5-hour|weekly) usage (\d+%) is above/);
  return usage ? `${usage[1]} usage ${usage[2]}` : reason;
}

export interface CardStatus {
  /** live and approval: a run is going; ok, warn, bad and quiet: how the last one went. */
  tone: "live" | "approval" | "ok" | "warn" | "bad" | "quiet";
  text: string;
  /** The whole story, for the tooltip. */
  title: string;
  /** The run the line opens, if it has one. */
  runId: string | null;
}

/** The one line a card leads with: a run going now, else how the last one went. `runs` newest first. */
export function cardStatus(a: Automation, runs: SessionMeta[], now: number): CardStatus {
  const live = runs.find(isLive);
  if (live) {
    const since = runTime(live.createdAt, now);
    const took = duration(runTook(live, now));
    if (live.status === "needs_approval") return { tone: "approval", text: `Needs approval · ${took}`, title: `Waiting for approval, started ${since}`, runId: live.id };
    if (live.status === "queued") return { tone: "live", text: "Queued", title: `Queued since ${since}`, runId: live.id };
    return { tone: "live", text: `Running · ${took}`, title: `Running since ${since}`, runId: live.id };
  }
  const last = a.lastRun;
  if (last && last.outcome !== "started") {
    const when = ago(last.at, now);
    if (last.outcome === "skipped") {
      return { tone: "warn", text: last.reason ? `Skipped ${when} · ${shortReason(last.reason)}` : `Skipped ${when}`, title: last.reason ? `Skipped ${when}: ${last.reason}` : `Skipped ${when}`, runId: null };
    }
    return { tone: "bad", text: `Failed · ${last.reason ?? "could not start"}`, title: `Failed ${when}: ${last.reason ?? "could not start"}`, runId: null };
  }
  // A remote run's session id carries its host prefix; the saved one may not.
  const sid = last?.sessionId;
  const run = sid ? runs.find((s) => s.id === sid || s.id.endsWith(`~${sid}`)) : runs[0];
  if (!run) {
    if (!last) return { tone: "quiet", text: "No runs yet", title: "No runs yet", runId: null };
    return { tone: "ok", text: `Last: ${ago(last.at, now)}`, title: `Last run ${runTime(last.at, now)}`, runId: null };
  }
  const outcome = runOutcome(run);
  const facts = `${duration(runTook(run, now))} · ${ago(run.createdAt, now)}`;
  const title = `Last run ${runSummary(run, now)}`;
  if (outcome === "failed") return { tone: "bad", text: `Failed · ${facts}`, title, runId: run.id };
  if (outcome === "stopped") return { tone: "quiet", text: `Stopped · ${facts}`, title, runId: run.id };
  return { tone: "ok", text: `Last: ${facts}`, title, runId: run.id };
}

/** An automation's runs on its host, newest first. Session ids on a remote host carry its prefix, so match on `host` too. */
export function runsOf(sessions: Record<string, SessionMeta>, host: string, automation: string): SessionMeta[] {
  return Object.values(sessions)
    .filter((s) => s.automation === automation && (s.host || "local") === host)
    .sort((a, b) => b.createdAt - a.createdAt);
}

/** The sidebar Automations button's dot. Runs stay out of the sidebar, so a
 *  run waiting for approval must outrank one that is only working: this dot is
 *  the only sign there that it waits for the user. */
export function runsAttention(runs: SessionMeta[]): "approval" | "working" | null {
  if (runs.some((s) => s.status === "needs_approval")) return "approval";
  if (runs.some((s) => s.status === "running")) return "working";
  return null;
}

/** Runs live on the Automations page; pinning one brings it back to the list. */
export const hiddenFromSidebar = (s: SessionMeta): boolean => !!s.automation && !s.pinned;
