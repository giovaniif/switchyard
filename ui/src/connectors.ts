// Words and order for the Connectors page: the MCP servers of every engine
// account on a host, merged into one service each (Linear is one card, however
// many accounts and sources list it), the ones that need the user first.
import type { ConnectorEngine, McpServer } from "./protocol";

const SOURCE: Record<string, string> = {
  claudeai: "claude.ai",
  plugin: "Plugin",
  user: "Your config",
  local: "This folder only",
  project: "Project .mcp.json",
  dynamic: "Added by an app",
  managed: "Managed",
  enterprise: "Managed",
  agent: "Agent",
  config: "config.toml",
  chatgpt: "ChatGPT app",
  switchyard: "Switchyard",
};

/** "claude.ai Datadog" -> Datadog via claude.ai; "plugin:engineering:datadog"
 *  -> datadog via the engineering plugin. */
export function displayName(s: Pick<McpServer, "name" | "source" | "origin">): { name: string; via: string } {
  // One added from the catalog keeps its key as the name ("linear").
  const known = CATALOG.find((c) => c.id === s.name.toLowerCase() && (!s.origin || originOf(c.url) === s.origin));
  // Held by Switchyard under a name the user typed: shown as typed.
  if (known) return { name: known.name, via: SOURCE[s.source] ?? s.source };
  const ai = /^claude\.ai (.+)$/.exec(s.name);
  if (ai) return { name: ai[1], via: "claude.ai" };
  const plugin = /^plugin:([^:]+):(.+)$/.exec(s.name);
  if (plugin) return { name: plugin[2], via: `${plugin[1]} plugin` };
  return { name: s.name, via: SOURCE[s.source] ?? s.source };
}

export const STATUS_LABEL: Record<string, string> = {
  connected: "Connected",
  "needs-auth": "Needs sign-in",
  failed: "Not working",
  pending: "Connecting",
  disabled: "Off",
};

const RANK: Record<string, number> = { "needs-auth": 0, failed: 1, pending: 2, connected: 3, disabled: 4 };

/** Needs sign-in, then broken, then the rest; by name within each. */
export function sortServers(list: McpServer[]): McpServer[] {
  return [...list].sort(
    (a, b) => (RANK[a.status] ?? 2) - (RANK[b.status] ?? 2) || displayName(a).name.localeCompare(displayName(b).name, undefined, { sensitivity: "base" }),
  );
}

/** What the user can do about a server: sign in, retry, or nothing. */
export function actionFor(s: McpServer): "connect" | "open" | "retry" | null {
  if (s.status === "needs-auth") return s.app ? (s.installUrl ? "open" : null) : "connect";
  if (s.status === "failed") return "retry";
  return null;
}

/** The error a server reports, without the transport type noise Codex adds. */
export function shortError(e: string | undefined): string {
  if (!e) return "";
  const cleaned = e.replace(/Transport \[[^\]]*\]/g, "Transport").replace(/\s+/g, " ").trim();
  return cleaned.length > 220 ? `${cleaned.slice(0, 219)}…` : cleaned;
}

/** A service the page knows by face: its name, one line on what it gives a
 *  session, and its mark (connectorBrands.ts). With a `url` it is a remote MCP
 *  server that signs in with OAuth, offered under Available (each answered 401
 *  with OAuth resource metadata on 2026-11-16). GitHub's own endpoint wants a
 *  token, not OAuth, so it has no `url` (`gh` covers PRs); a self-hosted one
 *  (Metabase) goes in with its own address. */
export interface KnownService {
  id: string;
  name: string;
  about: string;
  /** A key of BRAND_MARKS; none draws a letter tile. */
  brand?: string;
  url?: string;
  note?: string;
  /** Other names it is listed under, compared like `id`. */
  aliases?: string[];
}
export const KNOWN: KnownService[] = [
  { id: "linear", name: "Linear", about: "Issues, projects and cycles", brand: "linear", url: "https://mcp.linear.app/mcp" },
  { id: "sentry", name: "Sentry", about: "Errors, releases and performance", brand: "sentry", url: "https://mcp.sentry.dev/mcp" },
  { id: "datadog", name: "Datadog", about: "Logs, metrics, monitors and traces", brand: "datadog", url: "https://mcp.datadoghq.com/api/unstable/mcp-server/mcp", note: "US1 site" },
  { id: "posthog", name: "PostHog", about: "Product analytics and flags", brand: "posthog", url: "https://mcp.posthog.com/mcp" },
  { id: "notion", name: "Notion", about: "Pages, databases and comments", brand: "notion", url: "https://mcp.notion.com/mcp" },
  { id: "atlassian", name: "Atlassian", about: "Jira and Confluence", brand: "atlassian", url: "https://mcp.atlassian.com/v1/mcp", aliases: ["jira", "confluence"] },
  { id: "vercel", name: "Vercel", about: "Deployments and logs", brand: "vercel", url: "https://mcp.vercel.com" },
  { id: "github", name: "GitHub", about: "Repositories, issues and pull requests", brand: "github" },
  { id: "figma", name: "Figma", about: "Files, frames and design tokens", brand: "figma" },
  { id: "gmail", name: "Gmail", about: "Read and draft email", brand: "gmail" },
  { id: "googledrive", name: "Google Drive", about: "Search and read your files", brand: "googledrive", aliases: ["drive"] },
  { id: "metabase", name: "Metabase", about: "Questions and dashboards", brand: "metabase" },
  { id: "stripe", name: "Stripe", about: "Payments, customers and invoices", brand: "stripe" },
  { id: "supabase", name: "Supabase", about: "Databases, auth and storage", brand: "supabase" },
  { id: "cloudflare", name: "Cloudflare", about: "Workers, DNS and zones", brand: "cloudflare" },
  { id: "slack", name: "Slack", about: "Channels and messages", brand: "slack" },
];
/** The ones "Connect" can add: a remote server with a known address. */
export const CATALOG = KNOWN.filter((k): k is KnownService & { url: string } => !!k.url);

