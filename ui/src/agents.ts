// Words and rules for agents (yardd's agents.rs): @mentions in a message,
// the card's status line, faces. Pure, so bun test covers them.
import type { Agent, AgentColor, AgentFace, MessageOrigin, Models, SessionMeta } from "./protocol";
import { isSlack, slackFrom } from "./slack";
import { ENGINE_NAME } from "./engines";

export const AGENT_COLORS: AgentColor[] = ["blue", "green", "amber", "rose", "violet", "teal", "orange", "slate"];
export const AGENT_NAME_MAX = 24;
/** The avatar's letter. */
export const initial = (name: string) => (name.trim()[0] ?? "?").toUpperCase();

/** Avatar faces, drawn white on the avatar's colour in a 24-unit box, in
 *  parts so they can move: `eyes` blink and glance, `mouth` talks while the
 *  agent works, `frame` (glasses, a brow) moves with the eyes. Eyes are
 *  zero-length strokes: round caps make them dots. */
export interface FaceParts {
  eyes: string;
  mouth: string;
  frame?: string;
}
export const FACES: Record<AgentFace, FaceParts> = {
  smile: { eyes: "M9 10v.01M15 10v.01", mouth: "M8.5 14q3.5 3.5 7 0" },
  grin: { eyes: "M9 9.5v.01M15 9.5v.01", mouth: "M8 13h8a4 4 0 0 1-8 0Z" },
  wink: { eyes: "M9 10v.01M13.5 10.5q1.5-1.5 3 0", mouth: "M8.5 14q3.5 3.5 7 0" },
  calm: { eyes: "M7.5 10q1.5 1.5 3 0M13.5 10q1.5 1.5 3 0", mouth: "M10 15.5h4" },
  glasses: {
    frame: "M6 10.5a2.5 2.5 0 1 0 5 0a2.5 2.5 0 1 0-5 0M13 10.5a2.5 2.5 0 1 0 5 0a2.5 2.5 0 1 0-5 0M11 10.5h2",
    eyes: "M8.5 10.5v.01M15.5 10.5v.01",
    mouth: "M9.5 16q2.5 1.5 5 0",
  },
  bot: { eyes: "M7.5 9h3v2.5h-3ZM13.5 9h3v2.5h-3Z", mouth: "M9 15.5h6" },
  happy: { eyes: "M7.5 10.5q1.5-2 3 0M13.5 10.5q1.5-2 3 0", mouth: "M8 13h8a4 4 0 0 1-8 0Z" },
  curious: { frame: "M13.5 7.8q1.5-1.4 3 0", eyes: "M9 10.5v.01M15 10.5v.01", mouth: "M12 14.2a1.3 1.3 0 1 0 0 2.6a1.3 1.3 0 1 0 0-2.6Z" },
};
export const AGENT_FACES = Object.keys(FACES) as AgentFace[];

/** The face an agent shows: its own, or a steady one picked from its id. */
export function faceOf(agent: Pick<Agent, "face" | "id">): AgentFace {
  if (agent.face && agent.face in FACES) return agent.face;
  let h = 0;
  for (const ch of agent.id) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return AGENT_FACES[h % AGENT_FACES.length];
}

/** A steady offset in seconds (negative: already part-way through), so agents
 *  side by side do not blink together. */
export function blinkDelay(id: string): number {
  let h = 7;
  for (const ch of id) h = (h * 33 + ch.charCodeAt(0)) >>> 0;
  return -((h % 50) / 10);
}

/** yardd's rule, said before it refuses: one word that reads as `@Name`. */
export function nameProblem(name: string, agents: Agent[], id?: string): string | null {
  const n = name.trim();
  if (!n) return "Give it a name.";
  if (!/^\p{L}[\p{L}\p{N}_-]*$/u.test(n)) return "One word: letters, digits, - or _, starting with a letter, so it can be @mentioned.";
  if ([...n].length > AGENT_NAME_MAX) return `At most ${AGENT_NAME_MAX} characters.`;
  if (agents.some((a) => a.id !== id && a.name.toLowerCase() === n.toLowerCase())) return `Another agent is already called ${n}.`;
  return null;
}

/** The `@word` the caret is in, if any: `@` at the start or after a space,
 *  then name characters up to the caret. */
export function mentionAt(text: string, caret: number): { start: number; query: string } | null {
  const m = /(^|\s)@([\p{L}\p{N}_-]*)$/u.exec(text.slice(0, caret));
  if (!m) return null;
  return { start: caret - m[2].length - 1, query: m[2] };
}

/** Agents for a typed `@query`: names that start with it first, then any that contain it. */
export function matchAgents(agents: Agent[], query: string, max = 6): Agent[] {
  const q = query.toLowerCase();
  const starts = agents.filter((a) => a.name.toLowerCase().startsWith(q));
  const contains = agents.filter((a) => !starts.includes(a) && a.name.toLowerCase().includes(q));
  return [...starts, ...contains].slice(0, max);
}

/** Replace the `@query` at `start` with the agent's name and a space. */
export function insertMention(text: string, start: number, caret: number, name: string): { text: string; caret: number } {
  const after = text.slice(caret).replace(/^[\p{L}\p{N}_-]*/u, "");
  const head = `${text.slice(0, start)}@${name} `;
  return { text: head + after.replace(/^ /, ""), caret: head.length };
}

/** A message that starts with `@Name` of an agent goes to that agent; `rest`
 *  is what it is sent (the mention dropped). */
export function leadingMention(text: string, agents: Agent[]): { agent: Agent; rest: string } | null {
  const m = /^\s*@([\p{L}\p{N}_-]+)(?=\s|$)/u.exec(text);
  if (!m) return null;
  const agent = agents.find((a) => a.name.toLowerCase() === m[1].toLowerCase());
  return agent ? { agent, rest: text.slice(m[0].length).trim() } : null;
}

/** The host an agent runs on: `local`, or a remote host's id. */
export const agentHost = (agent: Pick<Agent, "host">): string => agent.host || "local";

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
  const name = typeof origin.name === "string" && origin.name ? origin.name : null;
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
