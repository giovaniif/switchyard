// Agents: the page of cards, an agent's conversation before its first message,
// what it asks the owner for, and the edit form. Each agent has one
// conversation, an ordinary session tagged with it (yardd's agents.rs), kept
// out of the sidebar: this page and the Agents nav row show its state. A new
// agent is made with a placeholder name and set up by talking to it.
import { createElement, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { Agent, AgentColor, AgentFace, AgentRequest, Attachment, Engine, Permission } from "./protocol";
import { accountsOn, hostName, hostOf, hostOnline, homeOn, modelsOn, type Actions, type State, type UploadTarget } from "./store";
import { AGENT_COLORS, AGENT_NAME_MAX, agentChoice, agentHost, agentStatus, agentWhere, cleanName, conversationOf, handleFor, handleOf, hostChange, leadingMention, moveNote, nameProblem, runsOn, type AgentTone } from "./agents";
import { AGENT_FACES, blinkDelay, detailFor, faceOf, faceTree, type SvgNode } from "./faces";
import { DEFAULT_ACCOUNT, accountChoices, accountLabel, accountName, choiceOnEngine } from "./accounts";
import { pickerEngines, shownEffort } from "./engines";
import { AnchoredPopover } from "./AnchoredPopover";
import { useSeen } from "./rowState";
import { SLACK_MARK, SlackSetup } from "./SlackSetup";
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
  isAgentHome,
  modelName,
  readRecent,
  setupSettings,
  tilde,
} from "./App";

const PENCIL = "M4 20h4L19 9l-4-4L4 16ZM13.5 6.5l4 4";
const MARK: Record<AgentTone, string> = { live: "working", approval: "approval", needsYou: "approval", answered: "answered", ok: "done", bad: "failed", quiet: "idle", offline: "idle" };
/** What a new agent could be for, to start its first message. */
const FIRST_IDEAS = ["Review my pull requests and say what to fix", "Watch my deploys and tell me in Slack when one fails", "Answer questions about this codebase"];

export const permissionLabel = (p: Permission) => PERMISSIONS.find((x) => x.id === p)?.label ?? p;

const AVATAR_PX = { xs: 14, sm: 20, md: 32, lg: 48, xl: 88 } as const;

/** The face's SVG description as React elements (SVG attribute names to React's). */
function svgElement(n: SvgNode, key = 0): React.ReactElement {
  const props: Record<string, string | number> = { key };
  for (const [k, v] of Object.entries(n.attrs)) props[k === "class" ? "className" : k.replace(/-(\w)/g, (_, c: string) => c.toUpperCase())] = v;
  return createElement(n.tag, props, ...n.children.map((c, i) => svgElement(c, i)));
}

