import { useEffect, useMemo, useReducer, useRef } from "react";
import { rememberSession } from "./sessionDefaults";
import { returnToComposer } from "./compose";
import { peekKey, peekResult, prPeeks, type PeekResult } from "./prPeek";
import {
  applyEvent,
  type AgentEvent,
  type BranchList,
  type ClaudeSessionList,
  type DirList,
  type EngineStates,
  type LoginState,
  type HostInstallMsg,
  type Changes,
  type Decision,
  type Engine,
  type Host,
  type HostEntry,
  type Item,
  type Models,
  type PrResult,
  type ProcRow,
  type KillReply,
  type SessionMeta,
  type Permission,
  type Usage,
  type Attachment,
  type QueueReturned,
  type AccountEntry,
  type Automation,
  type Agent,
  type McpAuthState,
  type McpStatus,
  type SlackView,
} from "./protocol";
import type { WidgetRecord } from "./widget";
import { createNotifier } from "./notify";
import { inMobileApp, type MobileStatus } from "./mobile";
import { base64ToBlob, fileToBase64, Replies, tooLarge } from "./attach";

/** Where an upload goes: a host from the new-session page, or the host of a
 *  session from its reply box. Empty means this machine. */
export type UploadTarget = { host?: string; sessionId?: string };

/** Thumbnails already fetched, as object URLs, by session and attachment.
 *  Kept for the page's life: the bytes never change. */
const thumbs = new Map<string, Promise<string>>();
/** Widgets already fetched, by session and widget id. They never change. */
const widgets = new Map<string, Promise<{ widget?: WidgetRecord; error?: string }>>();

export type Route =
  | { page: "new" }
  | { page: "hosts"; host?: string; tab?: "sessions" | "engines" | "system" }
  | { page: "subscriptions" }
  | { page: "sessions" }
  /** Every agent, as cards. */
  | { page: "agents" }
  /** An agent's one conversation (or, before its first message, the empty one). */
  | { page: "agent"; id: string }
  /** The create/edit form; `id` is `new` for a new agent. */
  | { page: "agentEdit"; id: string }
  | { page: "settings" }
  | { page: "session"; id: string }
  | { page: "automations" }
  /** Every engine account's MCP servers (connectors), per host. */
  | { page: "connectors" }
  /** The create/edit form; `id` is `new` for a new automation. */
  | { page: "automation"; host: string; id: string };

/** The URL hash for a route, so a reload or Back lands on the same screen. */
export function routeHash(route: Route): string {
  switch (route.page) {
    case "session":
      return `#/s/${encodeURIComponent(route.id)}`;
    case "agents":
      return "#/agents";
    case "agent":
      return `#/agents/${encodeURIComponent(route.id)}`;
    case "agentEdit":
      return `#/agents/${encodeURIComponent(route.id)}/edit`;
    case "hosts": {
      const query = new URLSearchParams();
      if (route.host) query.set("host", route.host);
      if (route.tab) query.set("tab", route.tab);
      return `#/hosts${query.size ? `?${query}` : ""}`;
    }
    case "subscriptions":
      return "#/subscriptions";
    case "sessions":
      return "#/sessions";
    case "settings":
      return "#/settings";
    case "automations":
      return "#/automations";
    case "connectors":
      return "#/connectors";
    case "automation":
      return `#/automations/${encodeURIComponent(route.host)}/${encodeURIComponent(route.id)}`;
    default:
      return "#/";
  }
}

export function parseHash(hash: string): Route {
  try {
    const s = /^#\/s\/(.+)$/.exec(hash);
    if (s) return { page: "session", id: decodeURIComponent(s[1]) };
    const e = /^#\/agents\/([^/]+)\/edit$/.exec(hash);
    if (e) return { page: "agentEdit", id: decodeURIComponent(e[1]) };
    const g = /^#\/agents\/([^/]+)$/.exec(hash);
    if (g) return { page: "agent", id: decodeURIComponent(g[1]) };
    const a = /^#\/automations\/([^/]+)\/([^/]+)$/.exec(hash);
    if (a) return { page: "automation", host: decodeURIComponent(a[1]), id: decodeURIComponent(a[2]) };
  } catch {
    return { page: "new" };
  }
  if (hash.startsWith("#/hosts?")) {
    const query = new URLSearchParams(hash.slice("#/hosts?".length));
    const host = query.get("host");
    const tab = query.get("tab");
    return { page: "hosts", ...(host ? { host } : {}), ...(tab === "sessions" || tab === "engines" || tab === "system" ? { tab } : {}) };
  }
  if (hash === "#/subscriptions") return { page: "subscriptions" };
  if (hash === "#/sessions") return { page: "sessions" };
  // Switchy's page became the agents; its question box is the host agent's.
  if (hash === "#/agents" || hash === "#/assistant") return { page: "agents" };
  if (hash === "#/automations") return { page: "automations" };
  if (hash === "#/settings") return { page: "settings" };
  if (hash === "#/connectors") return { page: "connectors" };
  return hash === "#/hosts" ? { page: "hosts" } : { page: "new" };
}

interface Thread {
  /** Next sequence number we expect; -1 until history has loaded. */
  next: number;
  items: Item[];
  /** Events that arrived before the history did. */
  early: { seq: number; event: AgentEvent }[];
}

