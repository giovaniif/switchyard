import { describe, expect, test } from "bun:test";
import type { SlackAgentApp, SlackConversation, SlackMessage } from "./protocol";
import { applyFormat, applyLink, applyMessage, callableAgents, calledAgents, composerKey, initials, linkUrl, mentionablePeople, mentionedPeople, mentionQuery, mergePage, pageKey, readListPrefs, shownConversations, sidebarConversations, startsGroup, suggestAgents, suggestMentions } from "./slackInbox";
import { initial, parseHash, reduce, routeHash } from "./store";
// The same cases yardd's `mention_handle` is tested on: the box must say a
// message calls an agent exactly when yardd will make it a mention.
import handleCalls from "../../crates/yardd/src/slack/handle-calls.json";

const msg = (ts: string, extra: Partial<SlackMessage> = {}): SlackMessage => ({ ts, user: "U1", text: `m${ts}`, edited: false, deleted: false, replyCount: 0, reactions: [], files: [], ...extra });
const conv = (id: string, extra: Partial<SlackConversation> = {}): SlackConversation => ({ team: "T", id, kind: "im", name: id, peer: "", lastTs: "1", lastRead: "1", unread: 0, ...extra });

describe("the Slack inbox's pages", () => {
  test("merge by ts, oldest first; Load older sets whether more is left", () => {
    let p = mergePage(undefined, [msg("3"), msg("4")], { U1: "Ana" }, true, false);
    expect(p.messages.map((m) => m.ts)).toEqual(["3", "4"]);
    // A page that was only waiting (Loading…) takes its first answer's `more`.
    expect(mergePage({ messages: [], users: {}, faces: {}, more: false, loading: true }, [msg("3")], {}, true, false).more).toBe(true);
    p = mergePage(p, [msg("1"), msg("2")], {}, false, true);
    expect(p.messages.map((m) => m.ts)).toEqual(["1", "2", "3", "4"]);
    expect(p.more).toBe(false);
    // Reopened: the newest page again keeps what was loaded and known.
    p = mergePage(p, [msg("3"), msg("4", { text: "edited" })], {}, true, false);
    expect(p.messages.map((m) => m.text)).toEqual(["m1", "m2", "m3", "edited"]);
    expect(p.more).toBe(false);
    expect(p.users.U1).toBe("Ana");
  });

  test("take a new or changed message where it belongs", () => {
    const pages = {
      [pageKey("T", "D1")]: mergePage(undefined, [msg("1"), msg("2", { replyCount: 1 })], {}, false, false),
      [pageKey("T", "D1", "2")]: mergePage(undefined, [msg("2", { replyCount: 1 }), msg("3", { threadTs: "2" })], {}, false, false),
    };
    let next = applyMessage(pages, "T", "D1", msg("4", { threadTs: "2" }), { U2: "Bo" });
    expect(next[pageKey("T", "D1")].messages.map((m) => m.ts)).toEqual(["1", "2"]);
    expect(next[pageKey("T", "D1", "2")].messages.map((m) => m.ts)).toEqual(["2", "3", "4"]);
    next = applyMessage(next, "T", "D1", msg("2", { replyCount: 2 }), {});
    expect(next[pageKey("T", "D1")].messages[1].replyCount).toBe(2);
    expect(next[pageKey("T", "D1", "2")].messages[0].replyCount).toBe(2);
    // A reply also sent to the conversation shows there too; an unloaded conversation stays unloaded.
    next = applyMessage(next, "T", "D1", msg("5", { threadTs: "2", subtype: "thread_broadcast" }), {});
    expect(next[pageKey("T", "D1")].messages.map((m) => m.ts)).toEqual(["1", "2", "5"]);
    expect(applyMessage(next, "T", "D9", msg("6"), {})).toBe(next);
  });

  test("the sidebar keeps unread and open conversations past its limit", () => {
    const list = [conv("A"), conv("B"), conv("C", { unread: 2 }), conv("D"), conv("E")];
    expect(shownConversations(list, 2, null).map((c) => c.id)).toEqual(["A", "B", "C"]);
    expect(shownConversations(list, 2, "T/E").map((c) => c.id)).toEqual(["A", "B", "C", "E"]);
  });

  test("groups messages by author and time", () => {
    expect(startsGroup(undefined, msg("100"))).toBe(true);
    expect(startsGroup(msg("100"), msg("160"))).toBe(false);
    expect(startsGroup(msg("100"), msg("500"))).toBe(true);
    expect(startsGroup(msg("100"), msg("160", { user: "U2" }))).toBe(true);
    expect(initials("TEST-Ana")).toBe("TA");
    expect(initials("gio")).toBe("GI");
  });
});