// Big faces follow the pointer when it comes near: one listener for all of
// them, read once per frame, so a page of faces costs nothing while it is still.
const NEAR_PX = 260;
const followers = new Set<HTMLElement>();
let pointer: { x: number; y: number } | null = null;
let frame = 0;
function follow() {
  frame = 0;
  for (const el of followers) {
    const r = el.getBoundingClientRect();
    const dx = pointer ? pointer.x - (r.left + r.width / 2) : 0;
    const dy = pointer ? pointer.y - (r.top + r.height / 2) : 0;
    const d = Math.hypot(dx, dy);
    const near = !!pointer && d < NEAR_PX;
    el.classList.toggle("is-following", near);
    if (!near) continue;
    // A full look once the pointer is a face-width away; towards it, never past.
    const k = Math.min(1, d / r.width) / (d || 1);
    el.style.setProperty("--look-x", (dx * k).toFixed(2));
    el.style.setProperty("--look-y", (dy * k).toFixed(2));
  }
}
const schedule = () => {
  if (!frame) frame = requestAnimationFrame(follow);
};
const onPointer = (e: PointerEvent) => {
  pointer = { x: e.clientX, y: e.clientY };
  schedule();
};
const onLeave = () => {
  pointer = null;
  schedule();
};
function useFollowPointer(ref: React.RefObject<HTMLElement | null>, on: boolean) {
  useEffect(() => {
    const el = ref.current;
    if (!on || !el || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    if (!followers.size) {
      window.addEventListener("pointermove", onPointer, { passive: true });
      document.documentElement.addEventListener("pointerleave", onLeave);
    }
    followers.add(el);
    return () => {
      followers.delete(el);
      el.classList.remove("is-following");
      if (!followers.size) {
        window.removeEventListener("pointermove", onPointer);
        document.documentElement.removeEventListener("pointerleave", onLeave);
      }
    };
  }, [ref, on]);
}

/** The agent's face: a little 3D character in its colour (faces.ts). It
 *  breathes, blinks, glances and tilts its head now and then; while `busy` it
 *  bounces, looks around and talks; the big ones follow a nearby pointer.
 *  Decorative: its name is always beside it. `still` where motion would
 *  distract (unpicked faces). */
export function AgentAvatar({ agent, size = "md", busy = false, still = false }: { agent: Pick<Agent, "color"> & Partial<Pick<Agent, "face" | "id">>; size?: keyof typeof AVATAR_PX; busy?: boolean; still?: boolean }) {
  const face = faceOf({ face: agent.face ?? "", id: agent.id ?? "" });
  const px = AVATAR_PX[size];
  // Gradient and clip ids must be unique on a page full of faces.
  const uid = "f" + useId().replace(/[^\w-]/g, "");
  const svg = useMemo(() => {
    const tree = faceTree({ color: agent.color, face }, detailFor(px), uid);
    tree.attrs = { ...tree.attrs, width: px, height: px };
    return svgElement(tree);
  }, [agent.color, face, px, uid]);
  const ref = useRef<HTMLSpanElement>(null);
  useFollowPointer(ref, (size === "lg" || size === "xl") && !still);
  // Faces side by side blink at their own moments.
  const style = { "--blink-delay": `${blinkDelay(agent.id ?? agent.color)}s` } as React.CSSProperties;
  return (
    <span ref={ref} className={`agent-avatar agent-avatar-${size}${busy ? " is-busy" : ""}${still ? " is-still" : ""}`} style={style} aria-hidden="true">
      {svg}
    </span>
  );
}

/** One change to an agent, saved at once (a picker, its face): answers why
 *  not, also kept in `error` for the page to show. */
export function useAgentSave(actions: Actions, id: string | undefined) {
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setError(null), [id]);
  const save = async (patch: Partial<Agent>): Promise<string | null> => {
    if (!id) return null;
    setError(null);
    const r = await actions.saveAgent({ id, ...patch });
    if (r.error) setError(r.error);
    return r.error ?? null;
  };
  return { save, error };
}

/** Picking another host for the agent. Before its first message that is all;
 *  after it, its conversation stays where it is and the next message starts
 *  one there, so the pick is said first and confirmed (`confirm`, shown above
 *  the box). */
export function useAgentMove(state: State, agent: Agent | undefined, save: (patch: Partial<Agent>) => Promise<string | null>) {
  const [to, setTo] = useState<string | null>(null);
  useEffect(() => setTo(null), [agent?.id, agent?.host]);
  const change = (host: string) => {
    if (!agent) return;
    const accounts = accountChoices(accountsOn(state, host), agent.engine).map((a) => a.id);
    void save(hostChange(agent, host, modelsOn(state, host), accounts));
  };
  const pick = (host: string) => {
    if (!agent || host === agentHost(agent)) return;
    if (agent.chat) setTo(host);
    else change(host);
  };
  const note = agent && to ? moveNote(agent, to, (h) => hostName(state, h)) : null;
  const confirm = agent && to && (
    <div className="agent-requests" role="group" aria-label={`Move ${agent.name}`}>
      <div className="agent-request agent-move">
        <Cloud size={13} />
        <span className="agent-request-text">{note}</span>
        <span className="agent-request-actions">
          <button className="btn-text" onClick={() => setTo(null)}>
            Keep it here
          </button>
          <button
            className="btn"
            onClick={() => {
              setTo(null);
              change(to);
            }}
          >
            Move to {hostName(state, to)}
          </button>
        </span>
      </div>
    </div>
  );
  return { pick, confirm: confirm || null };
}

/** How the agent runs, as New session's box has it: the model (with its
 *  engine), the account where the engine has a choice, effort and
 *  permission, and the host when `onHost` is given. Each pick saves the
 *  agent at once, with Edit's rules: a new engine, model or account goes on
 *  in a fresh context, refused while a turn runs. */
