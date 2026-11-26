// Words and rules for agents (yardd's agents.rs): @mentions in a message,
// the card's status line. Faces are in faces.ts. Pure, so bun test covers them.
import type { Agent, AgentColor, HostEntry, MessageOrigin, Models, SessionMeta } from "./protocol";
import { isSlack, slackFrom } from "./slack";
import { ENGINE_NAME } from "./engines";

export const AGENT_COLORS: AgentColor[] = ["blue", "green", "amber", "rose", "violet", "teal", "orange", "slate"];
/** A display name's length, in characters (yardd's NAME_MAX). */
export const AGENT_NAME_MAX = 40;
/** The avatar's letter. */
export const initial = (name: string) => (name.trim()[0] ?? "?").toUpperCase();

// Faces are drawn in faces.ts (3D characters that move); the names stay
// reachable from here.
export { AGENT_FACES, FACES, avatarSvg, blinkDelay, faceOf } from "./faces";

/** A display name as yardd keeps it: trimmed, each run of spaces one space. */
export const cleanName = (name: string) => name.trim().split(/\s+/).filter(Boolean).join(" ");

// Control characters, and zero-width and direction marks that could make one name look like another.
const INVISIBLE = /[\p{Cc}\u00ad\u200b-\u200f\u202a-\u202e\u2060-\u2069\ufeff]/u;

/** yardd's rule, said before it refuses: any printable text up to
 *  AGENT_NAME_MAX characters, unique ignoring case. */
export function nameProblem(name: string, agents: Agent[], id?: string): string | null {
  const n = cleanName(name);
  if (!n) return "Give it a name.";
  if ([...n].length > AGENT_NAME_MAX) return `At most ${AGENT_NAME_MAX} characters.`;
  if (INVISIBLE.test(n)) return "No control or invisible characters.";
  if (agents.some((a) => a.id !== id && a.name.toLowerCase() === n.toLowerCase())) return `Another agent is already called ${n}.`;
  return null;
}

// What a handle is made of: letters (with their marks), digits, - and _.
const HANDLE_CHAR = "[\\p{L}\\p{M}\\p{N}_-]";

/** The handle a name reads as (yardd's `handle_base`): its letters and digits
 *  as typed, one `-` for whatever stands between them, starting with a
 *  letter; `agent` for a name with no letter. "Custom Domains Triage" is
 *  `Custom-Domains-Triage`. */
export function handleBase(name: string): string {
  let out = "";
  let gap = false;
  for (const c of name) {
    if (!/[\p{L}\p{M}\p{N}_]/u.test(c)) {
      gap = true;
      continue;
    }
    if (!out && !/\p{L}/u.test(c)) continue;
    if (gap && out) out += "-";
    gap = false;
    out += c;
  }
  return out || "agent";
}

/** The handle a new name would get among the other agents' (yardd's
 *  `handle_for`), to say it before saving. */
export function handleFor(name: string, agents: Agent[], id?: string): string {
  const taken = new Set(agents.filter((a) => a.id !== id).map((a) => handleOf(a).toLowerCase()));
  const old = agents.find((a) => a.id === id)?.handle;
  const base = handleBase(name);
  if (old && !taken.has(old.toLowerCase())) {
    const lo = old.toLowerCase();
    const lb = base.toLowerCase();
    if (lo === lb) return base;
    if (lo.startsWith(`${lb}-`) && /^\d+$/.test(lo.slice(lb.length + 1))) return old;
  }
  if (!taken.has(base.toLowerCase())) return base;
  for (let n = 2; ; n++) if (!taken.has(`${base}-${n}`.toLowerCase())) return `${base}-${n}`;
}

/** What an agent is `@mentioned` by: its handle, or its name from a yardd
 *  before handles (those names were handles). */
export const handleOf = (agent: Pick<Agent, "name" | "handle">): string => agent.handle || agent.name;

/** The `@word` the caret is in, if any: `@` at the start or after a space,
 *  then handle characters up to the caret. */
export function mentionAt(text: string, caret: number): { start: number; query: string } | null {
  const m = new RegExp(`(^|\\s)@(${HANDLE_CHAR}*)$`, "u").exec(text.slice(0, caret));
  if (!m) return null;
  return { start: caret - m[2].length - 1, query: m[2] };
}