describe("calling an agent from the reply box", () => {
  const agents = [{ id: "g1", name: "Reviewer", handle: "reviewer" }, { id: "g2", name: "Deploy Bot", handle: "deploy-bot" }];
  test("suggests while an @handle is typed at the caret", () => {
    expect(mentionQuery("hi @rev", 7)).toBe("rev");
    expect(mentionQuery("@", 1)).toBe("");
    expect(mentionQuery("mail a@rev", 10)).toBeNull();
    expect(mentionQuery("hi @rev there", 13)).toBeNull();
  });
  test("calls each agent whose handle is written as a word", () => {
    expect(calledAgents("@reviewer and @deploy-bot, please", agents)).toEqual(["g1", "g2"]);
    expect(calledAgents("@reviewers a@reviewer @deploy-bot2", agents)).toEqual([]);
    expect(calledAgents("thanks @reviewer.", agents)).toEqual(["g1"]);
  });
  test("offers agents whose app yardd can mention in this workspace, here or on a remote host", () => {
    const app = (extra: Partial<SlackAgentApp>): SlackAgentApp => ({ step: "ready", appId: "A1", teamId: "T0", teamName: "Acme", handle: "x", ownerName: "", canRedirect: false, botUser: "U1", ...extra });
    const listed = [
      { id: "g1", name: "Reviewer" },
      { id: "h1~g2", name: "Far" },
      { id: "h2~g3", name: "Old Host" },
      { id: "g4", name: "Elsewhere" },
      { id: "g5", name: "Halfway" },
      { id: "g6", name: "No App" },
    ];
    const apps = {
      g1: app({ handle: "reviewer", botUser: "UAGENT" }),
      // An agent's app on the server, listed by the hub under its prefixed id.
      "h1~g2": app({ handle: "far", botUser: "UAGENT2" }),
      // An older host says no bot user: yardd could not make it a mention.
      "h2~g3": app({ handle: "old-host", botUser: undefined }),
      g4: app({ handle: "elsewhere", teamId: "T9" }),
      g5: app({ handle: "halfway", step: "appToken" }),
    };
    expect(callableAgents(listed, apps, "T0")).toEqual([
      { id: "g1", name: "Reviewer", handle: "reviewer" },
      { id: "h1~g2", name: "Far", handle: "far" },
    ]);
    expect(callableAgents(listed, undefined, "T0")).toEqual([]);
  });
  test("offers the agent whose handle is typed first, then those it starts, then the rest", () => {
    const list = [{ id: "g3", name: "Pre-reviewer", handle: "pre-reviewer" }, { id: "g4", name: "Reviewer Two", handle: "reviewer-two" }, ...agents];
    expect(suggestAgents(list, "reviewer").map((a) => a.id)).toEqual(["g1", "g4", "g3"]);
    expect(suggestAgents(list, "rev").map((a) => a.id)).toEqual(["g4", "g1", "g3"]);
    expect(suggestAgents(list, "").map((a) => a.id)).toEqual(["g3", "g4", "g1", "g2"]);
    expect(suggestAgents(list, "deploy b")).toEqual([agents[1]]);
  });
  test("keys work the list like Slack's own picker", () => {
    const key = (k: string, extra: { shiftKey?: boolean; composing?: boolean } = {}) => ({ key: k, shiftKey: false, composing: false, ...extra });
    const two = agents;
    // Up and down move the highlight, round the ends.
    expect(composerKey(key("ArrowDown"), two, 0, "")).toEqual({ do: "move", to: 1 });
    expect(composerKey(key("ArrowDown"), two, 1, "")).toEqual({ do: "move", to: 0 });
    expect(composerKey(key("ArrowUp"), two, 0, "")).toEqual({ do: "move", to: 1 });
    // Enter or Tab writes the highlighted one: a bare @ is not sent.
    expect(composerKey(key("Enter"), two, 1, "")).toEqual({ do: "pick", agent: two[1] });
    expect(composerKey(key("Tab"), two, 0, "")).toEqual({ do: "pick", agent: two[0] });
    expect(composerKey(key("Enter"), [two[0]], 0, "rev")).toEqual({ do: "pick", agent: two[0] });
    // Escape closes the list, sending nothing.
    expect(composerKey(key("Escape"), two, 0, "")).toEqual({ do: "close" });
    // The handle typed in full is a complete mention: Enter sends it.
    expect(composerKey(key("Enter"), [two[0]], 0, "reviewer")).toEqual({ do: "send" });
    // No list: Enter sends, the other keys are the box's own.
    expect(composerKey(key("Enter"), [], 0, null)).toEqual({ do: "send" });
    for (const k of ["ArrowDown", "ArrowUp", "Tab", "Escape"]) expect(composerKey(key(k), [], 0, null)).toBeNull();
    // Shift+Enter is a new line and an IME's Enter its own, list or not.
    expect(composerKey(key("Enter", { shiftKey: true }), two, 0, "")).toBeNull();
    expect(composerKey(key("Enter", { shiftKey: true }), [], 0, null)).toBeNull();
    expect(composerKey(key("Enter", { composing: true }), two, 0, "")).toBeNull();
    expect(composerKey(key("ArrowDown", { composing: true }), two, 0, "")).toBeNull();
    expect(composerKey(key("Tab", { shiftKey: true }), two, 0, "")).toBeNull();
  });
  test("agrees with yardd on every case", () => {
    for (const c of handleCalls) {
      expect([c.text, calledAgents(c.text, [{ id: "g", name: "x", handle: c.handle }]).length > 0]).toEqual([c.text, c.calls]);
    }
  });
});

