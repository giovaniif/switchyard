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
export function mergePage(old: SlackPage | undefined, messages: SlackMessage[], users: Record<string, string>, more: boolean, older: boolean, faces: Record<string, string> = {}): SlackPage {
  let list = old?.messages ?? [];
  for (const m of messages) list = upsert(list, m);
  const known = !!old?.messages.length;
  return { messages: list, users: { ...old?.users, ...users }, faces: { ...old?.faces, ...faces }, more: older || !known ? more : old!.more, loading: false };
}

/** A top-level message: in the conversation's own list (a reply sent to the
 *  channel too counts). */
export const topLevel = (m: SlackMessage) => !m.threadTs || m.subtype === "thread_broadcast";

/** One message new or changed (`slack.inbox.message`), into the loaded pages
 *  it belongs to: its conversation's when top level, its thread's, and the
 *  thread it starts. Pages not loaded stay so. */
export function applyMessage(pages: Record<string, SlackPage>, team: string, channel: string, m: SlackMessage, users: Record<string, string>, faces: Record<string, string> = {}): Record<string, SlackPage> {
  const keys = [topLevel(m) ? pageKey(team, channel) : null, m.threadTs ? pageKey(team, channel, m.threadTs) : null, pageKey(team, channel, m.ts)].filter((k): k is string => !!k);
  let next = pages;
  for (const key of keys) {
    const page = next[key];
    if (!page) continue;
    next = { ...next, [key]: { ...page, messages: upsert(page.messages, m), users: { ...page.users, ...users }, faces: { ...page.faces, ...faces } } };
  }
  return next;
}

/** The conversations the sidebar shows: newest first, `limit` of them, but
 *  never hiding one with unread messages or the one open. */
export function shownConversations(list: SlackConversation[], limit: number, open: string | null): SlackConversation[] {
  return list.filter((c, i) => i < limit || c.unread > 0 || `${c.team}/${c.id}` === open);
}

/** What the sidebar's Slack section lists, as he set it (kept per browser). */
export interface SlackListPrefs {
  show: "all" | "unread" | "favorites";
  sort: "recent" | "name";
}
export const SLACK_LIST_DEFAULT: SlackListPrefs = { show: "all", sort: "recent" };
export const SLACK_SHOW_LABEL: Record<SlackListPrefs["show"], string> = { all: "All conversations", unread: "Unread only", favorites: "Favorites only" };
export const SLACK_SORT_LABEL: Record<SlackListPrefs["sort"], string> = { recent: "Most recent", name: "A to Z" };

/** Saved preferences read back; anything else is the default. */
export function readListPrefs(saved: string | null): SlackListPrefs {
  try {
    const v = JSON.parse(saved ?? "null");
    return { show: Object.hasOwn(SLACK_SHOW_LABEL, v?.show) ? v.show : "all", sort: Object.hasOwn(SLACK_SORT_LABEL, v?.sort) ? v.sort : "recent" };
  } catch {
    return SLACK_LIST_DEFAULT;
  }
}

/** The section's rows: favorites first, then the rest in the chosen order
 *  (`list` comes newest first). A filter keeps only its conversations and
 *  the one open, so reading a conversation does not take it from under him.
 *  Unfiltered and not expanded, the rest stops at `limit`, but an unread or
 *  open conversation and a favorite always show. `hidden`: how many Show all
 *  would add. */
