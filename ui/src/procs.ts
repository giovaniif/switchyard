// What the Processes tab shows: the host sampler's list, as a tree.
import type { ProcRow } from "./protocol";

/** Parents before their children, children by pid. A process whose parent is
 *  not in the list (a session's sidecar, a shell) is a root. */
export function procTree(rows: ProcRow[]): { row: ProcRow; depth: number }[] {
  const byPid = new Map(rows.map((r) => [r.pid, r]));
  const kids = new Map<number, ProcRow[]>();
  const roots: ProcRow[] = [];
  for (const r of rows) {
    if (r.ppid !== r.pid && byPid.has(r.ppid)) kids.set(r.ppid, [...(kids.get(r.ppid) ?? []), r]);
    else roots.push(r);
  }
  const out: { row: ProcRow; depth: number }[] = [];
  const seen = new Set<number>();
  const walk = (r: ProcRow, depth: number) => {
    if (seen.has(r.pid)) return;
    seen.add(r.pid);
    out.push({ row: r, depth });
    for (const k of (kids.get(r.pid) ?? []).sort((a, b) => a.pid - b.pid)) walk(k, depth + 1);
  };
  for (const r of roots.sort((a, b) => a.pid - b.pid)) walk(r, 0);
  // A parent loop leaves its members unreached: list them rather than hide them.
  for (const r of rows) walk(r, 0);
  return out;
}

/** A command line on one line: long absolute paths cut to their last segment, then the whole cut to `max`. */
export function shortCommand(cmd: string, max = 80): string {
  const text = cmd
    .trim()
    .split(/\s+/)
    .map((word) => {
      const segments = word.split("/");
      return word.startsWith("/") && segments.length > 3 && word.length > 24 ? segments[segments.length - 1] || word : word;
    })
    .join(" ");
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

const MB = 1024 ** 2;

/** "340 MB", "1.6 GB". */
export function memText(bytes: number): string {
  if (bytes >= 1024 * MB) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  return `${Math.round(bytes / MB)} MB`;
}

/** "3 processes · 12% CPU · 340 MB". */
export function procSummary(rows: ProcRow[]): string {
  if (!rows.length) return "No processes";
  const cpu = rows.reduce((sum, r) => sum + r.cpu, 0);
  const mem = rows.reduce((sum, r) => sum + r.memBytes, 0);
  return `${rows.length} ${rows.length === 1 ? "process" : "processes"} · ${Math.round(cpu)}% CPU · ${memText(mem)}`;
}

export type KillSignal = "term" | "kill";

/** Where a row's Stop stands, kept by the tab per pid. `start` is the row's
 *  start time when it was asked, so a reused pid does not inherit it. */
export type KillState = { signal: KillSignal; tree: boolean; start?: number } & (
  | { phase: "sending" }
  | { phase: "done"; result: "killed" | "running" | "refused" | "failed"; reason?: string }
);

/** The stop controls a row offers. */
export interface RowActions {
  /** The × that asks to stop it with SIGTERM. */
  stop: boolean;
  /** Also "Stop with children": it has children in the list. */
  withChildren: boolean;
  /** SIGTERM did not end it: "Force kill" (SIGKILL). */
  force: boolean;
  /** Force kill the children too, as the Stop before it did. */
  forceTree: boolean;
  /** A line under the row: under way, still running, or why not. */
  note?: { text: string; bad?: boolean };
}

/** The engine's own process is not offered a Stop: yardd refuses it, and the
 *  session's Stop and Delete are the ways to end an agent. */
export const ENGINE_HINT = "The session's agent. Use Stop or Delete to end it.";

/** What a row of the Processes tab offers, given the tab's last kill of it. */
export function rowActions(row: ProcRow, rows: ProcRow[], kill?: KillState): RowActions {
  const none: RowActions = { stop: false, withChildren: false, force: false, forceTree: false };
  if (row.engine) return none;
  const k = kill && (kill.start === undefined || row.start === undefined || kill.start === row.start) ? kill : undefined;
  const children = rows.some((r) => r.ppid === row.pid && r.pid !== row.pid);
  const idle: RowActions = { ...none, stop: true, withChildren: children };
  if (!k) return idle;
  if (k.phase === "sending") return { ...none, note: { text: k.signal === "kill" ? "Force killing…" : "Stopping…" } };
  switch (k.result) {
    case "killed":
      return { ...none, note: { text: k.tree ? "Stopped with its children." : "Stopped." } };
    case "running":
      return { ...none, force: true, forceTree: k.tree, note: { text: `Still running after ${k.signal === "kill" ? "SIGKILL" : "SIGTERM"}.` } };
    case "refused":
      return { ...idle, note: { text: `Not stopped: ${k.reason ?? "refused"}.`, bad: true } };
    case "failed":
      return { ...idle, note: { text: k.reason ?? "Not stopped.", bad: true } };
  }
}

/** Forget kills of processes that left the list, except one under way. */
export function pruneKills(kills: Record<number, KillState>, rows: ProcRow[]): Record<number, KillState> {
  const listed = new Set(rows.map((r) => r.pid));
  const kept = Object.entries(kills).filter(([pid, k]) => listed.has(Number(pid)) || k.phase === "sending");
  return kept.length === Object.keys(kills).length ? kills : Object.fromEntries(kept);
}

/** What the Processes tab says instead of a list, or null to show the list. */
export function procsBlank(rows: ProcRow[] | undefined, online: boolean): string | null {
  // Offline, the last list is stale and nothing new can come: no rows to stop,
  // and no "Loading…" that never ends.
  if (!online) return "Processes are available when this host reconnects.";
  if (!rows) return "Loading…";
  if (procTree(rows).length === 0) return "Nothing is running for this session.";
  return null;
}