describe("the Slack inbox in the store", () => {
  test("a conversation has its own address", () => {
    const route = { page: "slack" as const, team: "T0", channel: "D0ANA" };
    expect(routeHash(route)).toBe("#/slack/T0/D0ANA");
    expect(parseHash("#/slack/T0/D0ANA")).toEqual(route);
  });

  test("comes with the snapshot and every change, from the hub only", () => {
    let s = reduce(initial, { t: "server", msg: { op: "snapshot", sessions: [], slackInbox: [conv("A")] } });
    expect(s.slackInbox.map((c) => c.id)).toEqual(["A"]);
    s = reduce(s, { t: "server", msg: { op: "slack.inbox", conversations: [conv("B"), conv("A")] } });
    expect(s.slackInbox.map((c) => c.id)).toEqual(["B", "A"]);
    expect(reduce(s, { t: "server", msg: { op: "slack.inbox", hostId: "h1", conversations: [] } }).slackInbox).toHaveLength(2);
    s = reduce(s, { t: "server", msg: { op: "slack.inbox.messages", team: "T", channel: "A", thread: null, before: null, messages: [msg("1")], users: {}, more: true } });
    s = reduce(s, { t: "server", msg: { op: "slack.inbox.message", team: "T", channel: "A", message: msg("2"), users: {} } });
    expect(s.slackPages["T/A"].messages.map((m) => m.ts)).toEqual(["1", "2"]);
  });
});

describe("the sidebar's Slack list as he sets it", () => {
  const list = [conv("D1", { name: "Marc" }), conv("C1", { name: "#eng", kind: "channel", unread: 2 }), conv("D2", { name: "Ana", favorite: true }), conv("D3", { name: "Bo" })];
  const ids = (prefs: Parameters<typeof sidebarConversations>[1], limit = 8, open: string | null = null, expanded = false) => sidebarConversations(list, prefs, limit, open, expanded).shown.map((c) => c.id);

  test("favorites come first, then the rest newest first", () => {
    expect(ids({ show: "all", sort: "recent" })).toEqual(["D2", "D1", "C1", "D3"]);
  });

  test("A to Z ignores a channel's #", () => {
    expect(ids({ show: "all", sort: "name" })).toEqual(["D2", "D3", "C1", "D1"]);
  });

  test("a filter keeps its conversations and the one open", () => {
    expect(ids({ show: "unread", sort: "recent" })).toEqual(["C1"]);
    expect(ids({ show: "unread", sort: "recent" }, 8, "T/D3")).toEqual(["C1", "D3"]);
    expect(ids({ show: "favorites", sort: "recent" })).toEqual(["D2"]);
  });

  test("the limit cuts only the rest, never a favorite, an unread or the open one", () => {
    const cut = sidebarConversations(list, { show: "all", sort: "recent" }, 1, "T/D3", false);
    expect(cut.shown.map((c) => c.id)).toEqual(["D2", "D1", "C1", "D3"]);
    expect(sidebarConversations(list, { show: "all", sort: "recent" }, 1, null, false)).toMatchObject({ hidden: 1 });
    expect(sidebarConversations(list, { show: "all", sort: "recent" }, 1, null, true).hidden).toBe(0);
    // A filter lists everything it keeps.
    expect(sidebarConversations(list, { show: "unread", sort: "recent" }, 0, null, false).hidden).toBe(0);
  });

  test("saved preferences are read back, anything else is the default", () => {
    expect(readListPrefs(JSON.stringify({ show: "unread", sort: "name" }))).toEqual({ show: "unread", sort: "name" });
    expect(readListPrefs("{\"show\":\"toString\"}")).toEqual({ show: "all", sort: "recent" });
    expect(readListPrefs("not json")).toEqual({ show: "all", sort: "recent" });
    expect(readListPrefs(null)).toEqual({ show: "all", sort: "recent" });
  });
});

