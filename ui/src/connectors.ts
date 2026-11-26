// Words and order for the Connectors page: which MCP servers each engine
// account has, the ones that need the user first.
import type { McpServer } from "./protocol";

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
};

/** "claude.ai Datadog" -> Datadog via claude.ai; "plugin:engineering:datadog"
 *  -> datadog via the engineering plugin. */
export function displayName(s: Pick<McpServer, "name" | "source" | "origin">): { name: string; via: string } {
  // One added from the catalog keeps its key as the name ("linear").
  const known = CATALOG.find((c) => c.id === s.name.toLowerCase() && (!s.origin || originOf(c.url) === s.origin));
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

/** "12 connected · 2 need sign-in · 1 not working". */
export function summary(list: McpServer[]): string {
  const n = (status: string) => list.filter((s) => s.status === status).length;
  const parts = [`${n("connected")} connected`];
  if (n("needs-auth")) parts.push(`${n("needs-auth")} need${n("needs-auth") === 1 ? "s" : ""} sign-in`);
  if (n("failed")) parts.push(`${n("failed")} not working`);
  if (n("pending")) parts.push(`${n("pending")} connecting`);
  return parts.join(" · ");
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

/** Remote MCP servers that sign in with OAuth, offered by "Add connector".
 *  Each answered 401 with OAuth resource metadata on 2026-11-16. GitHub's
 *  wants a token, not OAuth, so it is not here (`gh` covers PRs); a
 *  self-hosted one (Metabase) goes in with its own address. */
export const CATALOG: { id: string; name: string; url: string; note?: string }[] = [
  { id: "linear", name: "Linear", url: "https://mcp.linear.app/mcp" },
  { id: "sentry", name: "Sentry", url: "https://mcp.sentry.dev/mcp" },
  { id: "datadog", name: "Datadog", url: "https://mcp.datadoghq.com/api/unstable/mcp-server/mcp", note: "US1 site" },
  { id: "posthog", name: "PostHog", url: "https://mcp.posthog.com/mcp" },
  { id: "notion", name: "Notion", url: "https://mcp.notion.com/mcp" },
  { id: "atlassian", name: "Atlassian", url: "https://mcp.atlassian.com/v1/mcp" },
  { id: "vercel", name: "Vercel", url: "https://mcp.vercel.com" },
];

const originOf = (url: string) => {
  try {
    return new URL(url).origin;
  } catch {
    return "";
  }
};

/** Catalog entries the account does not have yet, by address or by name
 *  ("claude.ai Datadog" and "plugin:engineering:datadog" count as Datadog). */
export function missingFromCatalog(servers: McpServer[]) {
  const origins = new Set(servers.map((s) => s.origin).filter(Boolean));
  const names = servers.map((s) => displayName(s).name.toLowerCase());
  return CATALOG.filter((c) => !origins.has(originOf(c.url)) && !names.some((n) => n === c.id || n.includes(c.id)));
}

/** The same rules yardd applies: a config key and a plain https address. */
export function newServerProblem(name: string, url: string): string | null {
  if (!/^[A-Za-z0-9][A-Za-z0-9_-]{0,39}$/.test(name)) return "Name it with letters, digits, - or _ (up to 40).";
  if (!/^https:\/\/[A-Za-z0-9.:-]+([/?#]\S*)?$/.test(url) || url.length > 300) return "The address must be a full https:// URL.";
  return null;
}

/** Servers in the account's own config, which "Remove" can take out. */
export const removable = (s: McpServer) => !s.app && (s.source === "user" || s.source === "config");


export type ConnectorFilter = "connected" | "available" | "failed" | "all";
export const CONNECTOR_PAGE_SIZE = 8;

/** Unused catalog entries are only shown when the user asks for them. */
export function filterServers(servers: McpServer[], query: string, filter: ConnectorFilter): McpServer[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  return sortServers(servers).filter((server) => {
    const matches = filter === "all" || (filter === "connected" ? ["connected", "pending"].includes(server.status)
      : filter === "available" ? ["needs-auth", "disabled"].includes(server.status) : server.status === "failed");
    const { name, via } = displayName(server);
    const text = `${name} ${via}`.toLowerCase();
    return matches && words.every((word) => text.includes(word));
  });
}

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