export function AgentRunPickers({ state, actions, agent, save, onHost, speed }: {
  state: State;
  actions: Actions;
  agent: Agent;
  save(patch: Partial<Agent>): Promise<string | null>;
  onHost?(host: string): void;
  speed?: { value: string; onChange(value: string): void; nextTurn?: boolean };
}) {
  const host = agentHost(agent);
  const models = modelsOn(state, host);
  const choice = agentChoice(agent, models);
  const engines = pickerEngines(models, choice.engine);
  const accounts = accountsOn(state, host);
  const account = accountChoices(accounts, choice.engine).some((a) => a.id === agent.account) ? agent.account : DEFAULT_ACCOUNT;
  const pickModel = (next: { engine: Engine; model: string; effort: string }) => {
    if (next.engine === choice.engine && next.model === choice.model && next.effort === choice.effort) return;
    // An account belongs to its engine: another engine runs on its Default.
    void save({ engine: next.engine, model: next.model, effort: next.effort, ...(next.engine !== agent.engine ? { account: DEFAULT_ACCOUNT } : {}) });
  };
  return (
    <>
      {onHost && <HostPicker state={state} value={host} onChange={onHost} onAddHost={() => actions.go({ page: "hosts" })} />}
      <ModelPicker models={models} value={choice} engines={engines} onChange={pickModel} speed={speed} />
      {accountChoices(accounts, choice.engine).length > 1 && (
        <AccountPicker
          state={state}
          host={host}
          engine={choice.engine}
          account={account}
          engines={engines}
          onPick={(engine, id) => {
            // Picking an account under another engine means running on it, as on New session.
            const next = engine === choice.engine ? null : choiceOnEngine(models, engine, choice.effort);
            if (id !== account || next) void save({ account: id, ...(next ?? {}) });
          }}
          onManage={() => actions.go({ page: "hosts", host, tab: "engines" })}
        />
      )}
      <EffortPicker models={models} value={choice} onChange={(effort) => effort !== choice.effort && void save({ effort })} />
      <PermissionPicker engine={choice.engine} value={agent.permission} onChange={(permission) => permission !== agent.permission && void save({ permission })} />
    </>
  );
}

/** What the narrow box's setup button says about the agent's pickers. */
export function agentSettings(state: State, agent: Agent) {
  const host = agentHost(agent);
  const models = modelsOn(state, host);
  const c = agentChoice(agent, models);
  const accounts = accountsOn(state, host);
  const account = accountChoices(accounts, c.engine).length > 1 ? accountName(accounts, c.engine, agent.account) : "";
  return setupSettings(modelName(models, c.engine, c.model), account, shownEffort(models, c.engine, c.model, c.effort), agent.permission);
}

/** The agent's face as a button: a small popover picks its face and colour,
 *  saved at once. The popover's own face shows the pick before yardd has it. */
export function AgentFaceButton({ actions, agent, size, busy = false, className = "" }: { actions: Actions; agent: Agent; size: "sm" | "lg" | "xl"; busy?: boolean; className?: string }) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  return (
    <>
      <button ref={anchor} className={`btn-quiet agent-face-button ${className}`} aria-label={`Face and colour of ${agent.name}`} title="Change its face and colour" aria-haspopup="dialog" aria-expanded={open} onClick={() => setOpen(!open)}>
        <AgentAvatar agent={agent} size={size} busy={busy} />
      </button>
      {open && <AgentLook actions={actions} agent={agent} anchor={anchor} onClose={() => setOpen(false)} />}
    </>
  );
}

