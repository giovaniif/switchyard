// OS notifications: when a session needs the user, and how one is shown.
// The rule and the status watcher are pure so they can be tested; showing
// is the Web Notifications API in a browser and the Tauri notification
// plugin in the desktop app (WKWebView has no `Notification`).

import { useState } from "react";
import type { AgentEvent, SessionMeta, Status } from "./protocol";
import { duration } from "./thread";
import { inMobileApp } from "./mobile";

export type Notice = "approval" | "finished" | "failed";

/** A finished turn shorter than this is not worth a banner: the user was
 *  hardly away. Approvals and failures always are, since they wait on them. */
export const MIN_TURN_MS = 60_000;

/** What a session's change of status means for the user, if anything. A
 *  session seen for the first time (`prev` unset) is not news. */
export function statusNotice(prev: Status | undefined, next: Status): Notice | null {
  if (prev === undefined || prev === next) return null;
  if (next === "needs_approval") return "approval";
  if (next === "error") return "failed";
  if (next === "idle" && (prev === "running" || prev === "needs_approval")) return "finished";
  return null;
}

export interface Where {
  enabled: boolean;
  focused: boolean;
  /** The session shown in the window right now. */
  openSessionId: string | null;
}

/** Only when the user is not already looking at that session. A note that
 *  names no session is for the user wherever they are. */
export function shouldNotify(where: Where, sessionId: string | null): boolean {
  if (!where.enabled) return false;
  return !(sessionId !== null && where.focused && where.openSessionId === sessionId);
}

/** What the current turn did, from its events, so a banner can say it. */
export interface TurnInfo {
  /** Local time the turn started; unknown for one already running at load. */
  startedAt?: number;
  /** How long it took, set when it ends. */
  ms?: number;
  /** The turn's latest answer. */
  answer?: string;
  approval?: { title: string; detail: string; question?: boolean };
  error?: string;
}

/** Agent text as one plain line for a banner. Untrusted, but a notification
 *  shows plain text only. */
export function plainLine(text: string, max = 160): string {
  const line = text
    .replace(/```[^\n]*\n?/g, "")
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/^\s{0,3}(#+|>|[-*+]|\d+\.)\s+/gm, "")
    .replace(/[*_`~]/g, "")
    .replace(/\s+/g, " ")
    .trim();
  return line.length > max ? `${line.slice(0, max - 1).trimEnd()}…` : line;
}

export function noticeText(kind: Notice, title: string, info: TurnInfo = {}): { title: string; body: string } {
  const join = (head: string, rest?: string) => (rest ? `${head} · ${rest}` : head);
  let body: string;
  if (kind === "approval") {
    if (info.approval?.question) body = join("Has a question", plainLine(info.approval.title));
    else {
      const what = info.approval && plainLine([info.approval.title, info.approval.detail].filter(Boolean).join(": "));
      body = join("Needs your approval", what);
    }
  } else if (kind === "failed") {
    body = join(info.ms !== undefined ? `Failed after ${duration(info.ms)}` : "Failed", info.error && plainLine(info.error));
  } else {
    body = join(info.ms !== undefined ? `Done in ${duration(info.ms)}` : "Done", info.answer && plainLine(info.answer));
  }
  return { title: title.trim() || "Switchyard", body };
}

/** Last known status of every session, to tell a change from a snapshot. */
export class StatusWatcher {
  private seen = new Map<string, { status: Status; host: string }>();
  private ends = new Map<string, string>();
  private turns = new Map<string, TurnInfo>();

  /** `now` is the page's clock: yardd's `at` would mix in a remote host's. */
  constructor(private now: () => number = () => Date.now()) {}

  /** What a session's events say about its current turn. */
  event(id: string, ev: AgentEvent) {
    const turn = this.turns.get(id) ?? {};
    switch (ev.type) {
      case "turn_started":
        this.turns.set(id, { startedAt: this.now() });
        return;
      case "assistant_message":
        if (ev.text.trim()) turn.answer = ev.text;
        break;
      case "approval_request":
        turn.approval = { title: ev.title, detail: ev.detail, ...(ev.kind === "question" ? { question: true } : {}) };
        break;
      case "error":
        turn.error = ev.message;
        break;
      case "turn_completed":
        this.turnEnded(id, String(ev.status));
        if (turn.startedAt !== undefined) turn.ms = this.now() - turn.startedAt;
        break;
      default:
        return;
    }
    this.turns.set(id, turn);
  }