export interface State {
  mobile: MobileStatus;
  /** The hub's Slack connection; null until the snapshot. */
  slack: SlackView | null;
  /** The hub's agents (agents live on this yardd only). */
  agents: Agent[];
  connected: boolean;
  route: Route;
  /** A session the URL named before it was listed (sessions arrive with the
   *  snapshot, a remote host's later still). Opened when it shows up. */
  restore: string | null;
  sessions: Record<string, SessionMeta>;
  threads: Record<string, Thread>;
  changes: Record<string, Changes>;
  pr: Record<string, PrResult>;
  /** The Processes tab's list per session: the latest `session.procs` reply. */
  procs: Record<string, ProcRow[]>;
  /** Remote hosts and their connection state. This Mac is not in the list. */
  hosts: HostEntry[];
  /** Per host id; `local` is this machine. */
  models: Record<string, Models>;
  usage: Record<string, Usage>;
  host: Record<string, Host>;
  /** This machine's name from the snapshot, before its first `host` stats sample. */
  localName: string;
  home: Record<string, string>;
  /** Where new sessions start and the folder picker opens, per host. */
  workspace: Record<string, string>;
  /** Engine install and login state per host; missing until asked. */
  engines: Record<string, EngineStates>;
  /** Added accounts per host (Default is implicit). */
  accounts: Record<string, AccountEntry[]>;
  /** Logins in progress, by `loginKey(host, engine, account)`. */
  logins: Record<string, LoginState>;
  /** Hosts whose connection on this socket dropped, until their next snapshot. */
  linkDown: Record<string, boolean>;
  /** Goes up whenever a host's sessions are reloaded, so open threads refetch. */
  hostEpoch: number;
  /** `git.branches` replies, by `branchKey(host, cwd)`. */
  branches: Record<string, BranchList>;
  /** `fs.dirs` replies, by `branchKey(host, path)`. */
  dirs: Record<string, DirList>;
  /** Claude Code sessions that can be imported, per host; missing until asked. */
  claudeSessions: Record<string, ClaudeSessionList>;
  /** Why the last import was refused, shown in the import dialog. */
  importError: string | null;
  /** Each host's own automations, by host id (`local` is this machine). */
  automations: Record<string, Automation[]>;
  /** MCP servers per engine account, by `mcpKey(host, engine, account)`.
   *  `loading` while a listing is on its way. */
  mcp: Record<string, McpStatus & { loading?: boolean }>;
  /** Sign-ins of MCP servers, by `mcpAuthKey(host, engine, account, server)`. */
  mcpAuth: Record<string, McpAuthState>;
  /** Servers being added or removed, by `mcpAuthKey(host, engine, account, name)`. */
  mcpChange: Record<string, { state: "adding" | "removing" | "done" | "failed"; error?: string }>;
  /** `host.install` progress by host id. */
  installs: Record<string, { lines: string[]; running: boolean; ok?: boolean }>;
  error: string | null;
  /** Goes up on every error, including a repeat of the same message. */
  errorCount: number;
  /** A quiet outcome worth showing once, such as a removed worktree. */
  notice: string | null;
}

export type SlackOp = "slack.tokens" | "slack.disconnect" | "slack.retry" | "slack.allow" | "slack.disallow" | "slack.dismiss" | "slack.route";

type Action =
  | { t: "socket"; connected: boolean }
  | { t: "route"; route: Route }
  | { t: "dismiss" }
  /** A decision on an approval was sent; its card waits for `approval_resolved`. */
  | { t: "deciding"; sessionId: string; requestId: string }
  | { t: "server"; msg: any };

const emptyThread = (): Thread => ({ next: -1, items: [], early: [] });

export const initial: State = {
  mobile: { configured: false },
  slack: null,
  agents: [],
  connected: false,
  route: { page: "new" },
  restore: null,
  sessions: {},
  threads: {},
  changes: {},
  pr: {},
  procs: {},
  hosts: [],
  models: {},
  usage: {},
  host: {},
  localName: "",
  home: {},
  workspace: {},
  engines: {},
  accounts: {},
  logins: {},
  linkDown: {},
  hostEpoch: 0,
  branches: {},
  dirs: {},
  claudeSessions: {},
  importError: null,
  automations: {},
  mcp: {},
  mcpAuth: {},
  mcpChange: {},
  installs: {},
  error: null,
  errorCount: 0,
  notice: null,
};

export function reduce(state: State, action: Action): State {
  switch (action.t) {
    case "socket":
      // A reconnect means a new yardd may be on the other end: reload threads.
      return action.connected ? { ...state, connected: true } : { ...state, connected: false, threads: {} };
    case "route":
      return go(state, action.route);
    case "dismiss":
      return { ...state, error: null, notice: null };
    case "deciding": {
      const thread = state.threads[action.sessionId];
      if (!thread) return state;
      const items = thread.items.map((it) =>
        it.kind === "approval" && it.requestId === action.requestId ? { ...it, sending: true } : it,
      );
      return { ...state, threads: { ...state.threads, [action.sessionId]: { ...thread, items } } };
    }
    case "server":
      return restored(reduceServer(state, action.msg));
  }
}

function releaseApprovals(threads: Record<string, Thread>): Record<string, Thread> {
  const out: Record<string, Thread> = {};
  for (const [id, thread] of Object.entries(threads)) {
    const held = thread.items.some((it) => it.kind === "approval" && it.sending);
    out[id] = held
      ? { ...thread, items: thread.items.map((it) => (it.kind === "approval" && it.sending ? { ...it, sending: false } : it)) }
      : thread;
  }
  return out;
}

/** Opens a route; a session not listed yet waits in `restore`. */
export function go(state: State, route: Route): State {
  if (route.page === "session" && !state.sessions[route.id]) return { ...state, route: { page: "new" }, restore: route.id };
  return { ...state, route, restore: null };
}

function restored(state: State): State {
  return state.restore && state.sessions[state.restore]
    ? { ...state, route: { page: "session", id: state.restore }, restore: null }
    : state;
}

const EMPTY_MODELS: Models = { claude: [], codex: [] };
const NO_USAGE: Usage = {};
const NO_ACCOUNTS: AccountEntry[] = [];

export const branchKey = (host: string, cwd: string) => `${host}\n${cwd}`;
/** Default's key is the same as before accounts existed. */
export const mcpKey = (host: string, engine: Engine, account = "default") => `${host}\n${engine}\n${account}`;
export const mcpAuthKey = (host: string, engine: Engine, account: string, server: string) => `${mcpKey(host, engine, account)}\n${server}`;
export const loginKey = (host: string, engine: Engine, account = "default") =>
  account === "default" ? `${host}\n${engine}` : `${host}\n${engine}\n${account}`;
export const accountsOn = (state: State, host: string): AccountEntry[] => state.accounts[host] ?? NO_ACCOUNTS;

/** Longest group name yardd keeps. */
export const GROUP_NAME_MAX = 60;
/** What yardd stores for a typed group name: trimmed and capped; empty means no group. */
export const cleanGroupName = (name: string) => name.trim().slice(0, GROUP_NAME_MAX);

export interface SidebarLayout {
  pinned: SessionMeta[];
  ungrouped: SessionMeta[];
  groups: { name: string; sessions: SessionMeta[] }[];
  archived: SessionMeta[];
}

/** The sidebar's sections. Pinned and ungrouped keep creation order; each
 *  group is newest activity first, groups sort by name, and a group with no
 *  live member is not listed. Pinned and archived sessions never show in a group. */
