import { expect, test } from "bun:test";
import {
  accountText, actionFor, availableToAdd, connectorOn, CATALOG, displayName, entryFor, groupServices, knownFor, newServerProblem, pickableConnectors, removable, searchServices,
  sectionOf, shortError, sortServers, targetFor, toggleConnector, whereText, type ConnectorAccount,
} from "./connectors";
import type { McpServer } from "./protocol";

const s = (name: string, status: string, extra: Partial<McpServer> = {}): McpServer => ({ name, status, source: "user", tools: 0, appTools: 0, ...extra });

test("names read as the service, with where it comes from", () => {
  expect(displayName(s("claude.ai Datadog", "connected", { source: "claudeai" }))).toEqual({ name: "Datadog", via: "claude.ai" });
  expect(displayName(s("plugin:engineering:sentry", "connected", { source: "plugin" }))).toEqual({ name: "sentry", via: "engineering plugin" });
  expect(displayName(s("posthog", "connected", { source: "config" }))).toEqual({ name: "PostHog", via: "config.toml" });
  expect(displayName(s("metabase", "connected", { source: "config" }))).toEqual({ name: "metabase", via: "config.toml" });
  expect(displayName(s("x", "connected", { source: "something-new" }))).toEqual({ name: "x", via: "something-new" });
  expect(displayName(s("linear", "connected", { source: "user", origin: "https://mcp.linear.app" }))).toEqual({ name: "Linear", via: "Your config" });
  expect(displayName(s("linear", "connected", { source: "user", origin: "https://evil.example" }))).toEqual({ name: "linear", via: "Your config" });
});

test("what needs the user comes first", () => {
  const list = [s("b", "connected"), s("metabase", "failed"), s("a", "connected"), s("Datadog", "needs-auth"), s("old", "disabled")];
  expect(sortServers(list).map((x) => x.name)).toEqual(["Datadog", "metabase", "a", "b", "old"]);
});

test("each state offers what can fix it", () => {
  expect(actionFor(s("x", "needs-auth"))).toBe("connect");
  expect(actionFor(s("GitHub", "needs-auth", { app: true, installUrl: "https://chatgpt.com/apps/github" }))).toBe("open");
  expect(actionFor(s("GitHub", "needs-auth", { app: true }))).toBeNull();
  expect(actionFor(s("x", "failed"))).toBe("retry");
  expect(actionFor(s("x", "connected"))).toBeNull();
});

test("Codex's transport noise is cut from errors", () => {
  expect(shortError("MCP startup failed: Send message error Transport [a::b<c::d>] error: Auth required")).toBe("MCP startup failed: Send message error Transport error: Auth required");
  expect(shortError(undefined)).toBe("");
});

test("an added server is a plain name and https address", () => {
  expect(newServerProblem("metabase", "https://metabase.example.com/api/mcp")).toBeNull();
  expect(newServerProblem("-x", "https://a.example")).toContain("Name");
  expect(newServerProblem("ok", "http://a.example")).toContain("https");
  expect(newServerProblem("ok", "https://a.example/a b")).toContain("https");
  expect(removable(s("x", "connected", { source: "user" }))).toBe(true);
  expect(removable(s("x", "connected", { source: "plugin" }))).toBe(false);
  expect(removable(s("x", "connected", { source: "switchyard" }))).toBe(true);
});


const claude: ConnectorAccount = { engine: "claude", account: "default", name: "Default", engineName: "Claude" };
const work: ConnectorAccount = { engine: "claude", account: "a1", name: "Work", engineName: "Claude" };
const codex: ConnectorAccount = { engine: "codex", account: "default", name: "Default", engineName: "Codex" };
const held: ConnectorAccount = { engine: "switchyard", account: "default", name: "OpenCode, Grok", engineName: "OpenCode · Grok" };

test("one service per connector, however many accounts and sources list it", () => {
  const services = groupServices([
    { account: claude, servers: [s("claude.ai Linear", "connected", { source: "claudeai" }), s("plugin:linear:linear", "needs-auth", { source: "plugin", origin: "https://mcp.linear.app" }), s("chart", "connected")] },
    { account: codex, servers: [s("linear", "needs-auth", { source: "config", origin: "https://mcp.linear.app" }), s("chart", "connected", { source: "config" }), s("GitHub", "needs-auth", { source: "chatgpt", app: true, installUrl: "https://chatgpt.com/apps/github" })] },
    { account: held, servers: [s("my-linear", "connected", { source: "switchyard", origin: "https://mcp.linear.app" })] },
  ]);
  expect(services.map((x) => [x.id, x.name, x.entries.length])).toEqual([["chart", "chart", 2], ["github", "GitHub", 1], ["linear", "Linear", 4]]);
  const linear = services[2];
  expect(linear.brand).toBe("linear");
  expect(linear.about).toBe("Issues, projects and cycles");
  // A working listing wins over the same account's signed-out one.
  expect(entryFor(linear, claude)?.server.name).toBe("claude.ai Linear");
  expect(whereText(linear, [claude, codex, held])).toBe("2 of 3 accounts");
  expect(whereText(services[0], [claude, codex])).toBe("All 2 accounts");
  expect(whereText(linear, [claude, work, codex])).toBe("Claude");
  expect(whereText(services[1], [claude, codex])).toBe("Not connected");
  expect(accountText(work)).toBe("Claude · Work");
  expect(accountText(held)).toBe("OpenCode · Grok");
});