export function sidebarConversations(list: SlackConversation[], prefs: SlackListPrefs, limit: number, open: string | null, expanded: boolean): { shown: SlackConversation[]; hidden: number } {
  const isOpen = (c: SlackConversation) => `${c.team}/${c.id}` === open;
  const bare = (c: SlackConversation) => c.name.replace(/^#/, "");
  const ordered = prefs.sort === "name" ? [...list].sort((a, b) => bare(a).localeCompare(bare(b), undefined, { sensitivity: "base" })) : list;
  const kept = ordered.filter((c) => (prefs.show === "unread" ? c.unread > 0 || isOpen(c) : prefs.show === "favorites" ? !!c.favorite || isOpen(c) : true));
  const favorites = kept.filter((c) => c.favorite);
  const rest = kept.filter((c) => !c.favorite);
  const cut = prefs.show === "all" && !expanded;
  const shownRest = cut ? rest.filter((c, i) => i < limit || c.unread > 0 || isOpen(c)) : rest;
  return { shown: [...favorites, ...shownRest], hidden: rest.length - shownRest.length };
}

/** A formatting button of the reply box applied to its text: Slack's marks
 *  round the selection (or a placeholder, left selected), a prefix on each
 *  selected line (a quote, a list), or a link written `[label](url)`, which
 *  yardd makes Slack's own. Answers the new text and selection. */
export type SlackFormat = "bold" | "italic" | "strike" | "code" | "block" | "quote" | "bullets" | "numbers";
const WRAP: Partial<Record<SlackFormat, [string, string, string]>> = {
  bold: ["*", "*", "bold"],
  italic: ["_", "_", "italic"],
  strike: ["~", "~", "strikethrough"],
  code: ["`", "`", "code"],
  block: ["```\n", "\n```", "code"],
};
export function applyFormat(text: string, start: number, end: number, format: SlackFormat): { text: string; start: number; end: number } {
  const wrap = WRAP[format];
  if (wrap) {
    const [open, close, placeholder] = wrap;
    const inner = text.slice(start, end) || placeholder;
    return { text: text.slice(0, start) + open + inner + close + text.slice(end), start: start + open.length, end: start + open.length + inner.length };
  }
  // Whole lines: from the start of the first selected line to the end of the last.
  const from = text.lastIndexOf("\n", start - 1) + 1;
  const nl = text.indexOf("\n", end);
  const to = nl < 0 ? text.length : nl;
  const lines = text.slice(from, to).split("\n");
  const out = lines.map((line, i) => (format === "quote" ? `> ${line}` : format === "bullets" ? `• ${line}` : `${i + 1}. ${line}`)).join("\n");
  return { text: text.slice(0, from) + out + text.slice(to), start: from + out.length, end: from + out.length };
}

/** A link put where the selection is; the selection is its label. */
export function applyLink(text: string, start: number, end: number, url: string, label: string): { text: string; start: number; end: number } {
  const clean = label.replace(/[[\]|\n]/g, " ").trim();
  const link = clean && clean !== url ? `[${clean}](${url})` : url;
  return { text: text.slice(0, start) + link + text.slice(end), start: start + link.length, end: start + link.length };
}

/** A web address as typed, fit for a link: http or https, no spaces. */
export function linkUrl(typed: string): string | null {
  const t = typed.trim();
  const url = /^https?:\/\//i.test(t) ? t : /^[^\s/]+\.[^\s]+$/.test(t) ? `https://${t}` : "";
  return url && !/[\s|<>]/.test(url) ? url : null;
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

/** Who the reply box offers on `@`: an agent the owner can call (one with its
 *  own app, ready, in that workspace), or a person of the workspace
 *  (`person`: `handle` is then their name, what the box writes after the @). */
export interface CallableAgent {
  id: string;
  name: string;
  handle: string;
  person?: true;
  face?: string;
}

/** The workspace's people as the `@` list offers them. */
export function mentionablePeople(people: { id: string; name: string; face: string }[]): CallableAgent[] {
  return people.map((p) => ({ id: p.id, name: p.name, handle: p.name, person: true as const, ...(p.face ? { face: p.face } : {}) }));
}

/** At most this many people under the agents in the `@` list. */
const PEOPLE_OFFERED = 8;

/** Agents first, then people, for a typed `@query`. */
export function suggestMentions(agents: CallableAgent[], people: CallableAgent[], query: string): CallableAgent[] {
  return [...suggestAgents(agents, query), ...suggestAgents(people, query).slice(0, PEOPLE_OFFERED)];
}

/** The people a message mentions: each whose `@Name` stands whole in it.
 *  Of two with one name, the one he picked (`picked`, ids) counts, else the
 *  first; a longer name is read before a shorter one inside it. */
export function mentionedPeople(text: string, people: CallableAgent[], picked: string[] = []): CallableAgent[] {
  const byName = new Map<string, CallableAgent>();
  for (const p of people) {
    const had = byName.get(p.name);
    if (!had || (picked.includes(p.id) && !picked.includes(had.id))) byName.set(p.name, p);
  }
  let rest = text;
  const out: CallableAgent[] = [];
  for (const p of [...byName.values()].sort((a, b) => b.name.length - a.name.length)) {
    const at = `@${p.name}`;
    let found = false;
    let from = 0;
    for (let i = rest.indexOf(at, from); i >= 0; i = rest.indexOf(at, from)) {
      const end = i + at.length;
      const alone = !/[\p{L}\p{N}]/u.test(rest[i - 1] ?? "") && !/[\p{L}\p{N}]/u.test(rest[end] ?? "");
      if (alone) {
        found = true;
        // Taken: a shorter name inside it is not another mention.
        rest = rest.slice(0, i) + " ".repeat(at.length) + rest.slice(end);
      }
      from = end;
    }
    if (found) out.push(p);
  }
  return out;
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

/** Agents offered for a typed `@query`: the one whose handle it is first
 *  (so the highlight sits on a mention typed in full), then those whose
 *  handle or name starts with it, then any that contain it. */
export function suggestAgents(agents: CallableAgent[], query: string): CallableAgent[] {
  const fields = (a: CallableAgent) => [a.handle.toLowerCase(), a.name.toLowerCase()];
  const rank = (a: CallableAgent) => (a.handle === query ? 0 : fields(a).some((f) => f.startsWith(query)) ? 1 : 2);
  return agents.filter((a) => fields(a).some((f) => f.includes(query))).sort((a, b) => rank(a) - rank(b));
}

/** What a key does in the reply box, the way Slack's own picker works: with
 *  the @ list open, Up and Down move the highlight round the ends, Enter or
 *  Tab writes the highlighted handle and Escape closes the list. Enter sends
 *  when no list is open, or when what is typed already is the highlighted
 *  handle (the mention is complete). null: the textarea's own key (Shift+Enter
 *  is a new line; an IME's Enter confirms its text). */
export type ComposerKey = { do: "move"; to: number } | { do: "pick"; agent: CallableAgent } | { do: "close" } | { do: "send" } | null;
export function composerKey(e: { key: string; shiftKey: boolean; composing: boolean }, offered: CallableAgent[], active: number, query: string | null): ComposerKey {
  if (e.composing) return null;
  const enter = e.key === "Enter" && !e.shiftKey;
  if (offered.length) {
    const at = Math.min(active, offered.length - 1);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") return { do: "move", to: (at + (e.key === "ArrowDown" ? 1 : offered.length - 1)) % offered.length };
    if (e.key === "Escape") return { do: "close" };
    if (enter && offered[at].handle === query) return { do: "send" };
    if (enter || (e.key === "Tab" && !e.shiftKey)) return { do: "pick", agent: offered[at] };
  }
  return enter ? { do: "send" } : null;
}

/** The agents a message calls: each `@handle` written as a word. */
export function calledAgents(text: string, agents: CallableAgent[]): string[] {
  return agents.filter((a) => !a.person && new RegExp(`(^|[^a-z0-9._-])@${a.handle.replace(/[.]/g, "\\.")}(?![a-z0-9_-])(?!\\.[a-z0-9_-])`, "i").test(text)).map((a) => a.id);
}

/** A person's two initials, for the round mark beside their messages. */
export function initials(name: string): string {
  const words = name.replace(/^[@#]/, "").split(/[\s._-]+/).filter(Boolean);
  const letters = words.length > 1 ? words[0][0] + words[1][0] : (words[0] ?? "?").slice(0, 2);
  return letters.toUpperCase();
}
