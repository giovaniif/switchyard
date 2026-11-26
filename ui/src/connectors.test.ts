import { expect, test } from "bun:test";
import { actionFor, connectorOn, filterServers, CATALOG, displayName, missingFromCatalog, newServerProblem, pickableConnectors, removable, shortError, sortServers, summary, toggleConnector } from "./connectors";
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
  expect(summary(list)).toBe("2 connected · 1 needs sign-in · 1 not working");
  expect(summary([s("a", "needs-auth"), s("b", "needs-auth")])).toBe("0 connected · 2 need sign-in");
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

test("the catalog offers what the account lacks, Linear first", () => {
  expect(CATALOG[0].id).toBe("linear");
  const have = [
    s("claude.ai Datadog", "connected", { source: "claudeai", origin: "https://mcp-proxy.anthropic.com" }),
    s("plugin:linear:linear", "connected", { source: "plugin", origin: "https://mcp.linear.app" }),
    s("my-sentry", "needs-auth", { origin: "https://mcp.sentry.dev" }),
  ];
  expect(missingFromCatalog(have).map((c) => c.id)).toEqual(["posthog", "notion", "atlassian", "vercel"]);
  expect(missingFromCatalog([]).length).toBe(CATALOG.length);
});

test("an added server is a plain name and https address", () => {
  expect(newServerProblem("metabase", "https://metabase.example.com/api/mcp")).toBeNull();
  expect(newServerProblem("-x", "https://a.example")).toContain("Name");
  expect(newServerProblem("ok", "http://a.example")).toContain("https");
  expect(newServerProblem("ok", "https://a.example/a b")).toContain("https");
  expect(removable(s("x", "connected", { source: "user" }))).toBe(true);
  expect(removable(s("x", "connected", { source: "plugin" }))).toBe(false);
});


test("connected view hides unused connectors and setup failures", () => {
  const list = [s("used", "connected"), s("unused", "needs-auth"), s("off", "disabled"), s("bad", "failed"), s("starting", "pending")];
  expect(filterServers(list, "", "connected").map((x) => x.name)).toEqual(["starting", "used"]);
  expect(filterServers(list, "", "available").map((x) => x.name)).toEqual(["unused", "off"]);
  expect(filterServers(list, "", "failed").map((x) => x.name)).toEqual(["bad"]);
  expect(filterServers(list, "", "all")).toHaveLength(5);
});
test("search matches readable service names and source, case insensitively", () => {
  const list = [s("plugin:engineering:sentry", "needs-auth", { source: "plugin" }), s("claude.ai Datadog", "connected", { source: "claudeai" })];
  expect(filterServers(list, "  SENTRY  engineering ", "available").map((x) => x.name)).toEqual(["plugin:engineering:sentry"]);
  expect(filterServers(list, "claude.ai", "connected").map((x) => x.name)).toEqual(["claude.ai Datadog"]);
  expect(filterServers(list, "sentry", "connected")).toEqual([]);
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
