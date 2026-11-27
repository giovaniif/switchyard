// What a session row's mark says: working, waiting on the user, an answer
// not opened yet, failed, or quiet. "Answered" is per browser: the turn end
// the user last had in front of them is kept in localStorage.

import { useEffect, useRef, useState } from "react";
import type { SessionMeta } from "./protocol";

export type RowState = "working" | "approval" | "connector" | "unread" | "answered" | "failed" | "queued" | "idle" | "offline";

export const ROW_LABEL: Record<RowState, string> = {
  working: "Running",
  approval: "Needs approval",
  connector: "Needs connector",
  answered: "Answered",
  unread: "Unread",
  failed: "Failed",
  queued: "Queued",
  idle: "Idle",
  offline: "Offline",
};

/** `seen`: the session's `turnEndedAt` when the user last had it open. */
export function rowState(s: SessionMeta, seen: number, online: boolean, unread = false): RowState {
  if (!online) return "offline";
  if (s.status === "running") return "working";
  if (s.status === "needs_approval") return "approval";
  if (s.status === "queued") return "queued";
  // Its turn ended, but a subagent it started is still at work.
  if (s.background?.length) return "working";
  // The agent asked for a service to be connected and its turn ended on that.
  if (s.needsConnector) return "connector";
  if (unread) return "unread";
  if (s.status === "error") return "failed";
  if (s.turnEndedAt !== undefined && s.turnEndedAt !== null && s.turnEndedAt > seen) {
    if (s.lastTurn === "completed") return "answered";
    if (s.lastTurn === "failed") return "failed";
  }
  return "idle";
}

export type Seen = Record<string, number>;

/** The map after the user saw `s`; the same object when nothing changed, so
 *  React skips the update. Sessions that are gone are dropped on the way. */
export function markSeen(seen: Seen, s: SessionMeta, ids: Set<string>): Seen {
  const end = s.turnEndedAt ?? 0;
  if ((seen[s.id] ?? 0) >= end) return seen;
  const next: Seen = {};
  for (const [id, at] of Object.entries(seen)) if (ids.has(id)) next[id] = at;
  next[s.id] = end;
  return next;
}

const KEY = "yard.seen";
const SINCE = "yard.seen.since";

function load(): { seen: Seen; since: number } {
  // Turns that ended before this browser first kept track count as seen, so
  // the first load does not mark every old session as answered.
  let since = Number(localStorage.getItem(SINCE));
  if (!since) {
    since = Date.now();
    localStorage.setItem(SINCE, String(since));
  }
  try {
    const seen = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    return { seen: seen && typeof seen === "object" ? seen : {}, since };
  } catch {
    return { seen: {}, since };
  }
}

const looking = () => document.visibilityState === "visible" && document.hasFocus();

/** Which sessions' answers the user has seen. The open session counts as
 *  seen while the window is in front; other windows follow through `storage`. */
export function useSeen(sessions: Record<string, SessionMeta>, openId: string | null, restoring: string | null = null) {
  const [{ seen, since }, set] = useState(load);
  const open = openId ? sessions[openId] : undefined;
  const visiting = openId ?? restoring;
  const previous = useRef(visiting);
  const [front, setFront] = useState(looking);
  const save = (next: Seen) => {
    localStorage.setItem(KEY, JSON.stringify(next));
    set({ seen: next, since });
  };
  const read = (id: string) => {
    const session = sessions[id];
    if (session) save(markSeen(seen, session, new Set(Object.keys(sessions))));
  };
  const unread = (id: string) => {
    const next: Seen = {};
    for (const [key, at] of Object.entries(seen)) if (sessions[key]) next[key] = at;
    // -1 is an explicit unread mark, independent of the last turn's outcome
    // and of the first-visit cutoff. A reload of the open chat preserves it.
    next[id] = -1;
    save(next);
  };
  useEffect(() => {
    const update = () => setFront(looking());
    const synced = (e: StorageEvent) => (e.key === KEY || e.key === null) && set(load());
    window.addEventListener("focus", update);
    window.addEventListener("blur", update);
    document.addEventListener("visibilitychange", update);
    window.addEventListener("storage", synced);
    return () => {
      window.removeEventListener("focus", update);
      window.removeEventListener("blur", update);
      document.removeEventListener("visibilitychange", update);
      window.removeEventListener("storage", synced);
    };
  }, []);
  useEffect(() => {
    const changed = previous.current !== visiting;
    previous.current = visiting;
    if (!open || !front) return;
    if (seen[open.id] === -1 && !changed) return;
    const next = markSeen(seen, open, new Set(Object.keys(sessions)));
    if (next !== seen) save(next);
  }, [visiting, openId, open?.turnEndedAt, front]);
  return {
    seenAt: (id: string) => Math.max(seen[id] ?? 0, since),
    isUnread: (id: string) => seen[id] === -1,
    markUnread: unread,
    markRead: read,
  };
}