/** Agents for a typed `@query`: those whose handle or name starts with it
 *  first, then any where a word of the name, or the handle, contains it. */
export function matchAgents(agents: Agent[], query: string, max = 6): Agent[] {
  const q = query.toLowerCase();
  const words = (a: Agent) => [handleOf(a), a.name, ...a.name.split(/[^\p{L}\p{M}\p{N}_]+/u)].map((w) => w.toLowerCase());
  const starts = agents.filter((a) => words(a).some((w) => w.startsWith(q)));
  const contains = agents.filter((a) => !starts.includes(a) && words(a).some((w) => w.includes(q)));
  return [...starts, ...contains].slice(0, max);
}

/** Replace the `@query` at `start` with the agent's handle and a space. */
export function insertMention(text: string, start: number, caret: number, handle: string): { text: string; caret: number } {
  const after = text.slice(caret).replace(new RegExp(`^${HANDLE_CHAR}*`, "u"), "");
  const head = `${text.slice(0, start)}@${handle} `;
  return { text: head + after.replace(/^ /, ""), caret: head.length };
}

/** A message that starts with an agent's `@handle` goes to that agent;
 *  `rest` is what it is sent (the mention dropped). */
export function leadingMention(text: string, agents: Agent[]): { agent: Agent; rest: string } | null {
  const m = new RegExp(`^\\s*@(${HANDLE_CHAR}+)(?=\\s|$)`, "u").exec(text);
  if (!m) return null;
  const agent = agents.find((a) => handleOf(a).toLowerCase() === m[1].toLowerCase());
  return agent ? { agent, rest: text.slice(m[0].length).trim() } : null;
}

/** The host an agent runs on: `local`, or a remote host's id. */
export const agentHost = (agent: Pick<Agent, "host">): string => agent.host || "local";

/** The agents on one host: names are unique per host (yardd's agents.rs). */
export const onHost = <A extends Pick<Agent, "host">>(agents: A[], host: string): A[] => agents.filter((a) => agentHost(a) === host);

/** An agent's id without its host (`<hostId>~<id>` for a host's agent). */
const bareId = (id: string) => id.slice(id.indexOf("~") + 1);

/** Where an agent the page names went: itself, or the one agent with the
 *  same id under another host, after it moved or its host took it over
 *  from this Mac. Null when there is no such agent. */
export function relocated(agents: Pick<Agent, "id">[], id: string): string | null {
  if (agents.some((a) => a.id === id)) return id;
  const same = agents.filter((a) => bareId(a.id) === bareId(id));
  return same.length === 1 ? same[0].id : null;
}

/** An agent this Mac still keeps for a host whose Switchyard is too old to
 *  own it (the older way: the record here, a hidden copy there). Its
 *  Slack app and record stay on this Mac, so it stops while the Mac sleeps:
 *  say what to do. Null for every other agent. */
export function keptNote(agent: Pick<Agent, "id" | "host" | "name" | "handOverError">, hosts: HostEntry[]): string | null {
  const host = agentHost(agent);
  if (host === "local" || agent.id.includes("~")) return null;
  const h = hosts.find((x) => x.id === host);
  // Its host refused it for good (another Mac's copy there): yardd stopped trying.
  if (agent.handOverError) return `${agent.name} could not move to ${h?.name ?? "its host"}: ${agent.handOverError}`;
  if (!h || h.ownAgents !== false) return null;
  return `${agent.name} still lives on this Mac because ${h.name} runs an older Switchyard. Use Update Switchyard on ${h.name}'s card in Hosts so it lives there and answers from any Mac.`;
}

/** The agent a session is a chat with: on this host by its tag; on a remote
 *  host only when one of the hub's agents names it as its conversation (or
 *  one it left there), since a remote's tag may name one of its own. */
export function agentOf<A extends Pick<Agent, "id" | "chat" | "previous">>(agents: A[], s: Pick<SessionMeta, "id" | "agent" | "host"> | undefined): A | undefined {
  if (!s) return undefined;
  if (s.host && s.host !== "local") return agents.find((a) => a.chat === s.id || a.previous?.includes(s.id));
  return s.agent ? agents.find((a) => a.id === s.agent) : undefined;
}

