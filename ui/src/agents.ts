// Words and rules for agents (yardd's agents.rs): @mentions in a message,
// the card's status line, the avatar's letter. Pure, so bun test covers them.
import type { Agent, AgentColor, Models, SessionMeta } from "./protocol";
import { ENGINE_NAME } from "./engines";

export const AGENT_COLORS: AgentColor[] = ["blue", "green", "amber", "rose", "violet", "teal", "orange", "slate"];
export const AGENT_NAME_MAX = 24;
/** The built-in host agent's id. */
export const HOST_AGENT = "scout";

/** The avatar's letter. */
export const initial = (name: string) => (name.trim()[0] ?? "?").toUpperCase();

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

/** The agent a session is a chat with. Agents live on the hub, so a remote
 *  session's tag names one of its own host's and is not looked up here. */
export function agentOf(agents: Agent[], s: Pick<SessionMeta, "agent" | "host"> | undefined): Agent | undefined {
  if (!s?.agent || (s.host && s.host !== "local")) return undefined;
  return agents.find((a) => a.id === s.agent);
}

/** An agent's chats, newest first. */
export function chatsOf(sessions: Record<string, SessionMeta>, id: string): SessionMeta[] {
  return Object.values(sessions)
    .filter((s) => s.agent === id && (!s.host || s.host === "local"))
    .sort((a, b) => b.updatedAt - a.updatedAt);
}

/** The ongoing chat, if it still exists. */
export function ongoing(agent: Agent, sessions: Record<string, SessionMeta>): SessionMeta | undefined {
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

export type AgentTone = "live" | "approval" | "ok" | "bad" | "quiet";

/** The card's one status line, from its ongoing chat. */
export function agentStatus(agent: Agent, sessions: Record<string, SessionMeta>, now: number): { tone: AgentTone; text: string; sessionId: string | null } {
  const chat = ongoing(agent, sessions);
  if (!chat) return { tone: "quiet", text: "No chat yet", sessionId: null };
  const at = { sessionId: chat.id };
  if (chat.status === "running") return { tone: "live", text: "Working…", ...at };
  if (chat.status === "needs_approval") return { tone: "approval", text: "Needs your approval", ...at };
  if (chat.status === "queued") return { tone: "approval", text: "Waiting for room on the host", ...at };
  if (chat.lastTurn === "failed") return { tone: "bad", text: `Failed ${ago(now - (chat.turnEndedAt ?? chat.updatedAt))}`, ...at };
  return { tone: "ok", text: `Last chat ${ago(now - (chat.turnEndedAt ?? chat.updatedAt))}`, ...at };
}

/** "Fake Fast · Low · Plan only": how the agent runs, in the picker's words. */
export function runsOn(agent: Agent, models: Models, permissionLabel: string): string {
  const list = models[agent.engine] ?? [];
  // No model of its own: the host's default, which a new chat starts on.
  const fallback = list.find((m) => m.isDefault) ?? list[0];
  const model = agent.model ? (list.find((m) => m.id === agent.model)?.name ?? agent.model) : (fallback?.name ?? `${ENGINE_NAME[agent.engine]} default`);
  const effort = agent.effort ? agent.effort[0].toUpperCase() + agent.effort.slice(1) : "";
  return [model, effort, permissionLabel].filter(Boolean).join(" · ");
}

/** Where the host agent reads from, in words. */
export function scopeLabel(scope: string, hostName: (id: string) => string): string {
  return scope === "all" ? "All hosts" : hostName(scope);
}
