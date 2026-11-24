// Agents: the page of cards, an agent's conversation before its first message,
// and the create/edit form. Each agent has one conversation, an ordinary
// session tagged with it (yardd's agents.rs), kept out of the sidebar: this
// page and the Agents nav row show its state.
import { useEffect, useMemo, useRef, useState } from "react";
import type { Agent, AgentColor, AgentFace, Attachment, Engine, Permission } from "./protocol";
import { accountsOn, hostName, hostOf, hostOnline, homeOn, modelsOn, type Actions, type State, type UploadTarget } from "./store";
import { AGENT_COLORS, AGENT_FACES, FACE_PATHS, agentHost, agentStatus, agentWhere, conversationOf, faceOf, leadingMention, moveNote, nameProblem, runsOn, scopeLabel, type AgentTone } from "./agents";
import { DEFAULT_ACCOUNT, accountChoices, accountLabel, choiceOnEngine } from "./accounts";
import { ENGINE_NAME, pickerEngines } from "./engines";
import { useSeen } from "./rowState";
import {
  AccountPicker,
  Cloud,
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
const MARK: Record<AgentTone, string> = { live: "working", approval: "approval", needsYou: "approval", answered: "answered", ok: "done", bad: "failed", quiet: "idle", offline: "idle" };
const HOST_QUESTIONS = ["What’s happening on my server?", "What changed since yesterday?", "Where is the API deployed?"];

export const permissionLabel = (p: Permission) => PERMISSIONS.find((x) => x.id === p)?.label ?? p;

const AVATAR_PX = { xs: 14, sm: 20, md: 32, lg: 48, xl: 88 } as const;
// Small avatars draw thicker lines, so the eyes stay visible at 14px.
const FACE_STROKE = { xs: 2.6, sm: 2.3, md: 2, lg: 1.8, xl: 1.6 } as const;

/** The agent's face, drawn on its colour. Decorative; its name is always beside it. */
export function AgentAvatar({ agent, size = "md" }: { agent: Pick<Agent, "color"> & Partial<Pick<Agent, "face" | "id" | "builtin">>; size?: keyof typeof AVATAR_PX }) {
  const face = faceOf({ face: agent.face ?? "", id: agent.id ?? "", builtin: !!agent.builtin });
  return (
    <span className={`agent-avatar agent-avatar-${size} agent-${agent.color}`} aria-hidden="true">
      <Icon d={FACE_PATHS[face]} size={AVATAR_PX[size]} width={FACE_STROKE[size]} />
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

/** An agent's place: its conversation, or the empty one before its first message. */
export const openAgent = (agent: Agent, actions: Actions) => actions.go({ page: "agent", id: agent.id });

/** The name of the agent's host while it cannot be reached. */
const offlineHost = (state: State, agent: Agent): string | undefined => {
  const host = agentHost(agent);
  return host !== "local" && !hostOnline(state, host) ? hostName(state, host) : undefined;
};

/** "Fake Fast · Work · Low · Ask first": the account only where the engine has a choice. */
const howItRuns = (state: State, agent: Agent) =>
  runsOn(agent, modelsOn(state, agentHost(agent)), agent.builtin ? "Read-only" : permissionLabel(agent.permission), accountLabel(accountsOn(state, agentHost(agent)), agent));

/** How it runs, after the host it runs on when that is not this one. */
function RunsLine({ state, agent, className }: { state: State; agent: Agent; className: string }) {
  const host = agentHost(agent);
  const text = howItRuns(state, agent);
  if (host === "local") return <div className={className}>{text}</div>;
  return (
    <div className={`${className} agent-runs-remote`} title={`Runs on ${hostName(state, host)}`}>
      <Cloud size={12} />
      <span>
        {hostName(state, host)} · {text}
      </span>
    </div>
  );
}

/** A note per conversation the agent left on a host it moved away from,
 *  opening it (read-only there). Newest first. */
export function EarlierConversations({ state, actions, agent }: { state: State; actions: Actions; agent: Agent }) {
  const earlier = [...(agent.previous ?? [])].reverse().filter((id) => state.sessions[id]);
  if (!earlier.length) return null;
  return (
    <>
      {earlier.map((id) => (
        <div key={id} className="note agent-earlier">
          Earlier messages with {agent.name} are on {hostName(state, hostOf(state.sessions[id]))}.
          <button className="btn-text note-link" onClick={() => actions.go({ page: "session", id })}>
            Open them
          </button>
        </div>
      ))}
    </>
  );
}

function AgentCard({ state, actions, agent, now, seenAt }: { state: State; actions: Actions; agent: Agent; now: number; seenAt(id: string): number }) {
  const chat = conversationOf(agent, state.sessions);
  const status = agentStatus(agent, state.sessions, now, chat ? seenAt(chat.id) : Infinity, offlineHost(state, agent));
  return (
    <section className="automation agent-card" aria-label={agent.name}>
      <button className="btn-quiet agent-card-head" title={`Open ${agent.name}'s conversation`} onClick={() => openAgent(agent, actions)}>
        <AgentAvatar agent={agent} />
        <span className="agent-card-name">
          <strong className="automation-name">{agent.name}</strong>
          <span className="muted small">{agent.builtin ? `Built in · reads ${scopeLabel(agent.scope, (h) => hostName(state, h)).toLowerCase()}` : `@${agent.name}`}</span>
        </span>
      </button>
      {agent.about && <p className="agent-about small">{agent.about}</p>}
      <RunsLine state={state} agent={agent} className="automation-when muted small" />
      <div className={`automation-status status-${status.tone}`} title={status.text}>
        <span className={`mark mark-${MARK[status.tone]}`} aria-hidden="true">
          {(status.tone === "approval" || status.tone === "needsYou") && <span className="mark-bang">!</span>}
        </span>
        <span className="automation-status-text">{status.text}</span>
      </div>
      <div className="automation-foot">
        <button className="btn agent-message" onClick={() => openAgent(agent, actions)}>
          {agent.builtin ? "Ask" : "Message"}
        </button>
        <span className="spacer" />
        <button className="btn-icon" aria-label={`Edit: ${agent.name}`} title="Edit" onClick={() => actions.go({ page: "agentEdit", id: agent.id })}>
          <Icon d={PENCIL} size={14} />
        </button>
      </div>
    </section>
  );
}

export function AgentsPage({ state, actions }: { state: State; actions: Actions }) {
  const now = useNow(30_000);
  const seen = useSeen(state.sessions, null);
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
              <AgentCard key={a.id} state={state} actions={actions} agent={a} now={now} seenAt={seen.seenAt} />
            ))}
            {custom.length === 0 && state.connected && (
              <button className="btn-quiet host-empty agent-empty" onClick={() => actions.go({ page: "agentEdit", id: "new" })}>
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

/** The agent's header in its conversation and before it: face, name, what it
 *  is for, Edit. */
/** `host`: where the conversation shown runs, when not the agent's own host
 *  (one it left behind when it moved). */
export function AgentBar({ state, agent, actions, host = agentHost(agent), children }: { state: State; agent: Agent; actions: Actions; host?: string; children?: React.ReactNode }) {
  return (
    <header className="bar agent-bar">
      <MenuButton />
      <AgentAvatar agent={agent} size="sm" />
      <h1>{agent.name}</h1>
      {agent.about && <span className="muted small agent-bar-about">{agent.about}</span>}
      {host !== "local" && (
        <span className="muted small bar-host">
          <Cloud size={13} />
          {hostName(state, host)}
          {!hostOnline(state, host) && " · offline"}
        </span>
      )}
      <span className="spacer" />
      {children}
      <button className="btn-text" onClick={() => actions.go({ page: "agentEdit", id: agent.id })}>
        Edit
      </button>
    </header>
  );
}

/** An agent's conversation before its first message: its face, what it is
 *  for, and the box. */
export function AgentStart({ state, actions, agent, earlier }: { state: State; actions: Actions; agent: Agent; earlier?: React.ReactNode }) {
  const [error, setError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const host = agentHost(agent);
  const online = hostOnline(state, host);
  // Its files go to the host its conversation will run on.
  const target = useMemo<UploadTarget>(() => (host === "local" ? {} : { host }), [host]);
  const send = async (text: string, attachments: Attachment[]) => {
    // A message that names another agent first goes to that one.
    const routed = leadingMention(text, state.agents);
    const to = routed?.agent ?? agent;
    const body = routed ? routed.rest : text;
    if (!body && !attachments.length) return false;
    setSending(true);
    setError(null);
    const r = await actions.askAgent(to.id, body, attachments.map((a) => a.id));
    setSending(false);
    if (r.error) {
      setError(r.error);
      return false;
    }
  };
  const where = agent.builtin ? scopeLabel(agent.scope, (h) => hostName(state, h)) : agentWhere(agent, (h) => hostName(state, h), tilde(agent.cwd, homeOn(state, host)));
  return (
    <div className="agent-page">
      <AgentBar state={state} agent={agent} actions={actions} />
      <div className="new agent-start">
        <div className="new-inner">
          <div className="agent-start-face">
            <AgentAvatar agent={agent} size="lg" />
          </div>
          <h1>{agent.builtin ? "What would you like to know?" : `Message ${agent.name}`}</h1>
          {agent.about && <p className="agent-start-about muted">{agent.about}</p>}
          {earlier}
          <Composer
            placeholder={agent.builtin ? `Ask ${agent.name} about your servers or your work` : `Message ${agent.name}`}
            rows={1}
            large
            draft={`agent:${agent.id}`}
            target={target}
            upload={agent.builtin ? undefined : actions.upload}
            busy={sending || !online}
            onSubmit={send}
            mentions={state.agents}
            mentionHint={(other) => (other.id === agent.id ? null : `Goes to ${other.name}`)}
          >
            <span className="muted small agent-runs-chip" title="Set in the agent's settings">
              {howItRuns(state, agent)}
            </span>
          </Composer>
          <div className="new-chips">
            <span className="muted small">{agent.builtin ? `Reads ${where.toLowerCase()}; runs no tools` : `Works in ${where}`}</span>
            {/* Send is disabled until its host is back; say why, as New session does. */}
            {!online && host !== "local" && (
              <>
                <span className="spacer" />
                <span className="muted small agent-start-offline" role="status">
                  {hostName(state, host)} is offline
                </span>
              </>
            )}
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
    </div>
  );
}

export function AgentForm({ state, actions, id }: { state: State; actions: Actions; id: string }) {
  const existing = id === "new" ? undefined : state.agents.find((a) => a.id === id);
  const builtin = !!existing?.builtin;
  // Where it runs. Scout reads every host from this one and never moves.
  const [host, setHost] = useState(() => (existing && !builtin ? agentHost(existing) : "local"));
  const models = modelsOn(state, host);
  const home = homeOn(state, host);
  const workspace = state.workspace[host] || home;
  const online = hostOnline(state, host);
  const [name, setName] = useState(existing?.name ?? "");
  const [color, setColor] = useState<AgentColor>(existing?.color ?? AGENT_COLORS[state.agents.length % AGENT_COLORS.length]);
  // A new agent's face steps differently from its colour, so the pairs vary,
  // and skips Scout's glasses.
  const [face, setFace] = useState<AgentFace>(() => {
    if (existing) return faceOf(existing);
    const pool = AGENT_FACES.filter((f) => f !== "glasses");
    return pool[(state.agents.length * 3) % pool.length];
  });
  const [about, setAbout] = useState(existing?.about ?? "");
  const [instructions, setInstructions] = useState(existing?.instructions ?? "");
  const [memory, setMemory] = useState(existing?.memory ?? "");
  // The agent edits its own memory (remember, set_memory): an open form
  // follows it until the user types there, and leaves it alone on save.
  const liveMemory = existing?.memory ?? "";
  const memoryBase = useRef(liveMemory);
  useEffect(() => {
    if (memory === memoryBase.current) setMemory(liveMemory);
    memoryBase.current = liveMemory;
  }, [liveMemory]);
  const [cwd, setCwd] = useState(existing?.cwd ?? "");
  const [scope, setScope] = useState(existing?.scope ?? "all");
  const [permission, setPermission] = useState<Permission>(existing?.permission ?? "ask");
  const [choice, setChoice] = useState(() =>
    existing?.model ? { engine: existing.engine, model: existing.model, effort: existing.effort } : loadChoice(models),
  );
  const [picked, setPicked] = useState(existing?.account ?? DEFAULT_ACCOUNT);
  // The account is the host's and the engine's: one that is not there runs on Default.
  const accounts = accountsOn(state, host);
  const account = accountChoices(accounts, choice.engine).some((a) => a.id === picked) ? picked : DEFAULT_ACCOUNT;
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  // Models arrive after the first render and differ per host. An agent with
  // no model of its own shows the host's default until one is picked; one
  // whose model the host does not list falls back to that default too.
  useEffect(() => {
    const listed = models[choice.engine] ?? [];
    if (listed.some((m) => m.id === choice.model)) return;
    if (existing?.model && !listed.length && host === agentHost(existing)) return;
    const engine = builtin ? (existing?.engine ?? "codex") : choice.engine;
    const list = models[engine] ?? [];
    const first = list.find((m) => m.isDefault) ?? list[0];
    setChoice(first ? { engine, model: first.id, effort: first.defaultEffort ?? first.efforts[0] ?? "" } : loadChoice(models));
  }, [models]);
  const engines = pickerEngines(models, choice.engine).filter((e) => !builtin || e === "codex" || e === "claude");
  const recent = useMemo(() => {
    const fromSessions = Object.values(state.sessions)
      .filter((s) => hostOf(s) === host)
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .map((s) => s.repo || s.cwd);
    return [...new Set([...readRecent(host), ...fromSessions])].slice(0, 12);
  }, [state.sessions, host]);
  const pickHost = (next: string) => {
    if (next === host) return;
    setHost(next);
    // A folder on one host means nothing on another.
    setCwd("");
  };
  // Picking an account under another engine means running on it, as on New session.
  const pickAccount = (engine: Engine, accountId: string) => {
    setPicked(accountId);
    const next = engine === choice.engine ? null : choiceOnEngine(models, engine, choice.effort);
    if (next) setChoice(next);
  };
  // A typed name's problem shows by the field: the foot of the form is below the fold.
  const nameIssue = name ? nameProblem(name, state.agents, existing?.id) : null;
  const problem = nameProblem(name, state.agents, existing?.id) ?? (!choice.model ? "Pick a model." : null);
  const save = async () => {
    setSaving(true);
    setError(null);
    const r = await actions.saveAgent({
      ...(existing ? { id: existing.id } : {}),
      name: name.trim(),
      color,
      face,
      about,
      instructions,
      ...(!existing || memory !== memoryBase.current ? { memory } : {}),
      engine: choice.engine,
      model: choice.model,
      effort: effortFor(models[choice.engine]?.find((m) => m.id === choice.model), choice.effort),
      account,
      ...(builtin ? { scope } : { cwd, permission, host }),
    });
    setSaving(false);
    if (r.error) setError(r.error);
    else actions.go({ page: "agents" });
  };
  const remove = async () => {
    setError(null);
    const r = await actions.deleteAgent(existing!.id);
    // Its conversation on an offline host keeps it here, said in the form.
    if (r.error) {
      setError(r.error);
      setConfirmDelete(false);
    } else actions.go({ page: "agents" });
  };
  const deleteControls = existing && !builtin && (
    <div className="row agent-form-delete">
      {confirmDelete ? (
        <>
          <span className="muted small">Its conversation is deleted too.</span>
          <button type="button" className="btn danger" onClick={() => void remove()}>
            Confirm delete
          </button>
          <button type="button" className="btn-text" onClick={() => setConfirmDelete(false)}>
            Keep it
          </button>
        </>
      ) : (
        <button type="button" className="btn-text danger" onClick={() => setConfirmDelete(true)}>
          Delete agent
        </button>
      )}
    </div>
  );
  const named = (h: string) => hostName(state, h);
  const elsewhere = host !== "local" ? ` on ${named(host)}` : "";
  const moving = builtin ? null : moveNote(existing, host, named);
  return (
    <div className="hosts automations agents">
      <header className="bar">
        <MenuButton />
        <h1>{existing ? `Edit ${existing.name}` : "New agent"}</h1>
      </header>
      <div className="scroll">
        <div className="hosts-inner automation-form agent-form">
          <div className="agent-form-hero">
            <AgentAvatar agent={{ color, face }} size="xl" />
            <div className="agent-form-name">
              <label htmlFor="agent-name" className="sr-only">
                Name
              </label>
              <input id="agent-name" className="field agent-name-input" value={name} maxLength={24} onChange={(e) => setName(e.target.value)} placeholder="Name it" autoFocus={!existing} />
              {nameIssue ? (
                <span className="small automation-reason" role="alert">
                  {nameIssue}
                </span>
              ) : (
                <span className="muted small">{name.trim() ? `Mention it as @${name.trim()} in any message.` : "One word, so it can be @mentioned."}</span>
              )}
            </div>
          </div>
          <section className="agent-form-card" aria-labelledby="agent-form-look">
            <div className="agent-form-card-head">
              <h2 id="agent-form-look">Look</h2>
              <p className="muted small">How it shows on its card, in mentions and in its conversation.</p>
            </div>
            <div className="automation-field">
              <span className="automation-label">Face</span>
              <div className="agent-faces" role="group" aria-label="Face">
                {AGENT_FACES.map((f) => (
                  <button key={f} type="button" className="btn-icon agent-face-option" aria-label={`Face: ${f}`} aria-pressed={face === f} title={f} onClick={() => setFace(f)}>
                    <AgentAvatar agent={{ color, face: f }} size="lg" />
                  </button>
                ))}
              </div>
            </div>
            <div className="automation-field">
              <span className="automation-label">Colour</span>
              <div className="row agent-colors" role="group" aria-label="Colour">
                {AGENT_COLORS.map((c) => (
                  <button key={c} type="button" className="btn-icon agent-swatch" aria-label={`Colour: ${c}`} aria-pressed={color === c} title={c} onClick={() => setColor(c)}>
                    <span className={`agent-swatch-dot agent-${c}`} />
                  </button>
                ))}
              </div>
            </div>
          </section>
          <section className="agent-form-card" aria-labelledby="agent-form-role">
            <div className="agent-form-card-head">
              <h2 id="agent-form-role">Role</h2>
              <p className="muted small">What it does and what it knows. A change reaches it with its next message.</p>
            </div>
            <div className="automation-field">
              <label htmlFor="agent-about">What it is for</label>
              <input id="agent-about" className="field" value={about} maxLength={160} onChange={(e) => setAbout(e.target.value)} placeholder="Reviews my pull requests" />
            </div>
            <div className="automation-field">
              <label htmlFor="agent-instructions">Instructions</label>
              <textarea id="agent-instructions" className="field automation-prompt" rows={5} value={instructions} onChange={(e) => setInstructions(e.target.value)} placeholder="Who it is and how it works: tone, what to check, what never to do" />
              <span className="muted small">Given at the start of its conversation.</span>
            </div>
            <div className="automation-field">
              <label htmlFor="agent-memory">Memory</label>
              <textarea id="agent-memory" className="field automation-prompt" rows={4} value={memory} onChange={(e) => setMemory(e.target.value)} placeholder="Facts to keep in mind: names, conventions, where things live" />
              <span className="muted small">Facts it keeps in mind.</span>
            </div>
          </section>
          <section className="agent-form-card" aria-labelledby="agent-form-where">
            <div className="agent-form-card-head">
              <h2 id="agent-form-where">Where and how</h2>
              <p className="muted small">
                {builtin
                  ? `Reads ${scopeLabel(scope, named).toLowerCase()} for each question and answers without tools: it cannot change anything. Runs on ${ENGINE_NAME[choice.engine]} on ${named("local")}.`
                  : cwd
                    ? `It works in ${tilde(cwd, home)}${elsewhere}, one writer at a time.`
                    : `It works in a folder of its own on ${named(host)}.`}
                {!builtin && permission === "full" && " Full access: it runs anything without asking."}
                {moving && ` ${moving}`}
                {!online && host !== "local" && ` ${named(host)} is offline: changes can be saved once it is back.`}
              </p>
            </div>
            <div className="agent-form-where">
              <div className="new-chips">
                {builtin ? (
                  <HostPicker state={state} value={scope} contextScope onChange={setScope} onAddHost={() => actions.go({ page: "hosts" })} />
                ) : (
                  <>
                    <HostPicker state={state} value={host} onChange={pickHost} onAddHost={() => actions.go({ page: "hosts" })} />
                    <FolderPicker host={host} home={home} workspace={workspace} value={cwd} recent={recent} dirs={state.dirs} listDirs={actions.listDirs} onChange={setCwd} placeholder="Its own folder" />
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
                {accountChoices(accounts, choice.engine).length > 1 && (
                  <AccountPicker state={state} host={host} engine={choice.engine} account={account} engines={engines} onPick={pickAccount} onManage={() => actions.go({ page: "hosts", host, tab: "engines" })} />
                )}
                <EffortPicker models={models} value={choice} onChange={(effort) => setChoice({ ...choice, effort })} />
                {!builtin && <PermissionPicker engine={choice.engine} value={permission} onChange={setPermission} />}
              </div>
            </div>
          </section>
          {(error || (name && problem && !nameIssue)) && (
            <div className="small automation-reason" role="alert">
              {error ?? problem}
            </div>
          )}
          <div className="agent-form-foot">
            {deleteControls}
            <span className="spacer" />
            <button type="button" className="btn-text" onClick={() => actions.go({ page: "agents" })}>
              Cancel
            </button>
            <button type="button" className="btn-primary" disabled={!!problem || saving || !state.connected || !online} onClick={() => void save()}>
              {saving ? "Saving" : existing ? "Save agent" : "Create agent"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