export function sidebarLayout(sessions: SessionMeta[]): SidebarLayout {
  const byCreated = [...sessions].sort((a, b) => b.createdAt - a.createdAt);
  const live = byCreated.filter((s) => !s.archived);
  const named = new Map<string, SessionMeta[]>();
  for (const s of live) {
    if (s.pinned || !s.group) continue;
    named.set(s.group, [...(named.get(s.group) ?? []), s]);
  }
  const groups = [...named.entries()]
    .map(([name, list]) => ({ name, sessions: list.sort((a, b) => b.updatedAt - a.updatedAt) }))
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) || (a.name < b.name ? -1 : 1));
  return {
    pinned: live.filter((s) => s.pinned),
    ungrouped: live.filter((s) => !s.pinned && !s.group),
    groups,
    archived: byCreated.filter((s) => s.archived),
  };
}

/** The host a session runs on, `local` for this machine. */
export const hostOf = (s: SessionMeta | undefined) => s?.host || "local";
export const modelsOn = (state: State, host: string): Models => state.models[host] ?? EMPTY_MODELS;
export const usageOn = (state: State, host: string): Usage => state.usage[host] ?? NO_USAGE;
export const homeOn = (state: State, host: string): string => state.home[host] ?? "";
/** The local name comes from yardd: "This Mac" on a Mac, the hostname or YARD_HOST_NAME elsewhere.
 *  The snapshot's `hostName` covers the seconds before the first `host` stats sample. */
export const hostName = (state: State, host: string): string =>
  host === "local" ? state.host.local?.name || state.localName || "This Mac" : state.hosts.find((h) => h.id === host)?.name ?? host;
/** An op's error as shown. A host's yardd that predates a feature this window
 *  offers answers "unknown op", which does not say what to do about it. */
export const hostErrorText = (state: State, host: string | undefined, message: string): string =>
  host && host !== "local" && message.startsWith("unknown op")
    ? `${hostName(state, host)} runs an older Switchyard. Use Update Switchyard on its card in Hosts, then try again.`
    : message;
/** A remote host's sessions are offline while the hub cannot reach it. */
export const hostOnline = (state: State, host: string): boolean =>
  host === "local"
    ? state.connected
    : state.connected && !state.linkDown[host] && state.hosts.some((h) => h.id === host && h.status === "connected");

function without<T>(rec: Record<string, T>, drop: (key: string) => boolean): Record<string, T> {
  return Object.fromEntries(Object.entries(rec).filter(([k]) => !drop(k)));
}

/** Replace one host's sessions, keeping everyone else's. */
function replaceHostSessions(state: State, host: string, list: SessionMeta[]): State {
  const gone = (id: string) => hostOf(state.sessions[id]) === host;
  const sessions = without(state.sessions, gone);
  for (const s of list) sessions[s.id] = s;
  // Events may have been missed while away: drop the threads so they reload from history.
  const threads = without(state.threads, gone);
  // The session we were looking at may be gone after a restart.
  const route =
    state.route.page === "session" && !sessions[state.route.id] ? ({ page: "new" } as Route) : state.route;
  return { ...state, sessions, threads, route, hostEpoch: state.hostEpoch + 1 };
}

