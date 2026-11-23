// Agents: the page of cards, an agent's start page (a chat not begun yet) and
// the create/edit form. An agent's chats are ordinary sessions tagged with it
// (yardd's agents.rs); the session view shows whose they are.
import { useEffect, useMemo, useState } from "react";
import type { Agent, AgentColor, Attachment, Permission, SessionMeta } from "./protocol";
import { hostName, hostOnline, homeOn, modelsOn, type Actions, type State, type UploadTarget } from "./store";
import { AGENT_COLORS, agentStatus, chatsOf, initial, leadingMention, nameProblem, ongoing, runsOn, scopeLabel, type AgentTone } from "./agents";
import { ENGINE_NAME, pickerEngines } from "./engines";
import { duration } from "./thread";
import {
  Composer,
  EffortPicker,
  FolderPicker,
  HostPicker,
  Icon,
  MenuButton,
  ModelPicker,
  PERMISSIONS,
  PermissionPicker,
  PLUS,
  effortFor,
  loadChoice,
  readRecent,
  tilde,
} from "./App";

const PENCIL = "M4 20h4L19 9l-4-4L4 16ZM13.5 6.5l4 4";
const LIST = "M9 6h11M9 12h11M9 18h11M4 6h.01M4 12h.01M4 18h.01";
const MARK: Record<AgentTone, string> = { live: "working", approval: "approval", ok: "done", bad: "failed", quiet: "idle" };
const HOST_QUESTIONS = ["What’s happening on my server?", "What changed since yesterday?", "Where is the API deployed?"];

const permissionLabel = (p: Permission) => PERMISSIONS.find((x) => x.id === p)?.label ?? p;

/** The agent's face: its initial on its colour. Decorative; its name is always beside it. */
export function AgentAvatar({ agent, size = "md" }: { agent: Pick<Agent, "name" | "color">; size?: "xs" | "sm" | "md" | "lg" }) {
  return (
    <span className={`agent-avatar agent-avatar-${size} agent-${agent.color}`} aria-hidden="true">
      {initial(agent.name)}
    </span>
  );
}

function useNow(every: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), every);
    return () => clearInterval(t);
  }, [every]);
  return now;
}

/** Open the agent's ongoing chat, or its start page when it has none. */
export function openAgent(agent: Agent, state: State, actions: Actions) {
  const chat = ongoing(agent, state.sessions);
  actions.go(chat ? { page: "session", id: chat.id } : { page: "agent", id: agent.id });
}

function ChatList({ chats, actions, now }: { chats: SessionMeta[]; actions: Actions; now: number }) {
  return (
    <ul className="automation-runs" aria-label="Chats, newest first">
      {chats.map((s) => (
        <li key={s.id} className="automation-run-item">
          <button className="btn-quiet automation-run" title={s.title} onClick={() => actions.go({ page: "session", id: s.id })}>
            <span className={`mark mark-${s.status === "running" ? "working" : s.lastTurn === "failed" ? "failed" : "done"}`} aria-hidden="true" />
            <span className="automation-run-when">{s.title}</span>
            <span className="automation-run-time">{duration(Math.max(0, now - s.updatedAt))} ago</span>
          </button>
        </li>
      ))}
    </ul>
  );
}