function AgentLook({ actions, agent, anchor, onClose }: { actions: Actions; agent: Agent; anchor: React.RefObject<HTMLButtonElement | null>; onClose(): void }) {
  const [look, setLook] = useState<{ face: AgentFace; color: AgentColor }>(() => ({ face: faceOf(agent), color: agent.color }));
  const [error, setError] = useState<string | null>(null);
  const pick = async (next: { face: AgentFace; color: AgentColor }) => {
    const before = look;
    setLook(next);
    setError(null);
    const r = await actions.saveAgent({ id: agent.id, face: next.face, color: next.color });
    if (r.error) {
      setLook(before);
      setError(r.error);
    }
  };
  return (
    <AnchoredPopover anchor={anchor} label={`Face and colour of ${agent.name}`} variant="agent-look-popover" onClose={onClose}>
      <div className="agent-look-head">
        <AgentAvatar agent={{ id: agent.id, ...look }} size="lg" />
        <span className="agent-look-title">
          <strong>{agent.name}</strong>
          <span className="muted small">Its face and colour, saved as you pick.</span>
        </span>
      </div>
      <div className="agent-look-grid" role="group" aria-label="Face">
        {AGENT_FACES.map((f) => (
          <button key={f} type="button" className="btn-icon agent-face-option agent-look-option" aria-label={`Face: ${f}`} aria-pressed={look.face === f} title={f} onClick={() => void pick({ ...look, face: f })}>
            <AgentAvatar agent={{ id: `${agent.id}-${f}`, color: look.color, face: f }} size="md" still={look.face !== f} />
          </button>
        ))}
      </div>
      <div className="agent-look-grid" role="group" aria-label="Colour">
        {AGENT_COLORS.map((c) => (
          <button key={c} type="button" className="btn-icon agent-swatch" aria-label={`Colour: ${c}`} aria-pressed={look.color === c} title={c} onClick={() => void pick({ ...look, color: c })}>
            <span className={`agent-swatch-dot agent-${c}`} />
          </button>
        ))}
      </div>
      {error && (
        <div className="small error-line" role="alert">
          {error}
        </div>
      )}
    </AnchoredPopover>
  );
}

/** A line of the agent's (its name, what it is for) edited where it is shown:
 *  click it, Enter saves, Escape cancels; leaving the field saves a change.
 *  A problem is said under the field, which stays open. */
export function InPlace({ value, label, empty, problem, maxLength, onSave, className = "" }: {
  value: string;
  /** The field's name, also the button's tooltip ("Rename", …). */
  label: string;
  /** Shown when there is no value yet, to add one. */
  empty?: string;
  problem?(text: string): string | null;
  maxLength: number;
  onSave(text: string): Promise<string | null>;
  className?: string;
}) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(value);
  const [error, setError] = useState<string | null>(null);
  const saving = useRef(false);
  const field = useRef<HTMLInputElement>(null);
  const problemId = `in-place-${label.replace(/\W+/g, "-").toLowerCase()}`;
  useLayoutEffect(() => {
    if (!editing) return;
    field.current?.focus();
    field.current?.select();
  }, [editing]);
  const cancel = () => {
    setEditing(false);
    setError(null);
  };
  const commit = async () => {
    if (saving.current) return;
    const next = text.trim();
    if (next === value) return cancel();
    const why = problem?.(next) ?? null;
    if (why) return setError(why);
    saving.current = true;
    const failed = await onSave(next);
    saving.current = false;
    if (failed) setError(failed);
    else setEditing(false);
  };
  if (!editing)
    return (
      <button
        className={`btn-quiet in-place${value ? "" : " in-place-empty"} ${className}`}
        title={label}
        onClick={() => {
          setText(value);
          setError(null);
          setEditing(true);
        }}
      >
        <span className="in-place-text">{value || empty}</span>
      </button>
    );
  return (
    <span className={`in-place-edit ${className}`}>
      <input
        ref={field}
        className="field in-place-input"
        aria-label={label}
        aria-invalid={!!error}
        aria-describedby={error ? problemId : undefined}
        value={text}
        maxLength={maxLength}
        enterKeyHint="done"
        placeholder={empty}
        onChange={(e) => {
          setText(e.target.value);
          setError(null);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            void commit();
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            cancel();
          }
        }}
        onBlur={() => !error && void commit()}
      />
      {error && (
        <span id={problemId} className="small in-place-problem" role="alert">
          {error}
        </span>
      )}
    </span>
  );
}

/** The agent's name, renamed in place. A name of its own ends its setup. */
export function AgentName({ state, actions, agent }: { state: State; actions: Actions; agent: Agent }) {
  return (
    <InPlace
      value={agent.name}
      label="Rename"
      maxLength={AGENT_NAME_MAX + 8}
      problem={(text) => nameProblem(text, state.agents, agent.id)}
      onSave={async (name) => (await actions.saveAgent({ id: agent.id, name: cleanName(name) })).error ?? null}
    />
  );
}

/** What the agent is for, changed in place. */
export function AgentAbout({ actions, agent, className }: { actions: Actions; agent: Agent; className?: string }) {
  return (
    <InPlace
      value={agent.about}
      label="What it is for"
      empty="Add what it is for"
      maxLength={160}
      className={className}
      onSave={async (about) => (await actions.saveAgent({ id: agent.id, about })).error ?? null}
    />
  );
}