function reduceServer(state: State, msg: any): State {
  switch (msg.op) {
    case "mobile.status":
      return msg.hostId ? state : { ...state, mobile: msg.mobile };
    case "agents":
      return msg.hostId ? state : { ...state, agents: (msg.agents as Agent[] | undefined) ?? [] };
    case "slack":
      return msg.hostId ? state : { ...state, slack: msg.slack as SlackView };
    case "snapshot": {
      const hosts = (msg.hosts ?? []) as HostEntry[];
      const known = new Set(hosts.map((h) => h.id));
      // Remote sessions stay listed (offline) until their host snapshot comes
      // in, unless their host was removed meanwhile.
      const next = replaceHostSessions({ ...state, hosts }, "local", msg.sessions as SessionMeta[]);
      const sessions = without(next.sessions, (id) => {
        const h = hostOf(next.sessions[id]);
        return h !== "local" && !known.has(h);
      });
      const route =
        next.route.page === "session" && !sessions[next.route.id] ? ({ page: "new" } as Route) : next.route;
      return {
        ...next,
        mobile: msg.mobile ?? { configured: false },
        slack: (msg.slack as SlackView | undefined) ?? null,
        agents: (msg.agents as Agent[] | undefined) ?? [],
        sessions,
        route,
        linkDown: {},
        models: { ...state.models, local: msg.models ?? EMPTY_MODELS },
        usage: { ...state.usage, local: msg.usage ?? {} },
        accounts: { ...state.accounts, local: msg.accounts ?? [] },
        host: msg.host ? { ...state.host, local: msg.host } : state.host,
        localName: msg.hostName ?? "",
        home: { ...state.home, local: msg.home ?? "" },
        workspace: { ...state.workspace, local: msg.workspace || msg.home || "" },
        engines: without(state.engines, (h) => h === "local"),
        automations: { ...state.automations, local: (msg.automations as Automation[] | undefined) ?? [] },
      };
    }
    case "host.snapshot": {
      const id = msg.hostId as string;
      const next = replaceHostSessions(state, id, msg.sessions as SessionMeta[]);
      return {
        ...next,
        linkDown: { ...state.linkDown, [id]: false },
        models: { ...state.models, [id]: msg.models ?? EMPTY_MODELS },
        usage: { ...state.usage, [id]: msg.usage ?? {} },
        accounts: { ...state.accounts, [id]: msg.accounts ?? [] },
        host: msg.host ? { ...state.host, [id]: msg.host } : state.host,
        home: { ...state.home, [id]: msg.home ?? "" },
        workspace: { ...state.workspace, [id]: msg.workspace || msg.home || "" },
        engines: without(state.engines, (h) => h === id),
        automations: { ...state.automations, [id]: (msg.automations as Automation[] | undefined) ?? [] },
      };
    }
    case "accounts":
      return { ...state, accounts: { ...state.accounts, [msg.hostId ?? "local"]: msg.accounts ?? [] } };
    case "host.down":
      return { ...state, linkDown: { ...state.linkDown, [msg.hostId]: true } };
    case "hosts": {
      const hosts = msg.hosts as HostEntry[];
      const known = new Set(["local", ...hosts.map((h) => h.id)]);
      const gone = (id: string) => !known.has(hostOf(state.sessions[id]));
      const sessions = without(state.sessions, gone);
      const route =
        state.route.page === "session" && !sessions[state.route.id] ? ({ page: "new" } as Route) : state.route;
      return {
        ...state,
        hosts,
        sessions,
        route,
        threads: without(state.threads, gone),
        host: without(state.host, (h) => !known.has(h)),
        models: without(state.models, (h) => !known.has(h)),
        accounts: without(state.accounts, (h) => !known.has(h)),
        automations: without(state.automations, (h) => !known.has(h)),
      };
    }
    case "session":
      return { ...state, sessions: { ...state.sessions, [msg.session.id]: msg.session } };
    case "session.deleted": {
      const id = msg.sessionId as string;
      const { [id]: _s, ...sessions } = state.sessions;
      const { [id]: _t, ...threads } = state.threads;
      const { [id]: _c, ...changes } = state.changes;
      const { [id]: _p, ...procs } = state.procs;
      const route = state.route.page === "session" && state.route.id === id ? ({ page: "new" } as Route) : state.route;
      return { ...state, sessions, threads, changes, procs, route };
    }
    case "created":
      return { ...state, route: { page: "session", id: msg.sessionId }, restore: null };
    case "models":
      return { ...state, models: { ...state.models, [msg.hostId ?? "local"]: msg.models } };
    case "usage":
      return { ...state, usage: { ...state.usage, [msg.hostId ?? "local"]: msg.usage } };
    case "host":
      return { ...state, host: { ...state.host, [msg.hostId ?? "local"]: msg.host } };
    case "changes":
      return { ...state, changes: { ...state.changes, [msg.sessionId]: msg.changes } };
    case "pr":
      return { ...state, pr: { ...state.pr, [msg.sessionId]: msg.pr } };
    case "session.procs":
      return { ...state, procs: { ...state.procs, [msg.sessionId]: msg.procs } };
    case "error": {
      // A host on an older Switchyard has no engine status; its card offers Update.
      if (msg.for === "engines.status" || msg.for === "session.procs") return state;
      // The import dialog says why; no banner behind it.
      if (msg.for === "session.import") return { ...state, importError: msg.message };
      // The Connectors page shows it on the host's cards instead of waiting forever.
      if (msg.for === "mcp.add" || msg.for === "mcp.remove") {
        const host = msg.hostId ?? "local";
        const mcpChange = { ...state.mcpChange };
        for (const [key, ch] of Object.entries(mcpChange)) if (key.startsWith(`${host}\n`) && (ch.state === "adding" || ch.state === "removing")) mcpChange[key] = { state: "failed", error: msg.message };
        return { ...state, mcpChange };
      }
      if (typeof msg.for === "string" && msg.for.startsWith("mcp.")) {
        const host = msg.hostId ?? "local";
        const error = /unknown op/.test(msg.message ?? "") ? "This host's Switchyard is older and cannot list connectors. Update it from the Hosts page." : msg.message;
        const mcp = { ...state.mcp };
        for (const [key, st] of Object.entries(mcp)) if (key.startsWith(`${host}\n`) && st.loading) mcp[key] = { ...st, loading: false, error };
        return { ...state, mcp };
      }
      // The refusal names no request, so every waiting card gets its buttons back.
      const threads = msg.for === "session.approve" ? releaseApprovals(state.threads) : state.threads;
      const error = hostErrorText(state, msg.hostId, msg.message);
      return { ...state, threads, error, errorCount: state.errorCount + 1 };
    }
    case "git.branches": {
      const reply = msg as BranchList;
      return { ...state, branches: { ...state.branches, [branchKey(reply.hostId ?? "local", reply.cwd)]: reply } };
    }
    case "host.install": {
      const m = msg as HostInstallMsg;
      const prev = state.installs[m.hostId];
      // The first line of a new run starts a fresh log.
      const base = prev?.running ? prev : { lines: [], running: true };
      const next = m.done
        ? { ...(prev ?? base), running: false, ok: !!m.ok }
        : { ...base, running: true, ok: undefined, lines: [...base.lines, m.line ?? ""].slice(-200) };
      return { ...state, installs: { ...state.installs, [m.hostId]: next } };
    }
    case "automations":
      return { ...state, automations: { ...state.automations, [msg.hostId ?? "local"]: msg.automations as Automation[] } };
    case "engines":
      return {
        ...state,
        engines: { ...state.engines, [msg.hostId ?? "local"]: { ...msg.engines, accounts: msg.accounts ?? {} } as EngineStates },
      };
    case "login": {
      const key = loginKey(msg.hostId ?? "local", msg.engine, msg.account);
      const { op: _op, hostId: _h, engine: _e, account: _a, ...login } = msg;
      return { ...state, logins: { ...state.logins, [key]: login as LoginState } };
    }
    case "mcp": {
      const key = mcpKey(msg.hostId ?? "local", msg.engine, msg.account);
      const { op: _op, hostId: _h, ...status } = msg;
      return { ...state, mcp: { ...state.mcp, [key]: status as McpStatus } };
    }
    // Local: a listing was asked for; the last one stays on screen meanwhile.
    case "mcp.loading": {
      const key = mcpKey(msg.host, msg.engine, msg.account);
      const cur = state.mcp[key] ?? { engine: msg.engine, account: msg.account ?? "default" };
      return { ...state, mcp: { ...state.mcp, [key]: { ...cur, loading: true } } };
    }
    case "mcp.auth": {
      const key = mcpAuthKey(msg.hostId ?? msg.host ?? "local", msg.engine, msg.account ?? "default", msg.server);
      if (msg.state === null) {
        const { [key]: _gone, ...rest } = state.mcpAuth;
        return { ...state, mcpAuth: rest };
      }
      const { op: _op, hostId: _h, host: _ho, engine: _e, account: _a, server: _s, ...auth } = msg;
      return { ...state, mcpAuth: { ...state.mcpAuth, [key]: auth as McpAuthState } };
    }
    case "mcp.change": {
      const key = mcpAuthKey(msg.hostId ?? msg.host ?? "local", msg.engine, msg.account ?? "default", msg.name);
      if (msg.state === null) {
        const { [key]: _gone, ...rest } = state.mcpChange;
        return { ...state, mcpChange: rest };
      }
      return { ...state, mcpChange: { ...state.mcpChange, [key]: { state: msg.state, ...(msg.error ? { error: msg.error } : {}) } } };
    }
    // Local: a new import (or a closed dialog) forgets the last refusal.
    case "import.started":
      return { ...state, importError: null };
    case "claude.sessions": {
      const reply = msg as ClaudeSessionList;
      return { ...state, claudeSessions: { ...state.claudeSessions, [reply.hostId ?? "local"]: reply } };
    }
    case "fs.dirs": {
      const reply = msg as DirList;
      return { ...state, dirs: { ...state.dirs, [branchKey(reply.hostId ?? "local", reply.path)]: reply } };
    }
    case "worktree.removed":
      // A kept branch is the user's to deal with, so the outcome is shown.
      return { ...state, notice: msg.message };
    case "history": {
      const prev = state.threads[msg.sessionId] ?? emptyThread();
      const events = msg.events as AgentEvent[];
      let items: Item[] = [];
      events.forEach((ev, seq) => (items = applyEvent(items, ev, seq)));
      let next = events.length;
      for (const e of prev.early.sort((a, b) => a.seq - b.seq)) {
        if (e.seq !== next) continue;
        items = applyEvent(items, e.event, e.seq);
        next++;
      }
      return { ...state, threads: { ...state.threads, [msg.sessionId]: { next, items, early: [] } } };
    }
    case "event": {
      const thread = state.threads[msg.sessionId] ?? emptyThread();
      const seq = msg.seq as number;
      let updated: Thread;
      if (thread.next < 0) {
        updated = { ...thread, early: [...thread.early, { seq, event: msg.event }] };
      } else if (seq === thread.next) {
        updated = { ...thread, next: seq + 1, items: applyEvent(thread.items, msg.event, seq) };
      } else {
        return state; // already applied through history
      }
      return { ...state, threads: { ...state.threads, [msg.sessionId]: updated } };
    }
    default:
      return state;
  }
}

