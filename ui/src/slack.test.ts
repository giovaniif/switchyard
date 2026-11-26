import { describe, expect, test } from "bun:test";
import type { AgentEvent, SlackAgentApp, SlackOrigin, SlackView } from "./protocol";
import { applyEvent } from "./protocol";
import { allowedSummary, avatarIconSvg, createAppUrl, iconKey, manifestText, setupSteps, SLACK_MANIFEST, slackStatus, tokenProblem, tokensNeeded } from "./slack";
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
    // A watched channel's notice: where it came from and how many.
    expect(originLabel({ ...origin, channelName: "#alerts", userName: "Datadog", watched: true, count: 3 })?.text).toBe("Watched on Slack · #alerts · 3 messages");
    expect(originLabel({ ...origin, channelName: "#alerts", userName: "Datadog", watched: true, count: 1 })?.text).toBe("Watched on Slack · #alerts · from Datadog");
    const [mine] = applyEvent([], { type: "user_message", text: "hi", origin: { kind: "user" } }, 0);
    expect(mine.kind === "user" && mine.origin).toBeUndefined();
  });
});

describe("an agent's own Slack app", () => {
  const app = (extra: Partial<SlackAgentApp> = {}): SlackAgentApp => ({ step: "install", appId: "A1", teamId: "T1", teamName: "Acme", handle: "reviewer", ownerName: "", canRedirect: true, ...extra });

  test("takes only the configuration token with a public address and an https page", () => {
    expect(tokensNeeded(view({ publicUrl: "https://slack.example.com", canRedirect: true }))).toBe(1);
    expect(tokensNeeded(view({ publicUrl: "", canRedirect: true }))).toBe(2);
    expect(tokensNeeded(view({ canRedirect: false }))).toBe(3);
    // An older yardd says neither.
    expect(tokensNeeded(view())).toBe(3);
  });

  test("numbers only the steps it still takes", () => {
    expect(setupSteps(view({ publicUrl: "https://slack.example.com" }), undefined)).toEqual(["Token", "Approve"]);
    expect(setupSteps(view(), undefined)).toEqual(["Token", "Approve", "Connect"]);
    // A made app keeps the way it was made, whatever the address is now.
    expect(setupSteps(view(), app({ eventsUrl: "https://slack.example.com/slack/events" }))).toEqual(["Token", "Approve"]);
    expect(setupSteps(view({ publicUrl: "https://slack.example.com" }), app({ eventsUrl: "" }))).toEqual(["Token", "Approve", "Connect"]);
  });

  test("wears the agent's face, drawn from the page's avatar, on its colour", () => {
    const face = '<svg viewBox="0 0 24 24" x="0" y="0" width="24" height="24" stroke="white"><path d="M8 10h.01"/></svg>';
    const svg = avatarIconSvg(face, { background: "teal", light: "white", clear: "transparent" }) ?? "";
    expect(svg.startsWith('<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 24 24">')).toBe(true);
    expect(svg).toContain('<rect width="24" height="24" fill="teal"/>');
    expect(svg).toContain('<stop offset="0" stop-color="white"/><stop offset="0.7" stop-color="transparent"/>');
    expect(svg).toContain(face);
    // A colour that is not one cannot break out of its attribute: no icon.
    expect(avatarIconSvg(face, { background: '"/><script>', light: "", clear: "" })).toBeNull();
    expect(avatarIconSvg(face, { background: "teal", light: '"><x', clear: "" })).toContain('stop-color="transparent"');
  });

  test("is sent again only when its picture changed", () => {
    const a = avatarIconSvg("<svg/>", { background: "teal", light: "", clear: "" }) ?? "";
    const b = avatarIconSvg("<svg/>", { background: "navy", light: "", clear: "" }) ?? "";
    expect(iconKey(a)).toBe(iconKey(a));
    expect(iconKey(a)).not.toBe(iconKey(b));
    expect(iconKey(a)).toMatch(/^v1-[0-9a-f]{16}$/);
  });
});