function AgentCard({ state, actions, agent, now }: { state: State; actions: Actions; agent: Agent; now: number }) {
  const [open, setOpen] = useState(false);
  const chats = useMemo(() => chatsOf(state.sessions, agent.id), [state.sessions, agent.id]);
  const status = agentStatus(agent, state.sessions, now);
  const body = (
    <>
      <span className={`mark mark-${MARK[status.tone]}`} aria-hidden="true">
        {status.tone === "approval" && <span className="mark-bang">!</span>}
      </span>
      <span className="automation-status-text">
        {status.text}
        {status.sessionId && state.sessions[status.sessionId] && <span className="muted"> · {state.sessions[status.sessionId].title}</span>}
      </span>
    </>
  );
  return (
    <section className="automation agent-card" aria-label={agent.name}>
      <div className="agent-card-head">
        <AgentAvatar agent={agent} />
        <div className="agent-card-name">
          <strong className="automation-name" title={agent.name}>
            {agent.name}
          </strong>
          <span className="muted small">{agent.builtin ? `Built in · reads ${scopeLabel(agent.scope, (h) => hostName(state, h)).toLowerCase()}` : `@${agent.name}`}</span>
        </div>
      </div>
      {agent.about && <p className="agent-about small">{agent.about}</p>}
      <div className="automation-when muted small">{runsOn(agent, modelsOn(state, "local"), agent.builtin ? "Read-only" : permissionLabel(agent.permission))}</div>
      {status.sessionId ? (
        <button className={`btn-quiet automation-status status-${status.tone}`} title="Open the chat" onClick={() => actions.go({ page: "session", id: status.sessionId! })}>
          {body}
        </button>
      ) : (
        <div className={`automation-status status-${status.tone}`}>{body}</div>
      )}
      <div className="automation-foot">
        <button className="btn agent-message" onClick={() => openAgent(agent, state, actions)}>
          {agent.builtin ? "Ask" : "Message"}
        </button>
        <span className="spacer" />
        <button className="btn-icon" aria-label={`New chat with ${agent.name}`} title="New chat" onClick={() => actions.go({ page: "agent", id: agent.id })}>
          <Icon d={PLUS} size={14} />
        </button>
        {chats.length > 0 && (
          <button className="btn-icon" aria-label={`Chats with ${agent.name}`} aria-expanded={open} title={open ? "Hide the chats" : `All ${chats.length} ${chats.length === 1 ? "chat" : "chats"}`} onClick={() => setOpen(!open)}>
            <Icon d={LIST} size={14} />
          </button>
        )}
        <button className="btn-icon" aria-label={`Edit: ${agent.name}`} title="Edit" onClick={() => actions.go({ page: "agentEdit", id: agent.id })}>
          <Icon d={PENCIL} size={14} />
        </button>
      </div>
      {open && chats.length > 0 && <ChatList chats={chats} actions={actions} now={now} />}
    </section>
  );
}