export interface Actions {
  /** A message for an agent: into its one conversation, which opens.
   *  Resolves with an error fit to show when yardd refused it. */
  askAgent(agentId: string, text: string, attachments?: string[]): Promise<{ sessionId?: string; error?: string }>;
  /** Create (no `id`) or change an agent; only the fields given change. */
  saveAgent(agent: Partial<Agent>): Promise<{ agent?: Agent; error?: string }>;
  /** Its chats stay, as plain sessions. The built-in agent cannot be deleted. */
  deleteAgent(id: string): void;
  /** A window opened the agent's conversation: its call for the owner (`needsYou`) was seen. */
  agentSeen(id: string): void;

  go(route: Route): void;
  dismissError(): void;
  create(input: {
    engine: Engine;
    model: string;
    effort: string;
    serviceTier?: string;
    cwd: string;
    text: string;
    permission?: Permission;
    /** An added account of the engine; its Default when left out. */
    account?: string;
    /** A worktree when the folder is in a repository; otherwise it runs in place. */
    worktree: boolean;
    branch?: string;
    /** Branch or ref the worktree is cut from; the folder's HEAD when left out. */
    base?: string;
    /** A remote host id; this machine when left out. */
    host?: string;
    /** Ids from `upload` to the same host. */
    attachments?: string[];
  }): void;
  /** Raw send, for the terminal's byte stream. */
  post(msg: object): void;
  /** Terminal messages (`term.*`) skip the reducer and go to listeners. */
  onTerm(listener: (msg: any) => void): () => void;
  send(sessionId: string, text: string, attachments?: string[]): void;
  /** Store a file on the host that will run the prompt. Rejects with a
   *  message fit to show, such as the size limit or an offline host. */
  upload(file: File, target: UploadTarget): Promise<Attachment>;
  /** An object URL for an attachment of a session, for a thumbnail. */
  attachmentUrl(sessionId: string, attachment: Attachment): Promise<string>;
  setModel(sessionId: string, model: string, effort: string): void;
  setServiceTier(sessionId: string, serviceTier: string): void;
  /** Change what the engine may do unasked; Claude switches at once, Codex from its next turn. */
  setPermission(sessionId: string, permission: Permission): void;
  /** Child session in the same folder, seeded with a brief. */
  handoff(sessionId: string, engine: Engine, model: string, effort: string, account?: string): void;
  interrupt(sessionId: string): void;
  approve(sessionId: string, requestId: string, decision: Decision): void;
  loadChanges(sessionId: string): void;
  /** Start a queued session even though the host is under pressure. */
  startNow(sessionId: string): void;
  /** Stops the session and removes it everywhere. Its worktree stays unless
   *  `removeWorktree`, which yardd refuses for a worktree with changes. */
  remove(sessionId: string, removeWorktree?: boolean): void;
  loadBranches(cwd: string, host: string): void;
  /** Subfolders of an absolute path on a host, for the folder picker. */
  listDirs(path: string, host: string): void;
  rename(sessionId: string, title: string): Promise<{ error?: string }>;
  /** Sidebar housekeeping. */
  setPinned(sessionId: string, pinned: boolean): void;
  /** Take back a message queued during a turn: to the reply box, or dropped. */
  unqueue(sessionId: string, id: string, drop?: boolean): void;
  steerQueued(sessionId: string, id: string): void;
  setArchived(sessionId: string, archived: boolean): void;
  /** Put a session in a named group, or take it out with null. */
  setGroup(sessionId: string, group: string | null): void;
  /** A drop in the sidebar: the pin and the group in one update (`dropUpdate`). */
  place(sessionId: string, update: { pinned?: boolean; group?: string | null }): void;
  /** Log an engine in on a host; progress arrives as `login` messages. */
  login(host: string, engine: Engine, account?: string): void;
  /** The code Claude's sign-in page gives back; for OpenCode, an API key or a sign-in code. */
  loginCode(host: string, engine: Engine, code: string, account?: string): void;
  /** OpenCode: the provider and way in picked from those its login offered. */
  loginChoose(host: string, provider: string, method: number, account?: string): void;
  /** Ask a host for its Claude Code sessions (`claude.sessions`). */
  listClaudeSessions(host: string): void;
  /** Import one as a session there; `created` opens it. */
  importClaudeSession(host: string, claudeSessionId: string): void;
  clearImportError(): void;
  loginCancel(host: string, engine: Engine, account?: string): void;
  /** An extra login for an engine on a host, with its own config folder there. */
  addAccount(host: string, engine: Engine, name: string): void;
  /** Deletes the account's folder and login; refused while a session uses it. */
  removeAccount(host: string, id: string): void;
  refreshUsage(host: string, engine: Engine, account: string): Promise<{ error?: string }>;
  loadPr(sessionId: string): void;
  loadProcs(sessionId: string): void;
  /** Signal one of the session's processes (`session.kill`); with `tree`, it
   *  and everything below it. Resolves with yardd's verdict, or `error` (with
   *  the `hostId` of a remote that refused the op) for `hostErrorText`. */
  killProc(sessionId: string, pid: number, start: number | undefined, signal: "term" | "kill", tree: boolean): Promise<KillReply | { error: string; hostId?: string }>;
  /** A PR link's hover card, through the session's host; cached per host and URL. */
  peekPr(sessionId: string, url: string): Promise<PeekResult>;
  addHost(input: { name: string; kind: "ssh" | "direct"; destination?: string; remotePort?: number; url?: string }): void;
  removeHost(hostId: string): void;
  /** Install or update yardd on an SSH host from the release, over SSH. */
  installHost(hostId: string): void;
  /** Create (no `id`) or change an automation on a host; only the fields given change.
   *  Resolves with the saved automation, or an error fit to show. */
  saveAutomation(host: string, automation: Partial<Automation>): Promise<{ automation?: Automation; error?: string }>;
  deleteAutomation(host: string, id: string): void;
  /** Run now: starts a run whatever the skip rules say. Resolves with what happened. */
  runAutomation(host: string, id: string): Promise<{ outcome?: string; sessionId?: string; reason?: string; error?: string }>;
  /** List an engine account's MCP servers on a host; answers as `mcp`. */
  mcpStatus(host: string, engine: Engine, account?: string): void;
  /** Start signing a server in; progress arrives as `mcp.auth`. */
  mcpAuth(host: string, engine: Engine, account: string, server: string): void;
  /** The address the browser ended on after a sign-in, for a host it cannot reach. */
  mcpCallback(host: string, engine: Engine, account: string, server: string, url: string): void;
  mcpReconnect(host: string, engine: Engine, account: string, server: string): void;
  /** Add a remote MCP server to an account's own config; progress as `mcp.change`. */
  mcpAdd(host: string, engine: Engine, account: string, name: string, url: string): void;
  /** Take a server out of an account's own config. */
  mcpRemove(host: string, engine: Engine, account: string, server: string): void;
  mcpChangeClear(host: string, engine: Engine, account: string, name: string): void;
  /** Forget a finished or abandoned sign-in on this page. */
  mcpAuthClear(host: string, engine: Engine, account: string, server: string): void;
  /** Slack on the hub: `slack.tokens`, `slack.allow`, `slack.route`, ...; answers as `slack`. */
  slack(op: SlackOp, fields?: Record<string, unknown>): void;
  /** A tool call's widget: its HTML, CSP, input and result. */
  widget(sessionId: string, id: string): Promise<{ widget?: WidgetRecord; error?: string }>;
  /** A call a widget makes to its own server. */
  widgetCall(sessionId: string, id: string, tool: string, args: Record<string, unknown>): Promise<{ result?: unknown; error?: string }>;
}