/** New agent: made at once with a placeholder name and the last model used,
 *  opened at its conversation, where it sets itself up. */
function useNewAgent(state: State, actions: Actions) {
  const [making, setMaking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const make = async () => {
    setMaking(true);
    setError(null);
    const c = loadChoice(modelsOn(state, "local"));
    const r = await actions.newAgent({ engine: c.engine, model: c.model, effort: c.effort });
    setMaking(false);
    if (r.error) setError(r.error);
  };
  return { making, error, make };
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
  runsOn(agent, modelsOn(state, agentHost(agent)), permissionLabel(agent.permission), accountLabel(accountsOn(state, agentHost(agent)), agent));

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
  // What it asked for waits on the owner: that comes before how its last turn went.
  const asks = !!(agent.request?.cwd || agent.request?.permission || agent.request?.slack);
  const tone: AgentTone = asks && status.tone !== "approval" ? "needsYou" : status.tone;
  const text = tone !== status.tone ? "Asks you for something" : status.text;
  const inSlack = state.slack?.agentApps?.[agent.id]?.step === "ready";
  return (
    <section className="automation agent-card" aria-label={agent.name}>
      <button className="btn-quiet agent-card-head" title={`Open ${agent.name}'s conversation`} onClick={() => openAgent(agent, actions)}>
        <AgentAvatar agent={agent} busy={status.tone === "live"} />
        <span className="agent-card-name">
          <strong className="automation-name">{agent.name}</strong>
          <span className="muted small">{agent.setup ? "Setting up" : `@${handleOf(agent)}`}</span>
        </span>
      </button>
      {agent.about ? <p className="agent-about small">{agent.about}</p> : agent.setup && <p className="agent-about small muted">Tell it what it is for in its conversation.</p>}
      <RunsLine state={state} agent={agent} className="automation-when muted small" />
      {inSlack && (
        <div className="agent-card-slack muted small" title={`People can @mention ${agent.name} in Slack`}>
          <Icon d={SLACK_MARK} size={11} />
          In Slack
        </div>
      )}
      <div className={`automation-status status-${tone}`} title={text}>
        <span className={`mark mark-${MARK[tone]}`} aria-hidden="true">
          {(tone === "approval" || tone === "needsYou") && <span className="mark-bang">!</span>}
        </span>
        <span className="automation-status-text">{text}</span>
      </div>
      <div className="automation-foot">
        <button className="btn agent-message" onClick={() => openAgent(agent, actions)}>
          Message
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
  const fresh = useNewAgent(state, actions);
  return (
    <div className="hosts automations agents">
      <header className="bar">
        <MenuButton />
        <h1>Agents</h1>
        <span className="spacer" />
        <button className="btn" disabled={fresh.making || !state.connected} onClick={() => void fresh.make()}>
          <Icon d={PLUS} size={13} />
          New agent
        </button>
      </header>
      <div className="scroll">
        <div className="hosts-inner automation-list">
          {fresh.error && (
            <div className="small automation-message" role="alert">
              {fresh.error}
            </div>
          )}
          <div className="automation-grid">
            {state.agents.map((a) => (
              <AgentCard key={a.id} state={state} actions={actions} agent={a} now={now} seenAt={seen.seenAt} />
            ))}
            {state.agents.length === 0 && state.connected && (
              <button className="btn-quiet host-empty agent-empty" disabled={fresh.making} onClick={() => void fresh.make()}>
                <Icon d={PLUS} size={16} />
                <span>
                  <strong>Make an agent</strong>
                  <span className="muted small">Tell it what it is for and it sets itself up. Type @ and its name in any message, or in Slack, to reach it.</span>
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
 *  is for (each changed in place), Slack, Edit. `host`: where the conversation shown runs, when not
 *  the agent's own host (one it left behind when it moved). */
export function AgentBar({ state, agent, actions, host = agentHost(agent), busy = false, children }: { state: State; agent: Agent; actions: Actions; host?: string; busy?: boolean; children?: React.ReactNode }) {
  const [slackOpen, setSlackOpen] = useState(false);
  const slack = state.slack?.agentApps?.[agent.id];
  const slackLabel = slack?.step === "ready" ? "In Slack" : slack ? "Finish Slack" : "Add to Slack";
  return (
    <header className="bar agent-bar">
      <MenuButton />
      <AgentFaceButton actions={actions} agent={agent} size="sm" busy={busy} />
      <h1 className="agent-bar-name">
        <AgentName state={state} actions={actions} agent={agent} />
      </h1>
      {agent.about && (
        <span className="small agent-bar-about">
          <AgentAbout actions={actions} agent={agent} />
        </span>
      )}
      {host !== "local" && (
        <span className="muted small bar-host">
          <Cloud size={13} />
          {hostName(state, host)}
          {!hostOnline(state, host) && " · offline"}
        </span>
      )}
      <span className="spacer" />
      {children}
      <button
        className={`btn-text agent-slack-button${slack?.state === "connected" ? " is-live" : ""}`}
        aria-label={slackLabel}
        title={slack?.step === "ready" ? `${agent.name} in Slack` : `Let people @mention ${agent.name} in Slack`}
        onClick={() => setSlackOpen(true)}
      >
        <Icon d={SLACK_MARK} size={13} />
        <span className="agent-slack-label">{slackLabel}</span>
      </button>
      <button className="btn-text" onClick={() => actions.go({ page: "agentEdit", id: agent.id })}>
        Edit
      </button>
      {slackOpen && <SlackSetup state={state} actions={actions} agent={agent} onClose={() => setSlackOpen(false)} />}
    </header>
  );
}

/** What the agent asked for in its conversation and may not give itself: a
 *  folder of the user's, more access, a place in Slack. Above the reply box
 *  until answered. */
export function AgentRequests({ state, actions, agent }: { state: State; actions: Actions; agent: Agent }) {
  const [slackOpen, setSlackOpen] = useState(false);
  const r = agent.request;
  const dialog = slackOpen && <SlackSetup state={state} actions={actions} agent={agent} onClose={() => setSlackOpen(false)} />;
  if (!r || (!r.cwd && !r.permission && !r.slack)) return dialog || null;
  const answer = (part: keyof AgentRequest, allow: boolean) => actions.answerRequest(agent.id, part, allow);
  const rows: { part: keyof AgentRequest; text: string; allow: string; onAllow(): void }[] = [];
  if (r.cwd) rows.push({ part: "cwd", text: `${agent.name} asks to work in ${tilde(r.cwd, homeOn(state, agentHost(agent)))}.`, allow: "Allow", onAllow: () => answer("cwd", true) });
  if (r.permission)
    rows.push({
      part: "permission",
      text: `${agent.name} asks for ${permissionLabel(r.permission)}${r.permission === "full" ? ": it would run anything without asking" : r.permission === "acceptEdits" ? ": it would change files without asking" : ""}.`,
      allow: `Give ${permissionLabel(r.permission)}`,
      onAllow: () => answer("permission", true),
    });
  if (r.slack)
    rows.push({
      part: "slack",
      text: `${agent.name} asks to join Slack, so people can @mention it there.`,
      allow: "Add to Slack",
      onAllow: () => {
        setSlackOpen(true);
        answer("slack", true);
      },
    });
  return (
    <div className="agent-requests" role="group" aria-label={`${agent.name} asks`}>
      {rows.map((row) => (
        <div key={row.part} className="agent-request">
          <AgentAvatar agent={agent} size="xs" still />
          <span className="agent-request-text">{row.text}</span>
          <span className="agent-request-actions">
            <button className="btn-text" onClick={() => answer(row.part, false)}>
              Not now
            </button>
            <button className="btn" onClick={row.onAllow}>
              {row.part === "slack" && <Icon d={SLACK_MARK} size={12} />}
              {row.allow}
            </button>
          </span>
        </div>
      ))}
      {dialog}
    </div>
  );
}

/** An agent's conversation before its first message. A new one greets the
 *  user and is set up from what they say; any other shows its face, what it
 *  is for, and the box. */
export function AgentStart({ state, actions, agent, earlier }: { state: State; actions: Actions; agent: Agent; earlier?: React.ReactNode }) {
  const [error, setError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [slackOpen, setSlackOpen] = useState(false);
  const host = agentHost(agent);
  const online = hostOnline(state, host);
  const edit = useAgentSave(actions, agent.id);
  const move = useAgentMove(state, agent, edit.save);
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
  const where = agentWhere(agent, (h) => hostName(state, h), tilde(agent.cwd, homeOn(state, host)));
  const discard = () => {
    void actions.deleteAgent(agent.id);
    actions.go({ page: "agents" });
  };
  return (
    <div className="agent-page">
      <AgentBar state={state} agent={agent} actions={actions} />
      <div className="new agent-start">
        <div className="new-inner">
          <div className="agent-start-face">
            <AgentFaceButton actions={actions} agent={agent} size={agent.setup ? "xl" : "lg"} />
          </div>
          {agent.setup ? (
            <>
              <h1>Hi! What should I do?</h1>
              <p className="agent-start-about muted">
                Tell me in your own words. I'll pick a name, write my own instructions and set myself up. Later, just tell me to change anything: my name, look, model,
                folder or access.
              </p>
            </>
          ) : (
            <>
              <h1>Message {agent.name}</h1>
              <p className="agent-start-about muted">
                <AgentAbout actions={actions} agent={agent} />
              </p>
            </>
          )}
          {earlier}
          <AgentRequests state={state} actions={actions} agent={agent} />
          <Composer
            placeholder={agent.setup ? "What it is for, where it works, what it should never do" : `Message ${agent.name}`}
            rows={1}
            large
            draft={`agent:${agent.id}`}
            target={target}
            upload={actions.upload}
            busy={sending || !online}
            onSubmit={send}
            mentions={state.agents}
            mentionHint={(other) => (other.id === agent.id ? null : `Goes to ${other.name}`)}
            settings={agentSettings(state, agent)}
            setupTitle="Agent setup"
          >
            <AgentRunPickers state={state} actions={actions} agent={agent} save={edit.save} />
          </Composer>
          <div className="new-chips">
            <HostPicker state={state} value={host} onChange={move.pick} onAddHost={() => actions.go({ page: "hosts" })} />
            <span className="muted small agent-start-where">Works in {where}</span>
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
          {move.confirm}
          {(error || edit.error) && (
            <div className="small automation-message" role="alert">
              {error ?? edit.error}
            </div>
          )}
          {agent.setup && (
            <>
              <div className="agent-suggestions" role="group" aria-label="Ideas">
                {FIRST_IDEAS.map((q) => (
                  <button key={q} className="btn-quiet" disabled={sending || !online} onClick={() => void send(q, [])}>
                    <span>{q}</span>
                    <span aria-hidden="true">↗</span>
                  </button>
                ))}
              </div>
              <div className="agent-start-foot">
                <button className="btn-text" onClick={() => setSlackOpen(true)}>
                  <Icon d={SLACK_MARK} size={13} />
                  Add it to Slack
                </button>
                <span className="spacer" />
                <button className="btn-text" onClick={discard}>
                  Discard
                </button>
              </div>
            </>
          )}
          {slackOpen && <SlackSetup state={state} actions={actions} agent={agent} onClose={() => setSlackOpen(false)} />}
        </div>
      </div>
    </div>
  );
}

/** The `new` address makes a new agent and opens it, as the New agent button does. */
export function NewAgentRedirect({ state, actions }: { state: State; actions: Actions }) {
  const fresh = useNewAgent(state, actions);
  useEffect(() => {
    if (state.connected) void fresh.make();
  }, [state.connected]);
  if (!fresh.error) return null;
  return (
    <div className="hosts automations agents">
      <header className="bar">
        <MenuButton />
        <h1>New agent</h1>
      </header>
      <div className="small automation-message" role="alert">
        {fresh.error}
      </div>
    </div>
  );
}

export function AgentForm({ state, actions, id }: { state: State; actions: Actions; id: string }) {
  const existing = state.agents.find((a) => a.id === id);
  const [host, setHost] = useState(() => (existing ? agentHost(existing) : "local"));
  const models = modelsOn(state, host);
  const home = homeOn(state, host);
  const workspace = state.workspace[host] || home;
  const online = hostOnline(state, host);
  const [name, setName] = useState(existing?.name ?? "");
  const [color, setColor] = useState<AgentColor>(existing?.color ?? AGENT_COLORS[state.agents.length % AGENT_COLORS.length]);
  // A new agent's face steps differently from its colour, so the pairs vary.
  const [face, setFace] = useState<AgentFace>(() => (existing ? faceOf(existing) : AGENT_FACES[(state.agents.length * 3 + 1) % AGENT_FACES.length]));
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
    const engine = choice.engine;
    const list = models[engine] ?? [];
    const first = list.find((m) => m.isDefault) ?? list[0];
    setChoice(first ? { engine, model: first.id, effort: first.defaultEffort ?? first.efforts[0] ?? "" } : loadChoice(models));
  }, [models]);
  const engines = pickerEngines(models, choice.engine);
  const recent = useMemo(() => {
    const fromSessions = Object.values(state.sessions)
      // Agents' conversations run in the agents' own folders, not projects.
      .filter((s) => hostOf(s) === host && !s.agent && !isAgentHome(s.cwd))
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
      name: cleanName(name),
      color,
      face,
      about,
      instructions,
      ...(!existing || memory !== memoryBase.current ? { memory } : {}),
      engine: choice.engine,
      model: choice.model,
      effort: effortFor(models[choice.engine]?.find((m) => m.id === choice.model), choice.effort),
      account,
      cwd,
      permission,
      host,
    });
    setSaving(false);
    if (r.error) setError(r.error);
    else actions.go(existing ? { page: "agent", id: existing.id } : { page: "agents" });
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
  const deleteControls = existing && (
    <div className="row agent-form-delete">
      {confirmDelete ? (
        <>
          <span className="muted small">Its conversation{state.slack?.agentApps?.[existing.id] ? " and its Slack app go" : " goes"} too.</span>
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
  const moving = moveNote(existing, host, named);
  return (
    <div className="hosts automations agents">
      <header className="bar">
        <MenuButton />
        <h1>{existing ? `Edit ${existing.name}` : "New agent"}</h1>
      </header>
      <div className="scroll">
        <div className="hosts-inner automation-form agent-form">
          <div className="agent-form-hero">
            <AgentAvatar agent={{ id: existing?.id, color, face }} size="xl" />
            <div className="agent-form-name">
              <label htmlFor="agent-name" className="sr-only">
                Name
              </label>
              <input id="agent-name" className="field agent-name-input" value={name} maxLength={AGENT_NAME_MAX + 8} onChange={(e) => setName(e.target.value)} placeholder="Name it" autoFocus={!existing} />
              {nameIssue ? (
                <span className="small automation-reason" role="alert">
                  {nameIssue}
                </span>
              ) : (
                <span className="muted small">
                  {name.trim() ? `Mention it as @${handleFor(name, state.agents, existing?.id)} in any message.` : "A word or a few: it is @mentioned by a handle made from them."} Or tell it what to change in its conversation.
                </span>
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
                    <AgentAvatar agent={{ id: `${existing?.id ?? "new"}-${f}`, color, face: f }} size="lg" still={face !== f} />
                  </button>
                ))}
              </div>
            </div>
            <div className="automation-field">
              <span className="automation-label">Colour</span>
              <div className="agent-colors" role="group" aria-label="Colour">
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
                {cwd ? `It works in ${tilde(cwd, home)}${elsewhere}, one writer at a time.` : `It works in a folder of its own on ${named(host)}.`}
                {permission === "full" && " Full access: it runs anything without asking."}
                {moving && ` ${moving}`}
                {!online && host !== "local" && ` ${named(host)} is offline: changes can be saved once it is back.`}
              </p>
            </div>
            <div className="agent-form-where">
              <div className="new-chips">
                <HostPicker state={state} value={host} onChange={pickHost} onAddHost={() => actions.go({ page: "hosts" })} />
                <FolderPicker host={host} home={home} workspace={workspace} value={cwd} recent={recent} dirs={state.dirs} listDirs={actions.listDirs} onChange={setCwd} placeholder="Its own folder" />
                {cwd && (
                  <button className="btn-text" onClick={() => setCwd("")}>
                    Use its own folder
                  </button>
                )}
              </div>
              <div className="automation-pickers">
                <ModelPicker models={models} value={choice} engines={engines} onChange={setChoice} />
                {accountChoices(accounts, choice.engine).length > 1 && (
                  <AccountPicker state={state} host={host} engine={choice.engine} account={account} engines={engines} onPick={pickAccount} onManage={() => actions.go({ page: "hosts", host, tab: "engines" })} />
                )}
                <EffortPicker models={models} value={choice} onChange={(effort) => setChoice({ ...choice, effort })} />
                <PermissionPicker engine={choice.engine} value={permission} onChange={setPermission} />
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
            <button type="button" className="btn-text" onClick={() => actions.go(existing ? { page: "agent", id: existing.id } : { page: "agents" })}>
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