export function AgentsPage({ state, actions }: { state: State; actions: Actions }) {
  const now = useNow(30_000);
  const custom = state.agents.filter((a) => !a.builtin);
  return (
    <div className="hosts automations agents">
      <header className="bar">
        <MenuButton />
        <h1>Agents</h1>
        <span className="spacer" />
        <button className="btn" onClick={() => actions.go({ page: "agentEdit", id: "new" })}>
          <Icon d={PLUS} size={13} />
          New agent
        </button>
      </header>
      <div className="scroll">
        <div className="hosts-inner automation-list">
          <div className="automation-grid">
            {state.agents.map((a) => (
              <AgentCard key={a.id} state={state} actions={actions} agent={a} now={now} />
            ))}
            {custom.length === 0 && state.connected && (
              <button className="host-empty agent-empty" onClick={() => actions.go({ page: "agentEdit", id: "new" })}>
                <Icon d={PLUS} size={16} />
                <span>
                  <strong>Make an agent of your own</strong>
                  <span className="muted small">A name, a model and notes on who it is and what to remember. Type @ and its name in any message to reach it.</span>
                </span>
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/** A new chat with an agent: its face, what it is for, and the box. */
export function AgentStart({ state, actions, agent }: { state: State; actions: Actions; agent: Agent }) {
  const [error, setError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const chat = ongoing(agent, state.sessions);
  const target = useMemo<UploadTarget>(() => ({}), []);
  const send = async (text: string, attachments: Attachment[]) => {
    // A message that names another agent first goes to that one.
    const routed = leadingMention(text, state.agents);
    const to = routed?.agent ?? agent;
    const body = routed ? routed.rest : text;
    if (!body && !attachments.length) return false;
    setSending(true);
    setError(null);
    const r = await actions.askAgent(to.id, body, attachments.map((a) => a.id), to.id === agent.id);
    setSending(false);
    if (r.error) {
      setError(r.error);
      return false;
    }
  };
  const where = agent.builtin ? scopeLabel(agent.scope, (h) => hostName(state, h)) : agent.cwd ? tilde(agent.cwd, homeOn(state, "local")) : "Its own folder";
  return (
    <>
      <header className="bar new-bar">
        <MenuButton />
        <h2 className="label-narrow">{agent.name}</h2>
        <span className="spacer" />
        {chat && (
          <button className="btn-text" onClick={() => actions.go({ page: "session", id: chat.id })}>
            Current chat
          </button>
        )}
        <button className="btn-text" onClick={() => actions.go({ page: "agentEdit", id: agent.id })}>
          Edit
        </button>
      </header>
      <div className="new agent-start">
        <div className="new-inner">
          <div className="agent-start-face">
            <AgentAvatar agent={agent} size="lg" />
          </div>
          <h1>{agent.builtin ? "What would you like to know?" : `New chat with ${agent.name}`}</h1>
          {agent.about && <p className="agent-start-about muted">{agent.about}</p>}
          <Composer
            placeholder={agent.builtin ? `Ask ${agent.name} about your servers or your work` : `Message ${agent.name}`}
            rows={1}
            large
            draft={`agent:${agent.id}`}
            target={target}
            upload={agent.builtin ? undefined : actions.upload}
            busy={sending || !state.connected}
            onSubmit={send}
            mentions={state.agents}
          >
            <span className="muted small agent-runs-chip" title="Set in the agent's settings">
              {runsOn(agent, modelsOn(state, "local"), agent.builtin ? "Read-only" : permissionLabel(agent.permission))}
            </span>
          </Composer>
          <div className="new-chips">
            <span className="muted small">{agent.builtin ? `Reads ${where.toLowerCase()}; runs no tools` : `Works in ${where}`}</span>
            <span className="spacer" />
            {chat && <span className="muted small">Starts a new chat; the current one stays</span>}
          </div>
          {error && (
            <div className="small automation-message" role="alert">
              {error}
            </div>
          )}
          {agent.builtin && (
            <div className="agent-suggestions" role="group" aria-label="Suggested questions">
              {HOST_QUESTIONS.map((q) => (
                <button key={q} className="btn-quiet" disabled={sending || !state.connected} onClick={() => void send(q, [])}>
                  <span>{q}</span>
                  <span aria-hidden="true">↗</span>
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
    </>
  );
}

export function AgentForm({ state, actions, id }: { state: State; actions: Actions; id: string }) {
  const existing = id === "new" ? undefined : state.agents.find((a) => a.id === id);
  const builtin = !!existing?.builtin;
  const models = modelsOn(state, "local");
  const home = homeOn(state, "local");
  const workspace = state.workspace.local || home;
  const [name, setName] = useState(existing?.name ?? "");
  const [color, setColor] = useState<AgentColor>(existing?.color ?? AGENT_COLORS[state.agents.length % AGENT_COLORS.length]);
  const [about, setAbout] = useState(existing?.about ?? "");
  const [instructions, setInstructions] = useState(existing?.instructions ?? "");
  const [memory, setMemory] = useState(existing?.memory ?? "");
  const [cwd, setCwd] = useState(existing?.cwd ?? "");
  const [scope, setScope] = useState(existing?.scope ?? "all");
  const [permission, setPermission] = useState<Permission>(existing?.permission ?? "ask");
  const [choice, setChoice] = useState(() =>
    existing?.model ? { engine: existing.engine, model: existing.model, effort: existing.effort } : loadChoice(models),
  );
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  // Models arrive after the first render; an agent with no model of its own
  // shows the host's default until one is picked.
  useEffect(() => {
    if (existing?.model) return;
    if (!models[choice.engine]?.some((m) => m.id === choice.model)) {
      const engine = builtin ? (existing?.engine ?? "codex") : choice.engine;
      const list = models[engine] ?? [];
      const first = list.find((m) => m.isDefault) ?? list[0];
      setChoice(first ? { engine, model: first.id, effort: first.defaultEffort ?? first.efforts[0] ?? "" } : loadChoice(models));
    }
  }, [models]);
  const engines = pickerEngines(models, choice.engine).filter((e) => !builtin || e === "codex" || e === "claude");
  const recent = useMemo(() => {
    const fromSessions = Object.values(state.sessions)
      .filter((s) => !s.host || s.host === "local")
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .map((s) => s.repo || s.cwd);
    return [...new Set([...readRecent("local"), ...fromSessions])].slice(0, 12);
  }, [state.sessions]);
  const problem = nameProblem(name, state.agents, existing?.id) ?? (!choice.model ? "Pick a model." : null);
  const save = async () => {
    setSaving(true);
    setError(null);
    const r = await actions.saveAgent({
      ...(existing ? { id: existing.id } : {}),
      name: name.trim(),
      color,
      about,
      instructions,
      memory,
      engine: choice.engine,
      model: choice.model,
      effort: effortFor(models[choice.engine]?.find((m) => m.id === choice.model), choice.effort),
      ...(builtin ? { scope } : { cwd, permission }),
    });
    setSaving(false);
    if (r.error) setError(r.error);
    else actions.go({ page: "agents" });
  };
  return (
    <div className="hosts automations agents">
      <header className="bar">
        <MenuButton />
        <h1>{existing ? `Edit ${existing.name}` : "New agent"}</h1>
      </header>
      <div className="scroll">
        <div className="hosts-inner automation-form">
          <div className="agent-form-identity">
            <AgentAvatar agent={{ name: name || "?", color }} size="lg" />
            <div className="automation-field agent-form-name">
              <label htmlFor="agent-name">Name</label>
              <input id="agent-name" className="field" value={name} maxLength={24} onChange={(e) => setName(e.target.value)} placeholder="Reviewer" autoFocus={!existing} />
              <span className="muted small">{name.trim() && !nameProblem(name, state.agents, existing?.id) ? `Mention it as @${name.trim()} in any message.` : "One word, so it can be @mentioned."}</span>
            </div>
          </div>
          <div className="automation-field">
            <span className="automation-label">Colour</span>
            <div className="row agent-colors" role="group" aria-label="Colour">
              {AGENT_COLORS.map((c) => (
                <button key={c} className="btn-icon agent-swatch" aria-label={`Colour: ${c}`} aria-pressed={color === c} title={c} onClick={() => setColor(c)}>
                  <span className={`agent-swatch-dot agent-${c}`} />
                </button>
              ))}
            </div>
          </div>
          <div className="automation-field">
            <label htmlFor="agent-about">What it is for</label>
            <input id="agent-about" className="field" value={about} maxLength={160} onChange={(e) => setAbout(e.target.value)} placeholder="Reviews my pull requests" />
          </div>
          <div className="automation-field">
            <label htmlFor="agent-instructions">Instructions</label>
            <textarea id="agent-instructions" className="field automation-prompt" rows={5} value={instructions} onChange={(e) => setInstructions(e.target.value)} placeholder="Who it is and how it works: tone, what to check, what never to do" />
            <span className="muted small">Given at the start of every chat.</span>
          </div>
          <div className="automation-field">
            <label htmlFor="agent-memory">Memory</label>
            <textarea id="agent-memory" className="field automation-prompt" rows={4} value={memory} onChange={(e) => setMemory(e.target.value)} placeholder="Facts to keep in mind: names, conventions, where things live" />
            <span className="muted small">Notes it carries into every chat. A chat already going keeps the notes it started with.</span>
          </div>
          <div className="automation-field">
            <span className="automation-label">Where and how</span>
            <div className="new-chips">
              {builtin ? (
                <HostPicker state={state} value={scope} contextScope onChange={setScope} onAddHost={() => actions.go({ page: "hosts" })} />
              ) : (
                <>
                  <FolderPicker host="local" home={home} workspace={workspace} value={cwd} recent={recent} dirs={state.dirs} listDirs={actions.listDirs} onChange={setCwd} />
                  {cwd && (
                    <button className="btn-text" onClick={() => setCwd("")}>
                      Use its own folder
                    </button>
                  )}
                </>
              )}
            </div>
            <div className="automation-pickers">
              <ModelPicker models={models} value={choice} engines={engines} onChange={setChoice} />
              <EffortPicker models={models} value={choice} onChange={(effort) => setChoice({ ...choice, effort })} />
              {!builtin && <PermissionPicker engine={choice.engine} value={permission} onChange={setPermission} />}
            </div>
            <span className="muted small">
              {builtin
                ? `Reads ${scopeLabel(scope, (h) => hostName(state, h)).toLowerCase()} for each question and answers without tools: it cannot change anything. Runs on ${ENGINE_NAME[choice.engine]} on ${hostName(state, "local")}.`
                : cwd
                  ? `Its chats work in ${tilde(cwd, home)}, one writer at a time.`
                  : `Its chats work in a folder of its own on ${hostName(state, "local")}.`}
              {!builtin && permission === "full" && " Full access: it runs anything without asking."}
            </span>
          </div>
          {(error || (name && problem)) && (
            <div className="small automation-reason" role="alert">
              {error ?? problem}
            </div>
          )}
          <div className="row">
            <button type="button" className="btn-primary" disabled={!!problem || saving || !state.connected || !hostOnline(state, "local")} onClick={() => void save()}>
              {saving ? "Saving" : existing ? "Save agent" : "Create agent"}
            </button>
            <button type="button" className="btn-text" onClick={() => actions.go({ page: "agents" })}>
              Cancel
            </button>
            <span className="spacer" />
            {existing && !builtin && !confirmDelete && (
              <button type="button" className="btn-text danger" onClick={() => setConfirmDelete(true)}>
                Delete agent
              </button>
            )}
            {existing && confirmDelete && (
              <>
                <span className="muted small">Its chats stay as sessions.</span>
                <button
                  type="button"
                  className="btn danger"
                  onClick={() => {
                    actions.deleteAgent(existing.id);
                    actions.go({ page: "agents" });
                  }}
                >
                  Confirm delete
                </button>
                <button type="button" className="btn-text" onClick={() => setConfirmDelete(false)}>
                  Keep it
                </button>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

