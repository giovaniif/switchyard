import { describe, expect, test } from "bun:test";
import type { AgentEvent, SlackAgentApp, SlackMe, SlackOrigin, SlackView } from "./protocol";
import { applyEvent } from "./protocol";
import { allowedSummary, avatarIconSvg, hostSlack, iconKey, mySteps, setupSteps, slackStatus, tokensNeeded } from "./slack";
import { originLabel } from "./agents";
import { initial, reduce } from "./store";

const view = (extra: Partial<SlackView> = {}): SlackView => ({ me: null, allowed: [], ...extra });
const me = (extra: Partial<SlackMe> = {}): SlackMe => ({
  step: "ready", appId: "A1", teamId: "T1", teamName: "Acme", userName: "Gio", canRedirect: true, userToken: "xoxp-…abcd", appToken: "xapp-…wxyz", ...extra,
});

const origin: SlackOrigin = { kind: "slack", team: "T1", channel: "C1", channelName: "#general", threadTs: "1.0", ts: "1.0", user: "U1", userName: "Gio" };

describe("the connection's status", () => {
  test("is the owner's own Slack: its step, then who he is and where, or the error", () => {
    expect(slackStatus(view())).toEqual({ label: "Not connected", tone: "muted", line: "Your direct messages and the channels you choose" });
    expect(slackStatus(view({ me: me({ step: "install" }) })).label).toBe("Approve in Slack");
    expect(slackStatus(view({ me: me({ step: "appToken" }) })).line).toBe("Approved by Gio in Acme. Paste its connection token.");
    expect(slackStatus(view({ me: me({ state: "connected", handle: "gio" }) }))).toEqual({ label: "Connected", tone: "ok", line: "As @gio in Acme" });
    expect(slackStatus(view({ me: me({ state: "error", error: "Your user token was refused (auth.test: invalid_auth)." }) })).line).toContain("invalid_auth");
    expect(slackStatus(view({ me: me({ state: "connecting" }) })).label).toBe("Connecting…");
  });

  test("numbers his steps", () => {
    expect(mySteps(view()).at).toBe(0);
    expect(mySteps(view({ me: me({ step: "install" }) })).at).toBe(1);
    expect(mySteps(view({ me: me({ step: "appToken" }) })).at).toBe(2);
    expect(mySteps(view({ me: me() })).at).toBe(3);
  });

  test("says whose requests can use tools", () => {
    expect(allowedSummary(view())).toContain("approves an agent's app");
    expect(allowedSummary(view({ allowed: [{ id: "U1", name: "Gio" }] }))).toBe("Only Gio.");
    expect(allowedSummary(view({ allowed: [{ id: "U1", name: "Gio" }, { id: "U2", name: "Ana" }] }))).toBe("2 people.");
  });

  test("comes with the snapshot and every change, from the hub only", () => {
    let s = reduce(initial, { t: "server", msg: { op: "snapshot", sessions: [], slack: view({ me: me({ state: "connecting" }) }) } });
    expect(s.slack?.me?.state).toBe("connecting");
    s = reduce(s, { t: "server", msg: { op: "slack", slack: view({ me: me({ state: "connected" }) }) } });
    expect(s.slack?.me?.state).toBe("connected");
    expect(reduce(s, { t: "server", msg: { op: "slack", hostId: "h1", slack: view() } }).slack?.me?.state).toBe("connected");
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

describe("an agent's own app on its host", () => {
  test("is made with that host's workspaces and addresses", () => {
    const view = { configured: false, workspaces: [{ teamId: "T1", teamName: "Mac's" }], canRedirect: true, publicUrl: "", hosts: { h1: { workspaces: [{ teamId: "T2", teamName: "Server's" }], canRedirect: false, publicUrl: "" } } } as unknown as SlackView;
    expect(hostSlack(view, "local")?.workspaces?.[0].teamId).toBe("T1");
    expect(hostSlack(view, "h1")?.workspaces?.[0].teamId).toBe("T2");
    expect(tokensNeeded(hostSlack(view, "h1"))).toBe(3);
    // A host that has said nothing yet offers no workspace.
    expect(hostSlack(view, "h2")?.workspaces).toEqual([]);
    expect(hostSlack(null, "h1")).toBeNull();
  });
});