  info(id: string): TurnInfo {
    return this.turns.get(id) ?? {};
  }

  /** A snapshot of one host's sessions: the new baseline, nothing to report. */
  seed(sessions: SessionMeta[], host = "local") {
    for (const [id, s] of this.seen) if (s.host === host) this.seen.delete(id);
    for (const s of sessions) this.seen.set(s.id, { status: s.status, host: s.host || "local" });
  }

  /** How a turn ended (`turn_completed`'s status). yardd sends it just before
   *  the session goes idle, and a failed turn ends idle too: this tells them apart. */
  turnEnded(id: string, status: string) {
    this.ends.set(id, status);
  }

  change(session: SessionMeta): Notice | null {
    const prev = this.seen.get(session.id)?.status;
    this.seen.set(session.id, { status: session.status, host: session.host || "local" });
    const kind = statusNotice(prev, session.status);
    const end = this.ends.get(session.id);
    this.ends.delete(session.id);
    if (kind !== "finished") return kind;
    // A turn the user stopped needs no banner.
    if (end === "failed") return "failed";
    if (end === "interrupted") return null;
    // Only what is worth leaving another app for: an automation's run reports
    // on its own page, and a quick turn ends before the user is really away.
    if (session.automation) return null;
    const ms = this.turns.get(session.id)?.ms;
    return ms !== undefined && ms < MIN_TURN_MS ? null : kind;
  }

  forget(id: string) {
    this.seen.delete(id);
    this.ends.delete(id);
    this.turns.delete(id);
  }
}

export interface Note {
  title: string;
  body?: string;
  sessionId?: string;
  url?: string;
}

const KEY = "yard.notifications";
export const isEnabled = () => localStorage.getItem(KEY) === "on";

/** The desktop app's window exposes Tauri's IPC; a plain browser does not. */
const tauriInvoke = (): ((cmd: string, args?: object) => Promise<unknown>) | undefined =>
  (window as any).__TAURI__?.core?.invoke;

/** Turning them on asks for permission the first time. */
export async function enable(): Promise<"on" | "blocked" | "unsupported"> {
  if (inMobileApp()) {
    const granted = await (window as any).SwitchyardMobile.requestNotifications();
    if (granted) localStorage.setItem(KEY, "on");
    return granted ? "on" : "blocked";
  }
  const invoke = tauriInvoke();
  let granted: boolean;
  if (invoke) {
    try {
      granted = (await invoke("plugin:notification|is_permission_granted")) === true;
      if (!granted) granted = (await invoke("plugin:notification|request_permission")) === "granted";
    } catch (err) {
      console.error("notification permission failed", err);
      return "unsupported";
    }
  } else {
    if (typeof Notification === "undefined") return "unsupported";
    granted = Notification.permission === "granted" || (Notification.permission !== "denied" && (await Notification.requestPermission()) === "granted");
  }
  if (granted) localStorage.setItem(KEY, "on");
  return granted ? "on" : "blocked";
}

export const HINT = {
  blocked: "Notifications are blocked. Allow them for this site in the browser's settings.",
  unsupported: "This browser cannot show notifications.",
};

/** What the switch shows for the stored choice and the browser's permission
 *  (`null`: the desktop app, which can only answer asynchronously). A
 *  permission revoked in the site's settings after the user turned them on
 *  would otherwise leave the switch saying On while nothing can show. */
export function switchState(stored: boolean, permission: NotificationPermission | "unsupported" | null): { on: boolean; hint: string | null } {
  if (!stored || permission === null || permission === "granted") return { on: stored, hint: null };
  if (permission === "denied") return { on: false, hint: HINT.blocked };
  if (permission === "unsupported") return { on: false, hint: HINT.unsupported };
  return { on: false, hint: null };
}

const permissionNow = (): NotificationPermission | "unsupported" | null =>
  tauriInvoke() || inMobileApp() ? null : typeof Notification === "undefined" ? "unsupported" : Notification.permission;