test("a service is known by its address first, then by its name", () => {
  expect(knownFor(s("issues", "connected", { origin: "https://mcp.linear.app" }))?.id).toBe("linear");
  expect(knownFor(s("claude.ai Google Drive", "connected", { source: "claudeai" }))?.id).toBe("googledrive");
  expect(knownFor(s("plugin:engineering:atlassian", "connected", { source: "plugin" }))?.id).toBe("atlassian");
  expect(knownFor(s("jira", "connected"))?.id).toBe("atlassian");
  expect(knownFor(s("chart", "connected"))).toBeUndefined();
});

test("cards go where they need the user: broken, then connected, then merely available", () => {
  const [bad, off, on, partly] = groupServices([
    { account: claude, servers: [s("a-broken", "failed", { error: "No URL" }), s("b-unused", "needs-auth"), s("c-used", "connected"), s("d-partly", "connected")] },
    { account: codex, servers: [s("b-unused", "disabled"), s("d-partly", "needs-auth")] },
  ]);
  expect([bad, off, on, partly].map(sectionOf)).toEqual(["needs", "available", "connected", "connected"]);
});

test("connecting a service on an account: sign in, add first, open ChatGPT, or nothing", () => {
  const services = groupServices([
    { account: claude, servers: [s("plugin:linear:linear", "needs-auth", { source: "plugin" }), s("gone", "failed")] },
    { account: codex, servers: [s("GitHub", "needs-auth", { source: "chatgpt", app: true, installUrl: "https://chatgpt.com/apps/github" }), s("mine", "needs-auth", { source: "config" })] },
  ]);
  const by = (id: string) => services.find((x) => x.id === id)!;
  expect(targetFor(by("linear"), claude)).toEqual({ kind: "auth", account: claude, server: "plugin:linear:linear" });
  // Not on Codex yet, and its address is known: added to Codex's config under the catalog's name.
  expect(targetFor(by("linear"), codex)).toEqual({ kind: "add", account: codex, server: "linear", url: "https://mcp.linear.app/mcp" });
  expect(targetFor(by("linear"), held)).toMatchObject({ kind: "add", server: "linear" });
  expect(targetFor(by("linear"), { ...work, signedOut: true })).toBeNull();
  expect(targetFor(by("github"), codex)).toMatchObject({ kind: "open", url: "https://chatgpt.com/apps/github" });
  // GitHub has no address of its own, and a typed server has none known: nothing to add elsewhere.
  expect(targetFor(by("github"), claude)).toBeNull();
  expect(targetFor(by("mine"), claude)).toBeNull();
  expect(targetFor(by("gone"), claude)).toMatchObject({ kind: "retry" });
});

test("Available offers the known servers no shown account lists, Linear first", () => {
  expect(CATALOG[0].id).toBe("linear");
  const have = groupServices([{ account: claude, servers: [s("claude.ai Datadog", "connected", { source: "claudeai", origin: "https://mcp-proxy.anthropic.com" }), s("plugin:linear:linear", "connected", { source: "plugin", origin: "https://mcp.linear.app" }), s("my-sentry", "needs-auth", { origin: "https://mcp.sentry.dev" })] }]);
  expect(availableToAdd(have).map((x) => x.id)).toEqual(["posthog", "notion", "atlassian", "vercel"]);
  expect(availableToAdd([]).length).toBe(CATALOG.length);
});

test("search matches the service's name, what it is for and where it comes from", () => {
  const services = groupServices([{ account: claude, servers: [s("plugin:engineering:sentry", "needs-auth", { source: "plugin" }), s("claude.ai Datadog", "connected", { source: "claudeai" })] }]);
  expect(searchServices(services, "  SENTRY  engineering ").map((x) => x.id)).toEqual(["sentry"]);
  expect(searchServices(services, "claude.ai").map((x) => x.id)).toEqual(["datadog"]);
  expect(searchServices(services, "traces").map((x) => x.id)).toEqual(["datadog"]);
  expect(searchServices(services, "linear")).toEqual([]);
  expect(searchServices(services, " ")).toHaveLength(2);
});

test("an agent's connectors: the default leaves Metabase out, a pick makes the list explicit", () => {
  const servers = [s("metabase", "connected"), s("handoff", "connected"), s("claude.ai Metabase", "connected", { source: "claudeai" }), s("GitHub", "needs-auth", { app: true })];
  const all = pickableConnectors(servers, null);
  expect(all).toEqual(["metabase", "handoff", "claude.ai Metabase"]);
  expect(all.filter((n) => connectorOn(null, n))).toEqual(["handoff"]);
  expect(toggleConnector(null, all, "metabase")).toEqual(["handoff", "metabase"]);
  expect(toggleConnector(null, all, "handoff")).toEqual([]);
  expect(toggleConnector(["handoff"], all, "handoff")).toEqual([]);
  // One the account no longer lists stays visible, so a save keeps it knowingly.
  expect(pickableConnectors(servers, ["gone"])).toContain("gone");
  expect(connectorOn([], "handoff")).toBe(false);
});
