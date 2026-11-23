// How a thread's items are laid out, after T3 Code's work log: tool calls and
// thinking between messages fold into one-line groups, and a finished turn
// folds all of its work into "Worked for 1m 12s" above its final answer.
import type { Item, SessionMeta, ToolItem } from "./protocol";

export type { ToolItem };
export type WorkItem = Extract<Item, { kind: "tool" | "reasoning" }>;

export type Row =
  | { type: "item"; key: string; item: Item }
  /** Consecutive tool calls and thinking. `live` while the turn is still adding to it. */
  | { type: "work"; key: string; entries: WorkItem[]; live: boolean }
  /** Everything a finished turn did before its answer. */
  | { type: "fold"; key: string; rows: Row[]; ms: number | null; status: string; summary: string }
  /** A tool call's widget (MCP Apps: a chart, a table). It stays in view
   *  when its turn folds, between the fold and the answer. */
  | { type: "widget"; key: string; tool: ToolItem }
  /** A stopped turn that did no work has no fold to say so: this marks its
   *  cut-off text, so it does not read as a full answer. */
  | { type: "stopped"; key: string; ms: number | null };

const isWork = (it: Item): it is WorkItem => it.kind === "tool" || it.kind === "reasoning";

/** An Agent call that is still going: it stays in view after its turn ends. */
const goingOn = (it: Item) => it.kind === "tool" && !!it.agent && it.state === "running";

/** Move each step that ran under an Agent call into that call's `steps`. A
 *  step whose call is not in the turn stays where it is. */
function nest(items: Item[]): Item[] {
  if (!items.some((it) => it.kind === "tool" && it.parent)) return items;
  const copies = new Map<string, ToolItem>();
  for (const it of items) if (it.kind === "tool") copies.set(it.itemId, { ...it, steps: undefined });
  const top: Item[] = [];
  for (const it of items) {
    if (it.kind !== "tool") {
      top.push(it);
      continue;
    }
    const copy = copies.get(it.itemId)!;
    const parent = it.parent && it.parent !== it.itemId ? copies.get(it.parent) : undefined;
    if (parent) (parent.steps ??= []).push(copy);
    else top.push(copy);
  }
  return top;
}

export function sessionSubagents(items: Item[]): ToolItem[] {
  return nest(items).filter((it): it is ToolItem => it.kind === "tool" && !!it.agent);
}

const widgetRow = (t: ToolItem): Row => ({ type: "widget", key: `widget-${t.key}`, tool: t });

/** Group runs of work items; `liveTail` marks the last run live. A widget
 *  shows right after its call and starts a new run. */
function group(items: Item[], liveTail: boolean): Row[] {
  const rows: Row[] = [];
  for (const it of items) {
    if (it.kind === "turn") continue;
    const last = rows[rows.length - 1];
    if (isWork(it)) {
      if (last?.type === "work") last.entries.push(it);
      else rows.push({ type: "work", key: it.key, entries: [it], live: false });
      if (it.kind === "tool" && it.app) rows.push(widgetRow(it));
    } else {
      rows.push({ type: "item", key: it.key, item: it });
    }
  }
  const last = rows[rows.length - 1];
  if (liveTail && last?.type === "work") last.live = true;
  return rows;
}