/** The sidebar switch: on or off, kept per browser. */
export function useNotifySetting() {
  const [initial] = useState(() => switchState(isEnabled(), permissionNow()));
  const [on, setOn] = useState(initial.on);
  const [hint, setHint] = useState<string | null>(initial.hint);
  const toggle = async () => {
    setHint(null);
    if (on) {
      localStorage.setItem(KEY, "off");
      setOn(false);
      return;
    }
    const result = await enable();
    if (result === "on") setOn(true);
    else setHint(HINT[result]);
  };
  return { on, hint, toggle };
}

/** How long after a desktop banner the window's next focus counts as a click on it. */
const CLICK_WINDOW_MS = 20_000;

/** Shows notes for the messages yardd sends. `open` takes the user to a session. */
export function createNotifier(opts: { openSessionId(): string | null; open(sessionId: string): void }) {
  const watcher = new StatusWatcher();
  const where = (): Where => ({
    enabled: isEnabled(),
    focused: document.hasFocus() && document.visibilityState === "visible",
    openSessionId: opts.openSessionId(),
  });
  const activate = (note: Note) => {
    window.focus();
    if (note.sessionId) opts.open(note.sessionId);
    else if (note.url) window.open(note.url, "_blank", "noopener,noreferrer");
  };

  // The desktop app reports a click on a banner through the plugin's
  // `actionPerformed` listener, with the `extra` we attached. Should the
  // listener not register, macOS still brings the window forward on a click,
  // so a focus just after a banner is taken for one.
  let listening: Promise<boolean> | null = null;
  const listen = () =>
    (listening ??= (async () => {
      const add = (window as any).__TAURI__?.core?.addPluginListener;
      if (!add) return false;
      try {
        await add("notification", "actionPerformed", (p: any) => {
          lastDesktop = null;
          const sessionId = p?.notification?.extra?.sessionId;
          if (typeof sessionId === "string") activate({ title: "", sessionId });
        });
        return true;
      } catch (err) {
        console.error("notification clicks are not reported", err);
        return false;
      }
    })());
  let lastDesktop: { sessionId: string; at: number } | null = null;
  window.addEventListener("focus", () => {
    const last = lastDesktop;
    lastDesktop = null;
    if (last && Date.now() - last.at < CLICK_WINDOW_MS) opts.open(last.sessionId);
  });

  const show = async (note: Note) => {
    if (inMobileApp()) {
      (window as any).SwitchyardMobile.notify(note);
      return;
    }
    const invoke = tauriInvoke();
    if (invoke) {
      try {
        const clicks = await listen();
        await invoke("plugin:notification|notify", { options: { title: note.title, body: note.body, extra: note.sessionId ? { sessionId: note.sessionId } : {} } });
        lastDesktop = !clicks && note.sessionId ? { sessionId: note.sessionId, at: Date.now() } : null;
      } catch (err) {
        console.error("notification failed", err);
      }
      return;
    }
    if (typeof Notification === "undefined" || Notification.permission !== "granted") return;
    // One banner per session: a newer one replaces the older.
    const n = new Notification(note.title, { body: note.body, tag: note.sessionId ? `yard:${note.sessionId}` : undefined });
    n.onclick = () => {
      n.close();
      activate(note);
    };
  };

  window.addEventListener("yard:notification", ((event: CustomEvent<Note>) => activate(event.detail)) as EventListener);
  return (msg: any) => {
    switch (msg.op) {
      case "snapshot":
        watcher.seed(msg.sessions ?? [], "local");
        break;
      case "host.snapshot":
        watcher.seed(msg.sessions ?? [], msg.hostId);
        break;
      case "event":
        if (msg.event) watcher.event(msg.sessionId, msg.event as AgentEvent);
        break;
      case "session.deleted":
        watcher.forget(msg.sessionId);
        break;
      case "session": {
        const session = msg.session as SessionMeta;
        const kind = watcher.change(session);
        if (kind && shouldNotify(where(), session.id)) void show({ ...noticeText(kind, session.title, watcher.info(session.id)), sessionId: session.id });
        break;
      }
      case "notify": {
        const sessionId = typeof msg.sessionId === "string" ? msg.sessionId : null;
        if (shouldNotify(where(), sessionId)) void show({ title: msg.title, body: msg.body, sessionId: sessionId ?? undefined, url: msg.url });
        break;
      }
    }
  };
}