/** An agent's conversation: kept out of the sidebar, shown on the Agents
 *  page and its nav row. A session tagged with an agent this window does
 *  not know (a hub's agent that runs here) is an ordinary session. */
export const isAgentConversation = (s: Pick<SessionMeta, "id" | "agent" | "host">, agents: Pick<Agent, "id" | "chat" | "previous">[]): boolean => !!agentOf(agents, s);

/** A conversation the agent left on a host it moved away from: readable,
 *  not answered from. */
export const isEarlierConversation = (agent: Agent, sessionId: string): boolean => agent.chat !== sessionId && !!agent.previous?.includes(sessionId);

/** Where an agent works, in words: "its own folder on box". The host is
 *  named only when it is not this one. */
export function agentWhere(agent: Agent, hostName: (id: string) => string, folder: string): string {
  const where = agent.cwd ? folder : "its own folder";
  return agentHost(agent) === "local" ? where : `${where} on ${hostName(agentHost(agent))}`;
}

/** What the form says about a change of host: the conversation cannot
 *  follow, so the next message starts a new one there with the latest
 *  exchanges, and the old one stays readable where it is. Null when nothing
 *  moves. */
export function moveNote(agent: Agent | undefined, host: string, hostName: (id: string) => string): string | null {
  if (!agent || agentHost(agent) === host) return null;
  const to = hostName(host);
  if (!agent.chat) return `It moves to ${to}.`;
  return `Its conversation stays on ${hostName(agentHost(agent))}, readable. The next message starts a new one on ${to} with its notes and the latest messages.`;
}

/** The agent's conversation, if it has one yet. */
export function conversationOf(agent: Agent, sessions: Record<string, SessionMeta>): SessionMeta | undefined {
  return agent.chat ? sessions[agent.chat] : undefined;
}

const ago = (ms: number) => {
  const m = Math.round(ms / 60_000);
  if (m < 1) return "just now";
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  return `${Math.round(h / 24)} d ago`;
};

/** `answered`: an answer the user has not opened yet. `needsYou`: the agent
 *  asked for the user (`notify_owner`) and they have not opened it since. */
export type AgentTone = "live" | "approval" | "needsYou" | "answered" | "ok" | "bad" | "quiet" | "offline";

/** The card's one status line, from the agent's conversation. `seenAt`: the
 *  turn end the user last had open (rowState's `yard.seen`). `offline`: the
 *  name of the agent's host while it cannot be reached. */
export function agentStatus(agent: Agent, sessions: Record<string, SessionMeta>, now: number, seenAt = Infinity, offline?: string): { tone: AgentTone; text: string; sessionId: string | null } {
  const chat = conversationOf(agent, sessions);
  // Its last state there is stale, and nothing can be sent until it is back.
  if (offline) return { tone: "offline", text: `${offline} is offline`, sessionId: chat?.id ?? null };
  if (!chat) return { tone: "quiet", text: "No messages yet", sessionId: null };
  const at = { sessionId: chat.id };
  if (chat.status === "needs_approval") return { tone: "approval", text: "Needs approval", ...at };
  // Asked for the user: that stays in view, working or not, until they look.
  if (chat.needsYou) return { tone: "needsYou", text: `Needs you · ${oneLine(chat.needsYou.message)}`, ...at };
  if (chat.status === "running") return { tone: "live", text: "Working…", ...at };
  if (chat.status === "queued") return { tone: "approval", text: "Waiting for room on the host", ...at };
  const when = ago(now - (chat.turnEndedAt ?? chat.updatedAt));
  if (chat.lastTurn === "failed") return { tone: "bad", text: `Failed ${when}`, ...at };
  if (chat.lastTurn === "interrupted") return { tone: "ok", text: `Stopped ${when}`, ...at };
  const unseen = (chat.turnEndedAt ?? 0) > seenAt;
  return { tone: unseen ? "answered" : "ok", text: `Answered ${when}${chat.lastMessage ? ` · ${chat.lastMessage}` : ""}`, ...at };
}

