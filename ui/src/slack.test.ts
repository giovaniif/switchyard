import { describe, expect, test } from "bun:test";
import type { AgentEvent, SlackOrigin, SlackView } from "./protocol";
import { applyEvent } from "./protocol";
import { allowedSummary, createAppUrl, manifestText, SLACK_MANIFEST, slackStatus, tokenProblem } from "./slack";
import { originLabel } from "./agents";
import { initial, reduce } from "./store";

const view = (extra: Partial<SlackView> = {}): SlackView => ({
  configured: false, appToken: "", botToken: "", state: "off", team: "", bot: "", error: null,
  allowed: [], requests: [], defaultAgent: null, channels: [], ...extra,
});

const origin: SlackOrigin = { kind: "slack", team: "T1", channel: "C1", channelName: "#general", threadTs: "1.0", ts: "1.0", user: "U1", userName: "Gio" };

describe("the Slack app manifest", () => {
  test("turns Socket Mode on and asks for what yardd uses", () => {
    expect(SLACK_MANIFEST.settings.socket_mode_enabled).toBe(true);
    for (const scope of ["app_mentions:read", "chat:write", "im:history", "im:read", "im:write", "channels:history", "groups:history", "users:read", "reactions:write"])
      expect(SLACK_MANIFEST.oauth_config.scopes.bot).toContain(scope);
    expect(SLACK_MANIFEST.settings.event_subscriptions.bot_events).toEqual(["app_mention", "message.im", "message.channels", "message.groups"]);
    expect(JSON.parse(manifestText())).toEqual(SLACK_MANIFEST);
    const url = new URL(createAppUrl());
    expect(url.origin).toBe("https://api.slack.com");
    expect(JSON.parse(url.searchParams.get("manifest_json")!)).toEqual(SLACK_MANIFEST);
  });
});

describe("pasted tokens", () => {
  test("are checked for their kind before they are sent", () => {
    expect(tokenProblem("app", "xapp-1-A0-123-abcdef")).toBeNull();
    expect(tokenProblem("bot", "xoxb-123-456-abcdef")).toBeNull();
    expect(tokenProblem("app", "")).toBeNull();
    expect(tokenProblem("app", "xoxb-123-456-abcdef")).toContain("bot token");
    expect(tokenProblem("bot", "xapp-1-A0-123-abcdef")).toContain("app-level token");
    expect(tokenProblem("bot", "xoxp-123-456-abcdef")).toContain("user token");
    expect(tokenProblem("bot", "xoxb-1 2")).toContain("no spaces");
    expect(tokenProblem("bot", "xoxb-1")).toContain("too short");
  });
});

describe("the connection's status", () => {
  test("names the bot and workspace, or the error", () => {
    expect(slackStatus(view())).toEqual({ label: "Not set up", tone: "muted", line: "Talk to your agents from Slack" });
    expect(slackStatus(view({ state: "connected", bot: "switchyard", team: "Acme" }))).toEqual({ label: "Connected", tone: "ok", line: "As @switchyard in Acme" });
    expect(slackStatus(view({ state: "error", error: "The bot token was refused (auth.test: invalid_auth)." })).line).toContain("invalid_auth");
    expect(slackStatus(view({ state: "connecting" })).label).toBe("Connecting…");
  });

  test("says who may talk to agents", () => {
    expect(allowedSummary(view())).toContain("direct message");
    expect(allowedSummary(view({ allowed: [{ id: "U1", name: "Gio" }] }))).toBe("Only Gio.");
    expect(allowedSummary(view({ allowed: [{ id: "U1", name: "Gio" }, { id: "U2", name: "Ana" }] }))).toBe("2 people.");
  });

  test("comes with the snapshot and every change, from the hub only", () => {
    let s = reduce(initial, { t: "server", msg: { op: "snapshot", sessions: [], slack: view({ state: "connecting" }) } });
    expect(s.slack?.state).toBe("connecting");
    s = reduce(s, { t: "server", msg: { op: "slack", slack: view({ state: "connected", bot: "b" }) } });
    expect(s.slack?.state).toBe("connected");
    expect(reduce(s, { t: "server", msg: { op: "slack", hostId: "h1", slack: view() } }).slack?.state).toBe("connected");
  });
});

describe("a message from Slack in the thread", () => {
  test("keeps its origin and reads as channel and person", () => {
    const ev: AgentEvent = { type: "user_message", text: "look at #12", origin };
    const [item] = applyEvent([], ev, 0);
    expect(item.kind === "user" && item.origin).toEqual(origin);
    expect(originLabel(origin)).toEqual({ text: "From Gio on Slack · #general", agentId: null });
    expect(originLabel({ ...origin, channelName: "Direct message" })?.text).toBe("From Gio on Slack · Direct message");
    expect(originLabel({ ...origin, channelName: "", userName: "" })?.text).toBe("From U1 on Slack · C1");
    expect(originLabel({ kind: "slack" })?.text).toBe("From someone on Slack");
    const [mine] = applyEvent([], { type: "user_message", text: "hi", origin: { kind: "user" } }, 0);
    expect(mine.kind === "user" && mine.origin).toBeUndefined();
  });
});
