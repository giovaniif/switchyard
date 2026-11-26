// Plan usage, as meters. Each engine reports its limits in its own shape
// (see Usage in protocol.ts); this turns them into one list of meters so the
// picker and the sidebar strip draw them the same way.
import type { AccountEntry, CodexLimit, Engine, LimitWindow, Usage } from "./protocol";
import { ENGINES, ENGINE_NAME } from "./engines";

/** Claude and Codex report their plan's windows, Grok its credit allowance;
 *  OpenCode and Gemini report none. */
export const reportsLimits = (engine: Engine) => engine === "claude" || engine === "codex" || engine === "grok";

/** The Subscriptions page's note on the engines it cannot show, derived from
 *  the engine list so a newly added engine is not left out of it. */
export function noLimitsNote(engines: Engine[] = ENGINES): string {
  const names = engines.filter((e) => !reportsLimits(e)).map((e) => ENGINE_NAME[e]);
  if (!names.length) return "";
  const list = names.length === 1 ? names[0] : `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;
  return `${list} ${names.length === 1 ? "doesn't" : "don't"} report subscription limits.`;
}

export type Level = "ok" | "warn" | "high";

export interface Meter {
  key: string;
  /** "5-hour", "Week", "Week · Sonnet". */
  label: string;
  /** A longer name for the popover, when "Weekly · all models" would be wrong. */
  title?: string;
  /** 0 to 100. */
  percent: number;
  /** Epoch milliseconds, when the engine said. */
  resetsAt: number | null;
  level: Level;
}

/** Amber above 70%, red above 90%. */
export const levelOf = (percent: number): Level => (percent > 90 ? "high" : percent > 70 ? "warn" : "ok");

const clamp = (n: number) => Math.max(0, Math.min(100, n));
export const percentText = (percent: number) => `${Math.round(clamp(percent))}%`;

/** Engines send epoch seconds; a value too big to be seconds is already ms. */
const toMs = (at: number | null | undefined): number | null => {
  if (typeof at !== "number" || !Number.isFinite(at)) return null;
  return at > 1e12 ? at : at * 1000;
};

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

/** "resets in 2 h 10 min", or "" when the engine gave no reset time. */
export function formatReset(resetsAt: number | null, now: number): string {
  if (resetsAt == null) return "";
  const left = resetsAt - now;
  if (left <= 0) return "resets now";
  if (left < MIN) return "resets in under a minute";
  const days = Math.floor(left / DAY);
  const hours = Math.floor((left % DAY) / HOUR);
  const mins = Math.floor((left % HOUR) / MIN);
  if (days > 0) return `resets in ${days} d${hours ? ` ${hours} h` : ""}`;
  if (hours > 0) return `resets in ${hours} h${mins ? ` ${mins} min` : ""}`;
  return `resets in ${mins} min`;
}

const timeOfDay = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const dateAndTime = new Intl.DateTimeFormat(undefined, { weekday: "short", month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });

/** "Resets 2:50 PM · in 1 h 35 min" (a day name and date when not today). */
export function resetText(resetsAt: number | null, now: number): string {
  if (resetsAt === null) return "Reset time not reported";
  const when = (new Date(resetsAt).toDateString() === new Date(now).toDateString() ? timeOfDay : dateAndTime).format(resetsAt);
  return `Resets ${when} · ${formatReset(resetsAt, now).replace(/^resets /, "")}`;
}

/** The engines report usage only while they run, so a figure can outlive its
 *  window. Once the reset time has passed nothing has been used since (that
 *  would have come with a newer reset time), so the window is empty again. */
const meter = (key: string, label: string, percent: number, resetsAt: number | null, now?: number, title?: string): Meter => {
  const named = title ? { title } : {};
  if (resetsAt != null && now != null && resetsAt <= now) return { key, label, ...named, percent: 0, resetsAt: null, level: "ok" };
  const p = clamp(percent);
  return { key, label, ...named, percent: p, resetsAt, level: levelOf(p) };
};

function windowLabel(mins: number | null | undefined, fallback: string): string {
  if (!mins) return fallback;
  if (mins >= 7 * 24 * 60) return "Week";
  if (mins % 60 === 0) return `${mins / 60}-hour`;
  return `${mins} min`;
}

const capitalize = (s: string) => (s ? s[0].toUpperCase() + s.slice(1) : s);

/** The SDK's window names that are not a plan limit of their own. */
const NOT_A_LIMIT = new Set(["seven_day_oauth_apps", "seven_day_overage_included"]);

/** One Codex limit's windows; another limit than the main one is named. */
function codexMeters(c: CodexLimit, name: string | null, now?: number): Meter[] {
  const out: Meter[] = [];
  const prefix = name ? `${c.limitId ?? name}:` : "";
  const add = (key: "primary" | "secondary", fallback: string) => {
    const w = c[key];
    if (!w) return;
    const label = windowLabel(w.windowDurationMins, fallback);
    out.push(meter(`${prefix}${key}`, name ? `${label} · ${name}` : label, w.usedPercent, toMs(w.resetsAt), now));
  };
  add("primary", "5-hour");
  add("secondary", "Week");
  return out;
}

/** An engine's meters, the 5-hour window first, then the week, then per-model
 *  weeks, then any other window the engine named. With `now`, windows whose
 *  reset time has passed read as empty. */
export function metersFor(engine: Engine, usage: Usage, now?: number): Meter[] {
  if (engine === "codex") {
    const c = usage.codex;
    if (!c) return [];
    const out = codexMeters(c, null, now);
    // Every limit by id, the main one among them: only the others are new.
    for (const [id, other] of Object.entries(c.byLimitId ?? {}).sort(([a], [b]) => a.localeCompare(b))) {
      if (!other || id === (c.limitId ?? "codex")) continue;
      out.push(...codexMeters({ limitId: id, ...other }, other.limitName || other.normalModelSlug || id, now));
    }
    return out;
  }
  if (!reportsLimits(engine)) return [];
  const c = engine === "grok" ? usage.grok : usage.claude;
  if (!c) return [];
  const windows: Record<string, LimitWindow> = { ...(c.windows ?? {}) };
  // An older yardd sent only the two fractions.
  if (engine === "claude" && !c.windows) {
    const old = c as NonNullable<Usage["claude"]>;
    if (old.fiveHour != null) windows.five_hour = { utilization: old.fiveHour, resetsAt: null };
    if (old.sevenDay != null) windows.seven_day = { utilization: old.sevenDay, resetsAt: null };
  }
  const out: Meter[] = [];
  const add = (key: string, label: string, title?: string) => {
    const w = windows[key];
    if (w && w.utilization != null && !out.some((m) => m.key === key)) out.push(meter(key, label, w.utilization * 100, toMs(w.resetsAt), now, title));
  };
  if (engine === "grok") {
    // Grok's allowance is credits for a billing period, not a model window.
    for (const key of Object.keys(windows).sort()) {
      const label = windows[key].label ?? capitalize(key);
      add(key, label, `${label === "Week" ? "Weekly" : label === "Month" ? "Monthly" : label} credits`);
    }
    return out;
  }
  add("five_hour", "5-hour");
  add("seven_day", "Week");
  for (const key of Object.keys(windows).sort()) {
    const m = key.match(/^seven_day_(.+)$/);
    if (m && !NOT_A_LIMIT.has(key)) add(key, windows[key].label ?? `Week · ${capitalize(m[1])}`);
  }
  // Rows the usage report named that the UI has no name of its own for. An
  // unnamed one (a turn event's `overage`) is not a limit of its own.
  for (const key of Object.keys(windows).sort()) {
    const label = windows[key].label;
    if (label && !NOT_A_LIMIT.has(key)) add(key, label);
  }
  return out;
}

/** The free resets an engine grants that are still to use (Codex's "Full
 *  reset" credits): how many, and when the first of them expires. */
export function fullResets(engine: Engine, usage: Usage, now: number): { count: number; title: string; expiresAt: number | null } | null {
  if (engine !== "codex") return null;
  const live = (usage.codex?.resetCredits?.credits ?? []).filter((c) => {
    const expires = toMs(c?.expiresAt);
    return c && (c.status ?? "available") === "available" && (expires == null || expires > now);
  });
  if (!live.length) return null;
  const expiries = live.map((c) => toMs(c.expiresAt)).filter((x): x is number => x != null);
  return { count: live.length, title: live[0].title || "Full reset", expiresAt: expiries.length ? Math.min(...expiries) : null };
}

/** "3 full resets available". */
export function fullResetsText(r: { count: number; title: string }): string {
  const name = r.title.toLowerCase();
  return `${r.count} ${name}${r.count === 1 ? "" : "s"} available`;
}

/** One row group of the sidebar strip: an engine under one account, as one host reported it. */
export interface StripGroup {
  engine: Engine;
  /** The account's name; null for Default. */
  account: string | null;
  host: string;
  /** In the shape `metersFor(engine, usage)` reads. */
  usage: Usage;
  /** False for an added account no host has reported limits for yet: it is
   *  listed anyway, so every account is in the strip. */
  reported: boolean;
}

/** A host the sidebar shows, with what it knows about the plan limits. */
export interface UsageSource {
  host: string;
  usage: Usage;
  accounts: AccountEntry[];
}

/** How recent a report is: its windows' reset times, shortest window first,
 *  then how much is used. */
function freshness(engine: Engine, usage: Usage): { resets: (number | null)[]; used: number } {
  if (engine === "codex") {
    const c = usage.codex;
    return { resets: [toMs(c?.primary?.resetsAt), toMs(c?.secondary?.resetsAt)], used: (c?.primary?.usedPercent ?? 0) + (c?.secondary?.usedPercent ?? 0) };
  }
  const w = (engine === "grok" ? usage.grok?.windows : usage.claude?.windows) ?? {};
  if (engine === "grok") {
    const all = Object.values(w);
    return { resets: [toMs(all[0]?.resetsAt)], used: all.reduce((n, x) => n + (x?.utilization ?? 0), 0) };
  }
  return {
    resets: [toMs(w.five_hour?.resetsAt), toMs(w.seven_day?.resetsAt)],
    used: (w.five_hour?.utilization ?? 0) + (w.seven_day?.utilization ?? 0),
  };
}

/** Whether report `a` came after `b`, as far as the reports tell: the same
 *  subscription's next window resets later, and inside one window use only
 *  grows. With no reset times to compare it cannot tell, and says no. */
function newer(engine: Engine, a: Usage, b: Usage): boolean {
  const fa = freshness(engine, a);
  const fb = freshness(engine, b);
  let known = false;
  for (let i = 0; i < fa.resets.length; i++) {
    const x = fa.resets[i];
    const y = fb.resets[i];
    if (x == null || y == null) continue;
    known = true;
    if (x !== y) return x > y;
  }
  return known && fa.used > fb.used;
}

/** What the sidebar strip shows: each engine and account once (an account is
 *  matched across hosts by name, since each host gives it its own id), from
 *  the shown host that reported it last, else the first one listed. A login
 *  is the same subscription on every machine, so one host's figures stand for
 *  all. Engines in the order given, Default first, then accounts as they appear. */
export function stripGroups(sources: UsageSource[], engines: Engine[]): StripGroup[] {
  // OpenCode and Gemini have no plan-limit reports, even with added accounts.
  engines = engines.filter(reportsLimits);
  const found = new Map<string, StripGroup>();
  const offer = (key: string, group: StripGroup) => {
    if (metersFor(group.engine, group.usage).length === 0) return;
    group.reported = true;
    const had = found.get(key);
    if (!had || newer(group.engine, group.usage, had.usage)) found.set(key, group);
  };
  for (const { host, usage, accounts } of sources) {
    for (const engine of engines) {
      offer(`${engine}\0`, { engine, account: null, host, usage: { [engine]: (usage as Record<string, unknown>)[engine] } as Usage, reported: false });
      for (const a of accounts) {
        if (a.engine !== engine) continue;
        offer(`${engine}\0${a.name.toLowerCase()}`, { engine, account: a.name, host, usage: { [engine]: usage.accounts?.[a.id] } as Usage, reported: false });
      }
    }
  }
  // Every added account is listed, reported or not, and so is Default next
  // to them: with two accounts, the one missing must not look like the only one.
  for (const { host, accounts } of sources) {
    for (const a of accounts) {
      if (!engines.includes(a.engine)) continue;
      const key = `${a.engine}\0${a.name.toLowerCase()}`;
      if (!found.has(key)) found.set(key, { engine: a.engine, account: a.name, host, usage: {} as Usage, reported: false });
      if (!found.has(`${a.engine}\0`)) found.set(`${a.engine}\0`, { engine: a.engine, account: null, host, usage: {} as Usage, reported: false });
    }
  }
  const all = [...found.values()];
  return engines.flatMap((engine) => {
    const mine = all.filter((g) => g.engine === engine);
    return [...mine.filter((g) => g.account === null), ...mine.filter((g) => g.account !== null)];
  });
}
