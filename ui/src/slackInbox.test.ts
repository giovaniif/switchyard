import { describe, expect, test } from "bun:test";
import type { SlackConversation, SlackMessage } from "./protocol";
import { applyMessage, calledAgents, initials, mentionQuery, mergePage, pageKey, shownConversations, startsGroup } from "./slackInbox";
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
    expect(mergePage({ messages: [], users: {}, more: false, loading: true }, [msg("3")], {}, true, false).more).toBe(true);
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