export function layout(items: Item[], running: boolean): Row[] {
  const rows: Row[] = [];
  // Split at user messages; anything before the first is a prelude.
  const turns: Item[][] = [[]];
  for (const it of items) {
    if (it.kind === "user") turns.push([it]);
    else turns[turns.length - 1].push(it);
  }
  // A message given to the agent mid-turn (steered) splits its turn in two
  // here; each part is timed from its start to the next part's.
  let lastMarker: Extract<Item, { kind: "turn" }> | undefined;
  turns.forEach((turn, n) => {
    const head = turn[0]?.kind === "user" ? turn[0] : null;
    if (head) rows.push({ type: "item", key: head.key, item: head });
    const body = nest(head ? turn.slice(1) : turn);
    const live = running && n === turns.length - 1;
    const own = body.find((it): it is Extract<Item, { kind: "turn" }> => it.kind === "turn");
    if (own) lastMarker = own;
    const steered = head?.kind === "user" && head.steered;
    const marker = own ?? (steered ? lastMarker : undefined);
    const nextHead = turns[n + 1]?.[0];
    const cut = nextHead?.kind === "user" && nextHead.steered ? nextHead.at : undefined;
    const from = own?.start ?? (steered && head?.kind === "user" ? head.at : undefined);
    const to = cut ?? marker?.end;
    // "Worked for" is the agent's time: an approval's wait for the user is
    // left out (one never answered waited until the turn ended).
    const ms = from != null && to != null ? to - from - waited(turn, from, to) : null;
    // Only the part that runs to the stop is marked, not each steered part.
    const stoppedMark = (): Row[] => (!live && marker && cut === undefined && marker.status === "interrupted" ? [{ type: "stopped", key: `stopped-${marker.key}`, ms }] : []);
    if (live || !body.some(isWork)) {
      rows.push(...group(body, live), ...stoppedMark());
      return;
    }
    // The answer is the last message; errors after it stay in view too.
    let answer = -1;
    for (let i = body.length - 1; i >= 0; i--) {
      if (body[i].kind === "assistant") {
        answer = i;
        break;
      }
    }
    const folded = (answer < 0 ? body : body.slice(0, answer)).filter((it) => it.kind !== "error" && !goingOn(it));
    const shown = body.filter((it, i) => (answer >= 0 && i >= answer) || (it.kind === "error" && !folded.includes(it)) || goingOn(it));
    const work = folded.filter(isWork);
    if (!work.length) {
      rows.push(...group(body, false), ...stoppedMark());
      return;
    }
    rows.push({
      type: "fold",
      key: `fold-${folded[0]?.key ?? body[0].key}`,
      // Widgets are shown outside the fold, so not twice inside it.
      rows: group(folded, false).filter((r) => r.type !== "widget"),
      ms,
      status: marker?.status ?? "completed",
      summary: summarize(work),
    });
    for (const t of work) if (t.kind === "tool" && t.app) rows.push(widgetRow(t));
    rows.push(...group(shown, false));
  });
  return rows;
}

/** How long this part's approvals waited for the user, within `from`..`to`.
 *  Several can wait at once (Claude asks in parallel), so the waits are
 *  merged: time is left out once, however many approvals shared it. */
function waited(items: Item[], from: number, to: number): number {
  const spans: [number, number][] = [];
  for (const it of items) {
    if (it.kind !== "approval" || it.askedAt == null) continue;
    const start = Math.max(it.askedAt, from);
    const end = Math.min(it.answeredAt ?? to, to);
    if (end > start) spans.push([start, end]);
  }
  spans.sort((a, b) => a[0] - b[0]);
  let ms = 0;
  let reached = -Infinity;
  for (const [start, end] of spans) {
    if (end <= reached) continue;
    ms += end - Math.max(start, reached);
    reached = end;
  }
  return ms;
}

const PLURAL: Record<string, [string, string, string]> = {
  read: ["Read", "file", "files"],
  edit: ["Edited", "file", "files"],
  command: ["Ran", "command", "commands"],
  search: ["Searched", "time", "times"],
  // The host agent reading its evidence (yardd agents.rs).
  context: ["Read", "host snapshot", "host snapshots"],
};

/** A call whose approval was denied or never answered: it did not run. */
export const didNotRun = (e: WorkItem) => e.kind === "tool" && e.state !== "ok" && (e.asked === "denied" || e.asked === "unanswered");

/** "Read 3 files, ran 2 commands", from the two biggest kinds of work. Calls
 *  that were not allowed to run are counted apart: "1 command not run". */