/** What the Agents nav row shows: the most pressing state of any agent's
 *  conversation, as the sidebar marks a session. */
export function agentsAttention(agents: Agent[], sessions: Record<string, SessionMeta>, seenAt: (id: string) => number): "approval" | "needsYou" | "working" | "failed" | "answered" | null {
  const tones = agents.map((a) => agentStatus(a, sessions, 0, conversationOf(a, sessions) ? seenAt(a.chat!) : Infinity).tone);
  const failedUnseen = agents.some((a) => {
    const c = conversationOf(a, sessions);
    return c?.lastTurn === "failed" && (c.turnEndedAt ?? 0) > seenAt(c.id);
  });
  if (tones.includes("approval")) return "approval";
  // An agent working on behind a call for the user still needs them.
  if (agents.some((a) => conversationOf(a, sessions)?.needsYou)) return "needsYou";
  if (tones.includes("live")) return "working";
  if (failedUnseen) return "failed";
  if (tones.includes("answered")) return "answered";
  return null;
}

const oneLine = (text: string) => text.split(/\s+/).filter(Boolean).join(" ");

/** The badge above a message the user did not type: who it came from.
 *  Null for the user's own (and for a kind this build does not know). */
export function originLabel(origin: MessageOrigin | undefined): { text: string; agentId: string | null } | null {
  if (!origin || origin.kind === "user") return null;
  // Said by its handle, as it would be mentioned; its name from a yardd before handles.
  const name = typeof origin.handle === "string" && origin.handle ? origin.handle : typeof origin.name === "string" && origin.name ? origin.name : null;
  const agentId = typeof origin.agentId === "string" ? origin.agentId : null;
  if (origin.kind === "agent") return { text: name ? `From @${name}` : "From another agent", agentId };
  if (origin.kind === "agentReply") return { text: name ? `Reply from @${name}` : "Reply from another agent", agentId };
  if (isSlack(origin)) return { text: slackFrom(origin), agentId: null };
  return null;
}

/** "Fake Fast · Low · Plan only": how the agent runs, in the picker's words.
 *  `account`: its account's name, where the engine has a choice (accountLabel). */
export function runsOn(agent: Agent, models: Models, permissionLabel: string, account?: string | null): string {
  const list = models[agent.engine] ?? [];
  // No model of its own: the host's default, which a new chat starts on.
  const fallback = list.find((m) => m.isDefault) ?? list[0];
  const model = agent.model ? (list.find((m) => m.id === agent.model)?.name ?? agent.model) : (fallback?.name ?? `${ENGINE_NAME[agent.engine]} default`);
  const effort = agent.effort ? agent.effort[0].toUpperCase() + agent.effort.slice(1) : "";
  return [model, account, effort, permissionLabel].filter(Boolean).join(" · ");
}

/** The model an agent's pickers show: its own, else the host's default for
 *  its engine (which a new conversation starts on), with an effort the model
 *  has. A model the host does not list is shown as it is. */
export function agentChoice(agent: Pick<Agent, "engine" | "model" | "effort">, models: Models): { engine: Agent["engine"]; model: string; effort: string } {
  const list = models[agent.engine] ?? [];
  const m = agent.model ? list.find((x) => x.id === agent.model) : (list.find((x) => x.isDefault) ?? list[0]);
  if (!m) return { engine: agent.engine, model: agent.model, effort: agent.effort };
  const effort = m.efforts.includes(agent.effort) ? agent.effort : (m.defaultEffort ?? m.efforts[0] ?? agent.effort);
  return { engine: agent.engine, model: m.id, effort };
}

/** What a pick of another host saves with it: the agent's own folder (one on
 *  the old host means nothing on the new), and the host's default model or
 *  account where the new host lacks the agent's. `accounts`: the account ids
 *  the new host has for the agent's engine. */
export function hostChange(agent: Pick<Agent, "engine" | "model" | "account">, host: string, models: Models, accounts: string[]): Partial<Agent> {
  const listed = (models[agent.engine] ?? []).some((m) => m.id === agent.model);
  return { host, cwd: "", model: listed ? agent.model : "", account: accounts.includes(agent.account) ? agent.account : "default" };
}