function socketUrl(): string {
  const override = (import.meta as any).env?.VITE_YARD_WS as string | undefined;
  if (override) return override;
  // Behind a TLS reverse proxy (the container image) the page is https, and
  // the browser refuses a plain ws:// socket from it.
  return `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`;
}

const onHost = (host: string, msg: object) => (host === "local" ? msg : { ...msg, host });

export function useYard(): { state: State; actions: Actions } {
  const [state, dispatch] = useReducer(reduce, initial, (s) => go(s, parseHash(location.hash)));
  const socket = useRef<WebSocket | null>(null);
  const opened = useRef(new Set<string>());
  const termListeners = useRef(new Set<(msg: any) => void>());
  const replies = useRef(new Replies());
  const routeRef = useRef(state.route);
  routeRef.current = state.route;
  const sessionsRef = useRef(state.sessions);
  sessionsRef.current = state.sessions;
  const agentsRef = useRef(state.agents);
  agentsRef.current = state.agents;
  const notifier = useRef<((msg: any) => void) | null>(null);
  notifier.current ??= createNotifier({
    openSessionId: () => {
      const route = routeRef.current;
      if (route.page === "session") return route.id;
      // An agent's page shows its conversation.
      return route.page === "agent" ? (agentsRef.current.find((a) => a.id === route.id)?.chat ?? null) : null;
    },
    open: (id) => {
      const session = sessionsRef.current[id];
      if (!session) return;
      // A banner about an agent's turn opens the agent's conversation.
      const agent = session.agent && (!session.host || session.host === "local") ? agentsRef.current.find((a) => a.id === session.agent) : undefined;
      dispatch({ t: "route", route: agent ? { page: "agent", id: agent.id } : { page: "session", id } });
    },
  });

  useEffect(() => {
    let stopped = false;
    let retry: ReturnType<typeof setTimeout> | undefined;
    const connect = () => {
      const ws = new WebSocket(socketUrl());
      socket.current = ws;
      ws.onopen = () => {
        opened.current.clear();
        dispatch({ t: "socket", connected: true });
      };
      ws.onmessage = (e) => {
        let msg: any;
        try {
          msg = JSON.parse(e.data);
        } catch {
          return; // ignore malformed frames
        }
        // Upload and thumbnail replies go to whoever asked, not the reducer.
        if (msg.op === "snapshot" && inMobileApp()) {
          if (msg.mobile) ws.send(JSON.stringify({ op: "mobile.connected" }));
          (window as any).SwitchyardMobile.connected();
        }
        if (replies.current.take(msg)) return;
        notifier.current!(msg);
        if (msg.op === "error" && msg.sessionId && (msg.for === "term.open" || msg.for === "term.new")) {
          termListeners.current.forEach((listener) => listener({ ...msg, op: "term.error" }));
        } else if (typeof msg.op === "string" && msg.op.startsWith("term.")) {
          termListeners.current.forEach((listener) => listener(msg));
        } else if (msg.op === "queue.returned") {
          // Stop or Edit gave queued text back: it goes to the reply box.
          const back = msg as QueueReturned;
          if (back.text || back.attachments?.length) returnToComposer(back.sessionId, back.text ?? "", back.attachments);
        } else {
          dispatch({ t: "server", msg });
        }
      };
      ws.onclose = () => {
        replies.current.failAll("The connection to yardd dropped. Try again.");
        dispatch({ t: "socket", connected: false });
        if (!stopped) retry = setTimeout(connect, 1500);
      };
    };
    connect();
    // iOS can suspend a socket without delivering Close until much later.
    const resume = () => {
      clearTimeout(retry);
      const ws = socket.current;
      if (ws) {
        ws.onclose = null;
        ws.close();
      }
      replies.current.failAll("Reconnecting to yardd.");
      dispatch({ t: "socket", connected: false });
      connect();
    };
    window.addEventListener("yard:resume", resume);
    return () => {
      stopped = true;
      window.removeEventListener("yard:resume", resume);
      clearTimeout(retry);
      socket.current?.close();
    };
  }, []);

  const post = (msg: object) => {
    const ws = socket.current;
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(msg));
  };

  /** Send an op with a fresh `ref` and wait for the reply that carries it. */
  const ask = async (msg: object, timeoutMs: number): Promise<any> => {
    const ws = socket.current;
    if (!ws || ws.readyState !== WebSocket.OPEN) return { error: "Not connected to yardd." };
    const ref = replies.current.ref();
    const reply = replies.current.wait(ref, timeoutMs);
    ws.send(JSON.stringify({ ...msg, ref }));
    return reply;
  };

  const upload = async (file: File, target: UploadTarget): Promise<Attachment> => {
    const big = tooLarge(file.size);
    if (big) throw new Error(big);
    const data = await fileToBase64(file);
    // A slow link to a remote host can take a while for 10 MB.
    const reply = await ask({ op: "attach.put", name: file.name, mime: file.type, data, ...target }, 5 * 60_000);
    if (reply.error || !reply.attachment) throw new Error(reply.error ?? "The upload failed.");
    return reply.attachment as Attachment;
  };

  const attachmentUrl = (sessionId: string, a: Attachment): Promise<string> => {
    const key = `${sessionId}\n${a.id}`;
    let url = thumbs.get(key);
    if (!url) {
      url = ask({ op: "attach.get", sessionId, id: a.id }, 2 * 60_000).then((reply) => {
        if (reply.error || typeof reply.data !== "string") throw new Error(reply.error ?? "No data.");
        return URL.createObjectURL(base64ToBlob(reply.data, reply.mime || a.mime));
      });
      // A failure (host offline) may succeed later, so it is not kept.
      url.catch(() => thumbs.delete(key));
      thumbs.set(key, url);
    }
    return url;
  };

  // Load a session's history the first time it is shown on this connection.
  // A host snapshot drops that host's threads, so reload whatever is gone.
  // An agent's page shows its conversation.
  const agentChat = state.route.page === "agent" ? (state.agents.find((a) => a.id === (state.route as { id: string }).id)?.chat ?? null) : null;
  const routeId = state.route.page === "session" ? state.route.id : agentChat && state.sessions[agentChat] ? agentChat : null;
  const routeHost = routeId ? hostOf(state.sessions[routeId]) : "local";
  const online = routeId ? hostOnline(state, routeHost) : false;
  const loaded = routeId ? !!state.threads[routeId] : false;
  useEffect(() => {
    if (routeId && !loaded) opened.current.delete(routeId);
    if (!routeId || !online || opened.current.has(routeId)) return;
    opened.current.add(routeId);
    post({ op: "session.open", sessionId: routeId });
    post({ op: "changes.get", sessionId: routeId });
  }, [routeId, online, state.hostEpoch]);

  // Keep the open screen in the URL: phone browsers reload tabs they
  // discarded, and Back should move between sessions.
  const hash = routeHash(state.route);
  useEffect(() => {
    if (state.restore || location.hash === hash) return;
    const shown = parseHash(location.hash);
    // A blank first URL, or a session that was deleted, is not worth a Back stop.
    const stale = !location.hash || (shown.page === "session" && !state.sessions[shown.id]);
    if (stale) history.replaceState(null, "", hash);
    else history.pushState(null, "", hash);
  }, [hash, state.restore]);
  useEffect(() => {
    const back = () => dispatch({ t: "route", route: parseHash(location.hash) });
    window.addEventListener("popstate", back);
    return () => window.removeEventListener("popstate", back);
  }, []);

  // Ask each reachable host whether its engines are logged in, once per snapshot.
  const unknownEngines = state.connected
    ? ["local", ...state.hosts.map((h) => h.id)].filter((h) => hostOnline(state, h) && state.host[h] && !state.engines[h]).join(" ")
    : "";
  useEffect(() => {
    for (const h of unknownEngines.split(" ").filter(Boolean)) post(onHost(h, { op: "engines.status" }));
  }, [unknownEngines]);

  const saveAutomation = async (host: string, automation: Partial<Automation>) => {
    const reply = await ask(onHost(host, { op: "automation.save", automation }), 30_000);
    return reply.error || !reply.automation ? { error: reply.error ?? "The automation was not saved." } : { automation: reply.automation as Automation };
  };
  const killProc = async (sessionId: string, pid: number, start: number | undefined, signal: "term" | "kill", tree: boolean) => {
    // yardd waits up to 3 s to see whether the process ended.
    const reply = await ask({ op: "session.kill", sessionId, pid, start, signal, tree }, 30_000);
    return reply.error ? { error: reply.error as string, hostId: reply.hostId as string | undefined } : (reply as KillReply);
  };
  const runAutomation = async (host: string, id: string) => {
    // A run cuts a worktree and may run its setup first.
    const reply = await ask(onHost(host, { op: "automation.run", id, force: true }), 5 * 60_000);
    return reply.error ? { error: reply.error as string } : { outcome: reply.outcome, sessionId: reply.sessionId, reason: reply.reason };
  };

  const actions = useMemo<Actions>(
    () => ({
      askAgent: async (agentId, text, attachments) => {
        // Answered as soon as the conversation is known; the turn goes on after.
        const reply = await ask({ op: "agent.ask", agentId, text, attachments }, 60_000);
        if (reply.op !== "created" || typeof reply.sessionId !== "string") return { error: reply.error ?? "The message was not sent." };
        dispatch({ t: "route", route: { page: "agent", id: agentId } });
        return { sessionId: reply.sessionId };
      },
      saveAgent: async (agent) => {
        const reply = await ask({ op: "agent.save", agent }, 30_000);
        return reply.error || !reply.agent ? { error: reply.error ?? "The agent was not saved." } : { agent: reply.agent as Agent };
      },
      deleteAgent: (id) => post({ op: "agent.delete", id }),
      agentSeen: (id) => post({ op: "agent.seen", agentId: id }),
      go: (route) => dispatch({ t: "route", route }),
      dismissError: () => dispatch({ t: "dismiss" }),
      create: (input) => post({ op: "session.create", ...input }),
      post,
      onTerm: (listener) => {
        termListeners.current.add(listener);
        return () => termListeners.current.delete(listener);
      },
      send: (sessionId, text, attachments) => {
        const session = sessionsRef.current[sessionId];
        if (session) rememberSession(session);
        post(attachments?.length ? { op: "session.send", sessionId, text, attachments } : { op: "session.send", sessionId, text });
      },
      upload,
      attachmentUrl,
      setModel: (sessionId, model, effort) => {
        const session = sessionsRef.current[sessionId];
        if (session) rememberSession({ ...session, model, effort });
        post({ op: "session.setModel", sessionId, model, effort });
      },
      setServiceTier: (sessionId, serviceTier) => post({ op: "session.setServiceTier", sessionId, serviceTier }),
      setPermission: (sessionId, permission) => post({ op: "session.setPermission", sessionId, permission }),
      handoff: (sessionId, engine, model, effort, account) => post({ op: "session.handoff", sessionId, engine, model, effort, account }),
      interrupt: (sessionId) => post({ op: "session.interrupt", sessionId }),
      approve: (sessionId, requestId, decision) => {
        // Until yardd resolves it, a second click (a double click) would send a second decision.
        dispatch({ t: "deciding", sessionId, requestId });
        post({ op: "session.approve", sessionId, requestId, decision });
      },
      loadChanges: (sessionId) => post({ op: "changes.get", sessionId }),
      startNow: (sessionId) => post({ op: "session.startNow", sessionId }),
      remove: (sessionId, removeWorktree) =>
        post(removeWorktree ? { op: "session.delete", sessionId, removeWorktree: true } : { op: "session.delete", sessionId }),
      loadBranches: (cwd, host) =>
        post(host === "local" ? { op: "git.branches", cwd } : { op: "git.branches", cwd, host }),
      listDirs: (path, host) => post(host === "local" ? { op: "fs.dirs", path } : { op: "fs.dirs", path, host }),
      refreshUsage: async (host, engine, account) => {
        const reply = await ask(onHost(host, { op: "usage.refresh", engine, account }), 65_000);
        return reply.error ? { error: String(reply.error) } : {};
      },
      loadPr: (sessionId) => post({ op: "pr.get", sessionId, linkOnly: true }),
      loadProcs: (sessionId) => post({ op: "session.procs", sessionId }),
      killProc,
      // gh gives up after 30 s; a remote host adds its own round trip.
      peekPr: (sessionId, url) => prPeeks.get(peekKey(sessionId, url), () => ask({ op: "pr.peek", sessionId, url }, 40_000).then(peekResult)),
      addHost: (input) => post({ op: "host.add", ...input }),
      removeHost: (hostId) => post({ op: "host.remove", hostId }),
      installHost: (hostId) => post({ op: "host.install", hostId }),
      saveAutomation,
      runAutomation,
      deleteAutomation: (host, id) => post(onHost(host, { op: "automation.delete", id })),
      rename: async (sessionId, title) => {
        const reply = await ask({ op: "session.rename", sessionId, title }, 10_000);
        return reply.op === "session.renamed" ? {} : { error: reply.error ?? "Couldn’t rename the session." };
      },
      setPinned: (sessionId, pinned) => post({ op: "session.update", sessionId, pinned }),
      unqueue: (sessionId, id, drop) => post(drop ? { op: "session.unqueue", sessionId, id, drop: true } : { op: "session.unqueue", sessionId, id }),
      steerQueued: (sessionId, id) => post({ op: "session.steer", sessionId, id }),
      setArchived: (sessionId, archived) => post({ op: "session.update", sessionId, archived }),
      setGroup: (sessionId, group) => post({ op: "session.update", sessionId, group }),
      place: (sessionId, update) => post({ op: "session.update", sessionId, ...update }),
      login: (host, engine, account) => post(onHost(host, { op: "login.start", engine, account })),
      loginCode: (host, engine, code, account) => post(onHost(host, { op: "login.code", engine, code, account })),
      loginChoose: (host, provider, method, account) => post(onHost(host, { op: "login.choose", engine: "opencode", provider, method, account })),
      listClaudeSessions: (host) => post(onHost(host, { op: "claude.sessions" })),
      importClaudeSession: (host, claudeSessionId) => {
        dispatch({ t: "server", msg: { op: "import.started" } });
        post(onHost(host, { op: "session.import", claudeSessionId }));
      },
      clearImportError: () => dispatch({ t: "server", msg: { op: "import.started" } }),
      loginCancel: (host, engine, account) => post(onHost(host, { op: "login.cancel", engine, account })),
      addAccount: (host, engine, name) => post(onHost(host, { op: "account.add", engine, name })),
      removeAccount: (host, id) => post(onHost(host, { op: "account.remove", id })),
      mcpStatus: (host, engine, account = "default") => {
        dispatch({ t: "server", msg: { op: "mcp.loading", host, engine, account } });
        post(onHost(host, { op: "mcp.status", engine, account }));
      },
      mcpAuth: (host, engine, account, server) => {
        dispatch({ t: "server", msg: { op: "mcp.auth", host, engine, account, server, state: "starting" } });
        post(onHost(host, { op: "mcp.auth", engine, account, server }));
      },
      mcpCallback: (host, engine, account, server, url) => {
        dispatch({ t: "server", msg: { op: "mcp.auth", host, engine, account, server, state: "sending" } });
        post(onHost(host, { op: "mcp.callback", engine, account, server, url }));
      },
      mcpReconnect: (host, engine, account, server) => {
        dispatch({ t: "server", msg: { op: "mcp.loading", host, engine, account } });
        post(onHost(host, { op: "mcp.reconnect", engine, account, server }));
      },
      mcpAdd: (host, engine, account, name, url) => {
        dispatch({ t: "server", msg: { op: "mcp.change", host, engine, account, name, state: "adding" } });
        post(onHost(host, { op: "mcp.add", engine, account, name, url }));
      },
      mcpRemove: (host, engine, account, server) => {
        dispatch({ t: "server", msg: { op: "mcp.change", host, engine, account, name: server, state: "removing" } });
        post(onHost(host, { op: "mcp.remove", engine, account, server }));
      },
      mcpChangeClear: (host, engine, account, name) => dispatch({ t: "server", msg: { op: "mcp.change", host, engine, account, name, state: null } }),
      mcpAuthClear: (host, engine, account, server) => dispatch({ t: "server", msg: { op: "mcp.auth", host, engine, account, server, state: null } }),
      slack: (op, fields = {}) => post({ ...fields, op }),
      widget: (sessionId, id) => {
        const key = `${sessionId}\n${id}`;
        let p = widgets.get(key);
        if (!p) {
          p = ask({ op: "widget.get", sessionId, id }, 60_000).then((r) => (r.widget ? { widget: r.widget as WidgetRecord } : { error: r.error ?? "The widget did not load." }));
          // A failure (host offline) is tried again next time.
          p.then((r) => r.error && widgets.delete(key));
          widgets.set(key, p);
        }
        return p;
      },
      widgetCall: (sessionId, id, tool, args) =>
        ask({ op: "widget.call", sessionId, id, tool, arguments: args }, 150_000).then((r) => (r.error ? { error: String(r.error) } : { result: r.result })),
    }),
    [],
  );

  return { state, actions };
}