export function summarize(entries: WorkItem[]): string {
  const ran = entries.filter((e) => !didNotRun(e));
  const counts = new Map<string, number>();
  for (const e of ran) {
    const kind = e.kind === "reasoning" ? "think" : PLURAL[e.toolKind] ? e.toolKind : "other";
    counts.set(kind, (counts.get(kind) ?? 0) + 1);
  }
  const parts = [...counts.entries()]
    .filter(([kind]) => kind !== "think")
    .sort((a, b) => b[1] - a[1])
    .slice(0, 2)
    .map(([kind, n]) => {
      if (kind === "other") return `${n} other ${n === 1 ? "step" : "steps"}`;
      const [verb, one, many] = PLURAL[kind];
      return `${verb.toLowerCase()} ${n} ${n === 1 ? one : many}`;
    });
  const failed = ran.filter((e) => e.kind === "tool" && e.state === "failed").length;
  const skipped = entries.filter(didNotRun) as ToolItem[];
  const kinds = new Set(skipped.map((e) => e.toolKind));
  const noun = kinds.size === 1 && PLURAL[skipped[0]?.toolKind] ? PLURAL[skipped[0].toolKind] : ["", "step", "steps"];
  const notRun = skipped.length ? `${skipped.length} ${skipped.length === 1 ? noun[1] : noun[2]} not run` : "";
  if (!parts.length && !notRun) return counts.has("think") ? "Thought" : "";
  const text = parts.join(", ");
  const head = text ? text[0].toUpperCase() + text.slice(1) + (failed ? ` · ${failed} failed` : "") : "";
  return [head, notRun].filter(Boolean).join(" · ");
}

/** One line for a thought: its leading **heading**, else its first sentence. */
export function thoughtLine(text: string): string {
  const t = text.trim();
  const heading = /^\*\*(.+?)\*\*/.exec(t);
  if (heading) return heading[1].trim();
  const plain = t.replace(/[*_`#>]/g, "").replace(/\s+/g, " ");
  const sentence = /^(.{1,140}?[.!?])(\s|$)/.exec(plain);
  const line = sentence ? sentence[1] : plain;
  return line.length > 140 ? `${line.slice(0, 137)}…` : line;
}

/** Whether the thread stays pinned to its bottom after a scroll event, given
 *  the scroll position before and after it and the distance left to the
 *  bottom. Only a move up unpins. A diagram or thumbnail is inserted before it
 *  has a height and grows a moment later; the browser then moves `scrollTop`
 *  down to keep the view steady and fires a scroll event far from the bottom.
 *  That is not the user leaving, and the resize that follows must still
 *  bring the bottom back. */
export function stillPinned(pinned: boolean, before: number, after: number, gap: number): boolean {
  if (gap < 80) return true;
  return after < before ? false : pinned;
}

/** "45s", "1m 12s", "2h 3m". */
export function duration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}

/** The session each hand-off note in `meta`'s thread points at, by note key.
 *  Both sides keep one title, so the notes are the way between them. yardd
 *  writes "Handed off to X." in the source, once per hand-off, and "Handed
 *  off from X." in the new session, whose `parent` is the source; each note
 *  names the other session. Notes written before they did fall back to order:
 *  a source's n-th note is its n-th child by creation. A session that is gone
 *  gets no link. */
export function handoffLinks(items: Item[], meta: SessionMeta, sessions: Record<string, SessionMeta>): Record<string, string> {
  const children = Object.values(sessions)
    .filter((s) => s.parent === meta.id)
    .sort((a, b) => a.createdAt - b.createdAt);
  const parent = meta.parent && sessions[meta.parent] ? meta.parent : null;
  const links: Record<string, string> = {};
  let next = 0;
  for (const it of items) {
    if (it.kind !== "note") continue;
    if (it.text.startsWith("Handed off to ")) {
      const child = it.session ? sessions[it.session] : children[next];
      next++;
      if (child) links[it.key] = child.id;
    } else if (it.text.startsWith("Handed off from ")) {
      const source = it.session ? (sessions[it.session] ? it.session : null) : parent;
      if (source) links[it.key] = source;
    }
  }
  return links;
}