const originOf = (url: string) => {
  try {
    return new URL(url).origin;
  } catch {
    return "";
  }
};

/** The same rules yardd applies: a config key and a plain https address. */
export function newServerProblem(name: string, url: string): string | null {
  if (!/^[A-Za-z0-9][A-Za-z0-9_-]{0,39}$/.test(name)) return "Name it with letters, digits, - or _ (up to 40).";
  if (!/^https:\/\/[A-Za-z0-9.:-]+([/?#]\S*)?$/.test(url) || url.length > 300) return "The address must be a full https:// URL.";
  return null;
}

/** Servers in the account's own config, which "Remove" can take out. */
export const removable = (s: McpServer) => !s.app && (s.source === "user" || s.source === "config" || s.source === "switchyard");


/** What an agent or automation without a list of its own leaves out (yardd's
 *  scope.rs `DENIED_BY_DEFAULT`), matched ignoring case. */
export const DENIED_BY_DEFAULT = ["metabase", "claude.ai Metabase"];

/** The servers an agent or automation can be given: the account's MCP
 *  servers (not ChatGPT apps, which are not scoped), plus any it already has
 *  that the account no longer lists, so a save does not drop them unseen. */
export function pickableConnectors(servers: McpServer[] | undefined, picked: string[] | null): string[] {
  const names = (servers ?? []).filter((s) => !s.app).map((s) => s.name);
  for (const n of picked ?? []) if (!names.includes(n)) names.push(n);
  return names;
}

/** Whether a server is on: `picked` null is the default (all but the deny list). */
export function connectorOn(picked: string[] | null, name: string): boolean {
  if (picked) return picked.includes(name);
  return !DENIED_BY_DEFAULT.some((d) => d.toLowerCase() === name.toLowerCase());
}

/** Turn one server on or off. The default becomes an explicit list of what it covered. */
export function toggleConnector(picked: string[] | null, all: string[], name: string): string[] {
  const now = picked ?? all.filter((n) => connectorOn(null, n));
  return now.includes(name) ? now.filter((n) => n !== name) : [...now, name];
}

/* ---------- services: one per connector, across accounts ---------- */

/** An engine account on the page, or Switchyard's own set for the engines
 *  that cannot sign a server in themselves (`engine` "switchyard"). */
export interface ConnectorAccount {
  engine: ConnectorEngine;
  account: string;
  /** "Default", "Work"; for Switchyard's set, the engines it serves. */
  name: string;
  /** "Claude", "Codex", "Switchyard". */
  engineName: string;
  /** Logged out: its servers cannot be listed. */
  signedOut?: boolean;
}
export const accountKey = (a: Pick<ConnectorAccount, "engine" | "account">) => `${a.engine}\n${a.account}`;

export interface ServiceEntry {
  account: ConnectorAccount;
  server: McpServer;
}
export interface Service {
  /** A KNOWN id, or the listed name squeezed to letters and digits. */
  id: string;
  name: string;
  about: string;
  brand?: string;
  known?: KnownService;
  entries: ServiceEntry[];
}

const squeeze = (name: string) => name.toLowerCase().replace(/[^a-z0-9]+/g, "");
/** The id of a service the page does not know by face, from its listed name. */
export const serviceId = (name: string) => squeeze(name) || name;

/** The known service a listed server is: by address first, then by name
 *  ("claude.ai Linear", "plugin:linear:linear" and "linear" are all Linear). */
export function knownFor(s: Pick<McpServer, "name" | "source" | "origin">): KnownService | undefined {
  if (s.origin) {
    const byOrigin = CATALOG.find((k) => originOf(k.url) === s.origin);
    if (byOrigin) return byOrigin;
  }
  const n = squeeze(displayName(s).name);
  return KNOWN.find((k) => [k.id, ...(k.aliases ?? [])].some((id) => n === id)) ?? KNOWN.find((k) => n.includes(k.id));
}

/** Every account's servers as services, by name. */
export function groupServices(lists: { account: ConnectorAccount; servers: McpServer[] }[]): Service[] {
  const byId = new Map<string, Service>();
  for (const { account, servers } of lists) {
    for (const server of servers) {
      const known = knownFor(server);
      const shown = displayName(server);
      const id = known?.id ?? serviceId(shown.name);
      const service = byId.get(id) ?? { id, name: known?.name ?? shown.name, about: known?.about ?? "", brand: known?.brand, known, entries: [] };
      service.entries.push({ account, server });
      byId.set(id, service);
    }
  }
  return [...byId.values()].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
}

const BEST: Record<string, number> = { connected: 0, pending: 1, "needs-auth": 2, failed: 3, disabled: 4 };

/** An account's entry for a service that says how it stands there: a working
 *  one wins over a second listing that is signed out or broken (claude.ai's
 *  Datadog next to a plugin's). */
export function entryFor(service: Service, account: ConnectorAccount): ServiceEntry | undefined {
  const key = accountKey(account);
  return service.entries.filter((e) => accountKey(e.account) === key).sort((a, b) => (BEST[a.server.status] ?? 2) - (BEST[b.server.status] ?? 2))[0];
}

/** The accounts a service is listed for, each once. */
export function serviceAccounts(service: Service): ConnectorAccount[] {
  const seen = new Map<string, ConnectorAccount>();
  for (const e of service.entries) if (!seen.has(accountKey(e.account))) seen.set(accountKey(e.account), e.account);
  return [...seen.values()];
}

export type ServiceSection = "needs" | "connected" | "available";

/** Where a service's card goes: broken somewhere needs the user; working
 *  anywhere is connected; listed but never signed in is merely available. */
export function sectionOf(service: Service): ServiceSection {
  const states = serviceAccounts(service).map((a) => entryFor(service, a)?.server.status);
  if (states.includes("failed")) return "needs";
  if (states.some((s) => s === "connected" || s === "pending")) return "connected";
  return "available";
}

/** "All 3 accounts", "1 of 3 accounts", or the one account's name. */
export function whereText(service: Service, accounts: ConnectorAccount[]): string {
  const on = accounts.filter((a) => entryFor(service, a)?.server.status === "connected");
  if (accounts.length <= 1) return on.length ? "" : "Not connected";
  if (!on.length) return "Not connected";
  if (on.length === accounts.length) return `All ${accounts.length} accounts`;
  return on.length === 1 ? accountText(on[0]) : `${on.length} of ${accounts.length} accounts`;
}

/** "Claude · Work"; an engine's only account goes by the engine alone. */
export const accountText = (a: ConnectorAccount) => (a.account === "default" || a.engine === "switchyard" ? a.engineName : `${a.engineName} · ${a.name}`);

/** One step of connecting a service on one account. */
export type ConnectTarget =
  /** Listed there and signed out: run its sign-in. */
  | { kind: "auth"; account: ConnectorAccount; server: string }
  /** Not there: add the known address to the account's config, then sign in. */
  | { kind: "add"; account: ConnectorAccount; server: string; url: string }
  /** A ChatGPT app: connected on chatgpt.com. */
  | { kind: "open"; account: ConnectorAccount; server: string; url: string }
  | { kind: "retry"; account: ConnectorAccount; server: string };

/** Engines whose own config "Connect" can add a remote server to. */
const ADDS: ConnectorEngine[] = ["claude", "codex", "switchyard"];

/** What connecting a service on an account takes, or null when there is
 *  nothing to do there (connected, logged out, or no way to add it). */
export function targetFor(service: Service, account: ConnectorAccount): ConnectTarget | null {
  if (account.signedOut) return null;
  const entry = entryFor(service, account);
  if (!entry) return service.known?.url && ADDS.includes(account.engine) ? { kind: "add", account, server: service.known.id, url: service.known.url } : null;
  const action = actionFor(entry.server);
  if (action === "connect") return { kind: "auth", account, server: entry.server.name };
  if (action === "open") return { kind: "open", account, server: entry.server.name, url: entry.server.installUrl! };
  if (action === "retry") return { kind: "retry", account, server: entry.server.name };
  return null;
}

/** Known services with an address that no shown account lists yet. */
export function availableToAdd(services: Service[]): Service[] {
  const have = new Set(services.map((s) => s.id));
  return CATALOG.filter((k) => !have.has(k.id)).map((k) => ({ id: k.id, name: k.name, about: k.about, brand: k.brand, known: k, entries: [] }));
}

/** Services whose name, description or source has every word typed. */
export function searchServices(services: Service[], query: string): Service[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return services;
  return services.filter((s) => {
    const text = [s.name, s.about, ...s.entries.map((e) => `${e.server.name} ${displayName(e.server).via}`)].join(" ").toLowerCase();
    return words.every((w) => text.includes(w));
  });
}
