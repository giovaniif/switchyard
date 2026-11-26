// The owner's own Slack in the page: loaded pages of messages (a
// conversation's, or a thread's), merged as yardd sends them (a page, an
// older page, one message new or changed), and what the sidebar shows.
import type { SlackAgentApp, SlackConversation, SlackMessage, SlackPage } from "./protocol";

/** A conversation's page key, or a thread's. */
export const pageKey = (team: string, channel: string, thread?: string | null) => (thread ? `${team}/${channel}/${thread}` : `${team}/${channel}`);

const byTs = (a: SlackMessage, b: SlackMessage) => (a.ts < b.ts ? -1 : a.ts > b.ts ? 1 : 0);

function upsert(list: SlackMessage[], m: SlackMessage): SlackMessage[] {
  const i = list.findIndex((x) => x.ts === m.ts);
  if (i >= 0) return list.map((x, j) => (j === i ? m : x));
  return [...list, m].sort(byTs);
}

/** A page yardd answered (`slack.inbox.messages`) merged into what is loaded:
 *  by ts, so a repeat changes nothing. `older`: it was a Load older page, so
 *  its `more` is the page's; a fresh newest page keeps what was known, unless
 *  nothing was loaded yet (a page only waiting for its first answer). */
export function mergePage(old: SlackPage | undefined, messages: SlackMessage[], users: Record<string, string>, more: boolean, older: boolean): SlackPage {
  let list = old?.messages ?? [];
  for (const m of messages) list = upsert(list, m);
  const known = !!old?.messages.length;
  return { messages: list, users: { ...old?.users, ...users }, more: older || !known ? more : old!.more, loading: false };
}

/** A top-level message: in the conversation's own list (a reply sent to the
 *  channel too counts). */
export const topLevel = (m: SlackMessage) => !m.threadTs || m.subtype === "thread_broadcast";

/** One message new or changed (`slack.inbox.message`), into the loaded pages
 *  it belongs to: its conversation's when top level, its thread's, and the
 *  thread it starts. Pages not loaded stay so. */
export function applyMessage(pages: Record<string, SlackPage>, team: string, channel: string, m: SlackMessage, users: Record<string, string>): Record<string, SlackPage> {
  const keys = [topLevel(m) ? pageKey(team, channel) : null, m.threadTs ? pageKey(team, channel, m.threadTs) : null, pageKey(team, channel, m.ts)].filter((k): k is string => !!k);
  let next = pages;
  for (const key of keys) {
    const page = next[key];
    if (!page) continue;
    next = { ...next, [key]: { ...page, messages: upsert(page.messages, m), users: { ...page.users, ...users } } };
  }
  return next;
}

/** The conversations the sidebar shows: newest first, `limit` of them, but
 *  never hiding one with unread messages or the one open. */
export function shownConversations(list: SlackConversation[], limit: number, open: string | null): SlackConversation[] {
  return list.filter((c, i) => i < limit || c.unread > 0 || `${c.team}/${c.id}` === open);
}

/** Messages that open a new group in the list: another author, more than
 *  five minutes since the previous one, or another day. */
export function startsGroup(prev: SlackMessage | undefined, m: SlackMessage): boolean {
  if (!prev) return true;
  const author = (x: SlackMessage) => x.user || x.username || "";
  if (author(prev) !== author(m)) return true;
  const a = Number(prev.ts) * 1000;
  const b = Number(m.ts) * 1000;
  return b - a > 5 * 60_000 || new Date(a).toDateString() !== new Date(b).toDateString();
}

/** An agent the owner can call in a Slack message: one with its own app,
 *  ready, in that workspace. */
export interface CallableAgent {
  id: string;
  name: string;
  handle: string;
}

/** Agents with their own app ready in this workspace and a bot yardd can
 *  mention: the ones he can call. On a hub, a host's agents are listed with
 *  their apps under the same prefixed ids, so an agent on the server is
 *  offered too, even while that host is offline (Slack carries the mention). */
export function callableAgents(agents: { id: string; name: string }[], apps: Record<string, SlackAgentApp> | undefined, team: string): CallableAgent[] {
  return agents.flatMap((a) => {
    const app = apps?.[a.id];
    return app && app.step === "ready" && app.teamId === team && app.handle && app.botUser ? [{ id: a.id, name: a.name, handle: app.handle }] : [];
  });
}

/** The `@handle` being typed at the caret, if any: what to suggest. */
export function mentionQuery(text: string, caret: number): string | null {
  const m = /(?:^|\s)@([a-z0-9._-]*)$/i.exec(text.slice(0, caret));
  return m ? m[1].toLowerCase() : null;
}

/** Agents offered for a typed `@query`. */
export function suggestAgents(agents: CallableAgent[], query: string): CallableAgent[] {
  return agents.filter((a) => a.handle.includes(query) || a.name.toLowerCase().includes(query));
}

/** What a key does in the reply box: the @ list's own keys, Send, or the
 *  textarea's default (null). */
export type ComposerKey = { do: "move"; to: number } | { do: "pick"; agent: CallableAgent } | { do: "close" } | { do: "send" } | null;
export function composerKey(e: { key: string; shiftKey: boolean; composing: boolean }, offered: CallableAgent[], _active: number, _query: string | null): ComposerKey {
  if (e.key !== "Enter" || e.shiftKey || e.composing) return null;
  return offered.length === 1 ? { do: "pick", agent: offered[0] } : { do: "send" };
}

/** The agents a message calls: each `@handle` written as a word. */
export function calledAgents(text: string, agents: CallableAgent[]): string[] {
  return agents.filter((a) => new RegExp(`(^|[^a-z0-9._-])@${a.handle.replace(/[.]/g, "\\.")}(?![a-z0-9_-])(?!\\.[a-z0-9_-])`, "i").test(text)).map((a) => a.id);
}

/** A person's two initials, for the round mark beside their messages. */
export function initials(name: string): string {
  const words = name.replace(/^[@#]/, "").split(/[\s._-]+/).filter(Boolean);
  const letters = words.length > 1 ? words[0][0] + words[1][0] : (words[0] ?? "?").slice(0, 2);
  return letters.toUpperCase();
}