describe("the reply box's formatting", () => {
  test("marks go round the selection, or a placeholder left selected", () => {
    expect(applyFormat("say hello now", 4, 9, "bold")).toEqual({ text: "say *hello* now", start: 5, end: 10 });
    const empty = applyFormat("", 0, 0, "code");
    expect(empty.text).toBe("`code`");
    expect(empty.text.slice(empty.start, empty.end)).toBe("code");
    expect(applyFormat("x", 0, 1, "block").text).toBe("```\nx\n```");
  });

  test("a quote or a list takes whole lines", () => {
    expect(applyFormat("one\ntwo\nthree", 5, 9, "quote").text).toBe("one\n> two\n> three");
    expect(applyFormat("one\ntwo", 0, 7, "numbers").text).toBe("1. one\n2. two");
    expect(applyFormat("one", 1, 1, "bullets").text).toBe("• one");
  });

  test("a link is written for yardd to make Slack's", () => {
    expect(applyLink("see docs", 4, 8, "https://example.com", "docs").text).toBe("see [docs](https://example.com)");
    expect(applyLink("", 0, 0, "https://example.com", "").text).toBe("https://example.com");
    expect(applyLink("", 0, 0, "https://example.com", "a [b] | c").text).toBe("[a  b    c](https://example.com)");
    expect(linkUrl(" example.com/a ")).toBe("https://example.com/a");
    expect(linkUrl("http://x.test")).toBe("http://x.test");
    expect(linkUrl("javascript:alert(1)")).toBeNull();
    expect(linkUrl("https://a b")).toBeNull();
  });
});

describe("mentioning people from the reply box", () => {
  const people = mentionablePeople([{ id: "U1", name: "Hugo Alves", face: "" }, { id: "U2", name: "Hugo", face: "https://x/y.png" }, { id: "U3", name: "Marc", face: "" }, { id: "U4", name: "Marc", face: "" }]);
  const agent = { id: "a1", name: "Reviewer", handle: "reviewer" };

  test("@ offers agents first, then people by any part of their name", () => {
    expect(suggestMentions([agent], people, "hug").map((a) => a.id)).toEqual(["U1", "U2"]);
    expect(suggestMentions([agent], people, "r").map((a) => a.id)).toEqual(["a1", "U3", "U4"]);
    expect(suggestMentions([agent], people, "alv").map((a) => a.id)).toEqual(["U1"]);
  });

  test("a name counts only whole, the longer one first, the picked one of two", () => {
    expect(mentionedPeople("hi @Hugo Alves and @Marc.", people).map((p) => p.id)).toEqual(["U1", "U3"]);
    expect(mentionedPeople("hi @Hugo, @Hugo Alves", people).map((p) => p.id)).toEqual(["U1", "U2"]);
    expect(mentionedPeople("hi @Marc", people, ["U4"]).map((p) => p.id)).toEqual(["U4"]);
    expect(mentionedPeople("mail a@Marc or @Marcus", people)).toEqual([]);
  });

  test("a person is never an agent call", () => {
    expect(calledAgents("@Marc @reviewer", [agent, ...people])).toEqual(["a1"]);
  });
});
