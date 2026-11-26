// The Automations page and its form. Each host keeps its own automations;
// this page lists them all as a grid of small cards, with the runs of each
// (ordinary sessions whose metadata names the automation, kept out of the
// sidebar) as a strip of marks.
import { SelectPicker } from "./SelectPicker";
import { useEffect, useMemo, useState } from "react";
import type { Automation, AutomationWindow, Engine, Permission, Schedule, SessionMeta } from "./protocol";
import { branchKey, homeOn, hostName, hostOf, hostOnline, modelsOn, usageOn, type Actions, type State } from "./store";
import { cardStatus, describeSchedule, runLabel, usageCaps, runOutcome, runsOf, runSummary, runTook, RUN_LABEL, scheduleParts, scheduleProblem, stripRuns, type CardStatus, type RunOutcome } from "./automations";
import { duration } from "./thread";
import { ENGINE_NAME, pickerEngines } from "./engines";
import { metersFor } from "./usage";
import { ConnectorPicker } from "./ConnectorPicker";
import { DEFAULT_ACCOUNT } from "./accounts";
import {
  BranchPicker,
  Cloud,
  FolderPicker,
  HostPicker,
  Icon,
  MenuButton,
  EffortPicker,
  ModelPicker,
  PERMISSIONS,
  PermissionPicker,
  PIN,
  PLUS,
  effortFor,
  loadChoice,
  isAgentHome,
  readRecent,
} from "./App";

const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
/** Marks in a card's run strip; the runs list holds the rest. Ten fit beside the actions on the narrowest card. */
const STRIP = 10;
const PLAY = "M7 5v14l11-7Z";
const PENCIL = "M4 20h4L19 9l-4-4L4 16ZM13.5 6.5l4 4";
const LIST = "M9 6h11M9 12h11M9 18h11M4 6h.01M4 12h.01M4 18h.01";

/** The status line's mark: the session row marks, plus done (filled) and skipped (amber ring) for runs. */
const STATUS_MARK: Record<CardStatus["tone"], string> = { live: "working", approval: "approval", ok: "done", bad: "failed", warn: "skipped", quiet: "idle" };

const live = (s: SessionMeta) => s.status === "running" || s.status === "needs_approval" || s.status === "queued";

/** The current time, refreshed so "in 12 min" does not go stale on an open
 *  page; every second while a run's elapsed time is shown. */
function useNow(every: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), every);
    return () => clearInterval(t);
  }, [every]);
  return now;
}

function Mark({ kind }: { kind: RunOutcome | string }) {
  return (
    <span className={`mark mark-${kind}`} aria-hidden="true">
      {kind === "approval" && <span className="mark-bang">!</span>}
    </span>
  );
}

/** Every run, newest first, with how long it took and a pin: pinning is how a run goes back to the sidebar. */
function RunList({ name, runs, state, actions, now }: { name: string; runs: SessionMeta[]; state: State; actions: Actions; now: number }) {
  return (
    <ul className="automation-runs" aria-label="Runs, newest first">
      {runs.map((run) => {
        const outcome = runOutcome(run);
        const when = runLabel(run, name, now);
        return (
          <li key={run.id} className="automation-run-item">
            <button className="btn-quiet automation-run" title={run.title} onClick={() => actions.go({ page: "session", id: run.id })}>
              <Mark kind={outcome} />
              <span className="automation-run-when">{when}</span>
              <span className={`automation-run-outcome run-${outcome}`}>{RUN_LABEL[outcome]}</span>
              <span className="automation-run-time">{duration(runTook(run, now))}</span>
            </button>
            <button
              className="btn-icon automation-pin"
              aria-label={run.pinned ? `Run ${when} is pinned` : `Pin run ${when}`}
              title={run.pinned ? "Pinned: it is in the sidebar" : "Pin: bring it back to the sidebar"}
              disabled={!!run.pinned || !hostOnline(state, hostOf(run))}
              onClick={() => actions.setPinned(run.id, true)}
            >
              <Icon d={PIN} size={13} />
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function AutomationCard({ state, actions, host, a, now }: { state: State; actions: Actions; host: string; a: Automation; now: number }) {
  const online = hostOnline(state, host);
  const runs = useMemo(() => runsOf(state.sessions, host, a.id), [state.sessions, host, a.id]);
  const [open, setOpen] = useState(false);
  const [starting, setStarting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const status = cardStatus(a, runs, now);
  const run = async () => {
    setStarting(true);
    setMessage(null);
    const r = await actions.runAutomation(host, a.id);
    setStarting(false);
    // A run that could not start is already the last run, reason included:
    // only a request yardd refused needs its own line.
    if (r.error) setMessage(r.error);
  };
  // Skips and failures to start come from yardd's last run, with its reason.
  const reason = status.runId === null && (status.tone === "warn" || status.tone === "bad");
  const statusClass = `automation-status status-${status.tone}${reason ? " automation-reason" : ""}`;
  const statusBody = (
    <>
      <Mark kind={STATUS_MARK[status.tone]} />
      <span className="automation-status-text">{status.text}</span>
    </>
  );
  const [schedule, next] = scheduleParts(a, now);
  return (
    <section className="automation" aria-label={a.name}>
      <div className="automation-head">
        <strong className="automation-name" title={a.name}>
          {a.name}
        </strong>
        {host !== "local" && <Cloud size={13} label={`Runs on ${hostName(state, host)}`} />}
        <span className="spacer" />
        <button
          className="switch"
          role="switch"
          aria-checked={a.enabled}
          aria-label={`Enabled: ${a.name}`}
          title={a.enabled ? "On: runs on its schedule" : "Off: runs only with Run now"}
          disabled={!online}
          onClick={() => void actions.saveAutomation(host, { id: a.id, enabled: !a.enabled })}
        >
          <span className="switch-knob" />
        </button>
      </div>
      <div className="automation-when automation-when-split muted small" title={`${schedule} · ${next}`}>
        <span className="automation-when-schedule">{schedule}</span>
        <span className="automation-when-next">{` · ${next}`}</span>
      </div>
      {status.runId ? (
        <button className={`btn-quiet ${statusClass}`} title={`${status.title}. Open it`} onClick={() => actions.go({ page: "session", id: status.runId! })}>
          {statusBody}
        </button>
      ) : (
        // No live region: "2 min ago" changes every minute and would be read out each time.
        <div className={statusClass} title={status.title}>
          {statusBody}
        </div>
      )}
      {message && (
        <div className="small automation-reason automation-message" role="status">
          {message}
        </div>
      )}
      <div className="automation-foot">
        <div className="automation-strip" role="group" aria-label={`Recent runs of ${a.name}, newest last`}>
          {stripRuns(runs, STRIP).map((r) => {
            const summary = runSummary(r, now, a.name);
            return (
              <button key={r.id} className="btn-icon run-mark" aria-label={`Open run ${summary}`} title={summary} onClick={() => actions.go({ page: "session", id: r.id })}>
                <Mark kind={runOutcome(r)} />
              </button>
            );
          })}
        </div>
        <span className="spacer" />
        <button className="btn-icon" aria-label={`Run now: ${a.name}`} title={starting ? "Starting" : "Run now"} disabled={!online || starting} onClick={run}>
          <Icon d={PLAY} size={14} />
        </button>
        {runs.length > 0 && (
          <button className="btn-icon" aria-label={`Runs: ${a.name}`} aria-expanded={open} title={open ? "Hide the runs" : `All ${runs.length} runs, to open or pin one`} onClick={() => setOpen(!open)}>
            <Icon d={LIST} size={14} />
          </button>
        )}
        <button className="btn-icon" aria-label={`Edit: ${a.name}`} title="Edit" disabled={!online} onClick={() => actions.go({ page: "automation", host, id: a.id })}>
          <Icon d={PENCIL} size={14} />
        </button>
      </div>
      {open && runs.length > 0 && <RunList name={a.name} runs={runs} state={state} actions={actions} now={now} />}
    </section>
  );
}

export function AutomationsPage({ state, actions }: { state: State; actions: Actions }) {
  const working = useMemo(() => Object.values(state.sessions).some((s) => !!s.automation && live(s)), [state.sessions]);
  const now = useNow(working ? 1000 : 15_000);
  const hosts = ["local", ...state.hosts.map((h) => h.id)];
  const groups = hosts.map((host) => ({ host, list: state.automations[host] ?? [] })).filter((g) => g.list.length > 0);
  const grid = (host: string, list: Automation[]) => (
    <div className="automation-grid">
      {list.map((a) => (
        <AutomationCard key={`${host}/${a.id}`} state={state} actions={actions} host={host} a={a} now={now} />
      ))}
    </div>
  );
  return (
    <div className="hosts automations">
      <header className="bar">
        <MenuButton />
        <h1>Automations</h1>
        <span className="spacer" />
        <button className="btn" onClick={() => actions.go({ page: "automation", host: "local", id: "new" })}>
          <Icon d={PLUS} size={13} />
          New automation
        </button>
      </header>
      <div className="scroll">
        <div className="hosts-inner automation-list">
          {groups.length === 0 && (
            <div className="host-empty">
              <span>
                <strong>No automations yet</strong>
                <span className="muted small">
                  An automation starts a session on a schedule, in its own worktree. Its runs archive themselves and are listed here instead of the sidebar.
                </span>
              </span>
            </div>
          )}
          {groups.length === 1 && grid(groups[0].host, groups[0].list)}
          {groups.length > 1 &&
            groups.map(({ host, list }) => (
              <section key={host} className="automation-host" aria-label={hostName(state, host)}>
                <h2 className="automation-host-name">
                  {hostName(state, host)}
                  {host !== "local" && <Cloud size={13} />}
                </h2>
                {grid(host, list)}
              </section>
            ))}
        </div>
      </div>
    </div>
  );
}

const pad = (n: number) => String(n).padStart(2, "0");
const percent = (text: string): number | null => (text.trim() === "" ? null : Number(text));

export function AutomationForm({ state, actions, host: host0, id }: { state: State; actions: Actions; host: string; id: string }) {
  const existing = id === "new" ? undefined : state.automations[host0]?.find((a) => a.id === id);
  const editing = !!existing;
  const [host, setHost] = useState(host0);
  const target = host === "local" || hostOnline(state, host) ? host : "local";
  const models = modelsOn(state, target);
  const home = homeOn(state, target);
  const workspace = state.workspace[target] || home;

  const [name, setName] = useState(existing?.name ?? "");
  const [prompt, setPrompt] = useState(existing?.prompt ?? "");
  const [cwd, setCwd] = useState(existing?.cwd ?? "");
  const [choice, setChoice] = useState(() => (existing ? { engine: existing.engine, model: existing.model, effort: existing.effort } : loadChoice(modelsOn(state, host0))));
  const [permission, setPermission] = useState<Permission>(existing?.permission ?? "ask");
  const [base, setBase] = useState(existing?.base ?? "");
  const [kind, setKind] = useState<Schedule["kind"]>(existing?.schedule.kind ?? "daily");
  const [every, setEvery] = useState(String(existing?.schedule.kind === "interval" ? existing.schedule.every : 1));
  const [unit, setUnit] = useState<"minutes" | "hours">(existing?.schedule.kind === "interval" ? existing.schedule.unit : "hours");
  const [time, setTime] = useState(existing && "time" in existing.schedule ? existing.schedule.time : "09:00");
  const [day, setDay] = useState(existing?.schedule.kind === "weekly" ? existing.schedule.day : 1);
  const [limited, setLimited] = useState(!!existing?.window);
  const [start, setStart] = useState(existing?.window?.start ?? "21:00");
  const [end, setEnd] = useState(existing?.window?.end ?? "09:00");
  const [days, setDays] = useState<number[]>(existing?.window?.days ?? []);
  const [skip, setSkip] = useState(existing?.skipWhileRunning ?? true);
  const [fiveHour, setFiveHour] = useState(existing?.maxFiveHour == null ? "" : String(existing.maxFiveHour));
  const [week, setWeek] = useState(existing?.maxWeek == null ? "" : String(existing.maxWeek));
  // Null: a record from before the list, which keeps the default until a pick.
  const [mcp, setMcp] = useState<string[] | null>(existing ? existing.mcp ?? null : []);
  const [mcpTouched, setMcpTouched] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  // Models, the home folder and branches arrive per host, after the first render.
  // An automation being edited keeps its saved model even when the host no
  // longer lists it (the picker names it by id): the default would switch its
  // engine on save.
  useEffect(() => {
    if (editing) return;
    if (Object.values(models).some((l) => (l?.length ?? 0) > 0) && !models[choice.engine]?.some((m) => m.id === choice.model)) setChoice(loadChoice(models));
  }, [models, target]);
  useEffect(() => {
    if (!cwd && workspace && !editing) setCwd(workspace);
  }, [workspace]);
  const branchList = state.branches[branchKey(target, cwd)];
  useEffect(() => {
    if (cwd && state.connected) actions.loadBranches(cwd, target);
  }, [cwd, target, state.connected]);
  const isRepo = !!branchList?.current;

  const recent = useMemo(() => {
    const fromSessions = Object.values(state.sessions)
      // Agents' conversations run in the agents' own folders, not projects.
      .filter((s) => hostOf(s) === target && !s.agent && !isAgentHome(s.cwd))
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .map((s) => s.repo || s.cwd);
    return [...new Set([...readRecent(target), ...fromSessions])].slice(0, 12);
  }, [state.sessions, target]);

  // A limit the engine has no figure for is not offered, and is saved as none.
  const caps = usageCaps(choice.engine, metersFor(choice.engine, usageOn(state, target)).length > 0);
  const schedule: Schedule =
    kind === "interval" ? { kind, every: Number(every), unit } : kind === "daily" ? { kind, time } : kind === "weekly" ? { kind, day, time } : { kind };
  const window: AutomationWindow | null = kind === "interval" && limited ? { start, end, ...(days.length ? { days } : {}) } : null;
  const problem =
    (!name.trim() && "Give it a name.") ||
    (!prompt.trim() && "Write the prompt it runs.") ||
    (!cwd && "Choose a folder.") ||
    scheduleProblem(schedule, window) ||
    ([caps.fiveHour && fiveHour, caps.week && week].some((t) => t && t.trim() !== "" && !(Number(t) >= 0 && Number(t) <= 100)) ? "A usage limit is a percentage from 0 to 100." : null);

  const save = async () => {
    setSaving(true);
    setError(null);
    const r = await actions.saveAutomation(target, {
      ...(existing ? { id: existing.id } : {}),
      name,
      prompt,
      cwd,
      ...choice,
      effort: effortFor(models[choice.engine]?.find((m) => m.id === choice.model), choice.effort),
      permission,
      base,
      schedule,
      window,
      skipWhileRunning: skip,
      maxFiveHour: caps.fiveHour ? percent(fiveHour) : null,
      maxWeek: caps.week ? percent(week) : null,
      ...(!existing || mcpTouched ? { mcp: mcp ?? [] } : {}),
    });
    setSaving(false);
    if (r.error) setError(r.error);
    else actions.go({ page: "automations" });
  };

  return (
    <div className="hosts automations">
      <header className="bar">
        <MenuButton />
        <h1>{editing ? "Edit automation" : "New automation"}</h1>
      </header>
      <div className="scroll">
        {/* Not a <form>: the pickers' buttons would submit it. */}
        <div className="hosts-inner automation-form">
          <div className="automation-field">
            <label htmlFor="automation-name">Name</label>
            <input id="automation-name" className="field" aria-label="Automation name" value={name} onChange={(e) => setName(e.target.value)} placeholder="Patrol" />
          </div>
          <div className="automation-field">
            <label htmlFor="automation-prompt">Prompt</label>
            <textarea
              id="automation-prompt"
              className="field automation-prompt"
              aria-label="Prompt"
              rows={6}
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder="What each run should do"
            />
          </div>
          <div className="automation-field">
            <span className="automation-label">Where and how</span>
            <div className="new-chips">
              {editing ? (
                <span className="muted small">Runs on {hostName(state, host0)}</span>
              ) : (
                <HostPicker state={state} value={target} onChange={(h) => { setHost(h); setCwd(""); setBase(""); }} onAddHost={() => actions.go({ page: "hosts" })} />
              )}
              <FolderPicker host={target} home={home} workspace={workspace} value={cwd} recent={recent} dirs={state.dirs} listDirs={actions.listDirs} onChange={(f) => { setCwd(f); setBase(""); }} />
              {isRepo && <BranchPicker current={branchList.current!} branches={branchList.branches ?? []} value={base} onChange={setBase} />}
            </div>
            <div className="automation-pickers">
              <ModelPicker models={models} value={choice} engines={pickerEngines(models)} onChange={setChoice} />
              <EffortPicker models={models} value={choice} onChange={(effort) => setChoice({ ...choice, effort })} />
              <PermissionPicker engine={choice.engine} value={permission} onChange={setPermission} />
            </div>
            <span className="muted small">
              {isRepo
                ? `Each run gets a new worktree cut from ${base || "the default branch"}, fetched first.`
                : "Not a git repository: runs work in the folder itself, one at a time."}
            </span>
          </div>
          <div className="automation-field">
            <span className="automation-label">Connectors</span>
            <ConnectorPicker state={state} actions={actions} host={target} engine={choice.engine} account={DEFAULT_ACCOUNT} value={mcp} who="Each run" onChange={(list) => { setMcp(list); setMcpTouched(true); }} />
          </div>
          <div className="automation-field">
            <span className="automation-label">Schedule</span>
            <div className="row automation-schedule">
              <SelectPicker<Schedule["kind"]> label="Schedule type" value={kind} onChange={setKind} options={[
                { value: "interval", label: "Every few minutes or hours" },
                { value: "daily", label: "Daily" },
                { value: "weekly", label: "Weekly" },
                { value: "manual", label: "Manual: only when started" },
              ]} />
              {kind === "interval" && (
                <>
                  <input aria-label="Repeat every" className="field" type="number" min={1} value={every} onChange={(e) => setEvery(e.target.value)} />
                  <SelectPicker<"minutes" | "hours"> label="Repeat unit" value={unit} onChange={setUnit} options={[
                    { value: "minutes", label: "minutes" },
                    { value: "hours", label: "hours" },
                  ]} />
                </>
              )}
              {kind === "weekly" && (
                <SelectPicker label="Day of the week" value={day} onChange={setDay} options={DAYS.map((label, value) => ({ value, label }))} />
              )}
              {(kind === "daily" || kind === "weekly") && <input aria-label="Time" className="field" type="text" placeholder="HH:MM" maxLength={5} value={time} onChange={(e) => setTime(e.target.value)} />}
            </div>
            {kind === "interval" && (
              <>
                <div className="row automation-schedule">
                  <label className="check">
                    <input type="checkbox" checked={limited} onChange={(e) => setLimited(e.target.checked)} />
                    Only between
                  </label>
                  {limited && <div className="row">
                    <input aria-label="Window start" className="field" type="text" placeholder="HH:MM" maxLength={5} value={start} onChange={(e) => setStart(e.target.value)} />
                    <span className="muted">and</span>
                    <input aria-label="Window end" className="field" type="text" placeholder="HH:MM" maxLength={5} value={end} onChange={(e) => setEnd(e.target.value)} />
                  </div>}
                </div>
                {limited && <>
                  <div className="row automation-days" role="group" aria-label="Window start days">
                    {DAYS.map((d, i) => <button key={d} className="btn-text" aria-label={`Window starts on ${d}`} aria-pressed={days.includes(i)} onClick={() => setDays(days.includes(i) ? days.filter((day) => day !== i) : [...days, i].sort())}>{d.slice(0, 3)}</button>)}
                  </div>
                  <span className="muted small">{days.length ? "Selected days" : "Every day"}; days name when the window starts, including overnight runs.</span>
                </>}
              </>
            )}
            <span className="muted small">
              {kind === "manual"
                ? "Runs only when started, with Run now or by a script."
                : `${describeSchedule(schedule, window)}, in ${hostName(state, target)}'s local time zone. A run missed while Switchyard was off starts once when it is back${window ? ", only inside the window" : ""}.`}
            </span>
          </div>
          <div className="automation-field">
            <span className="automation-label">Skip a run</span>
            <label className="check">
              <input type="checkbox" checked={skip} onChange={(e) => setSkip(e.target.checked)} />
              While the previous run is still going
            </label>
            <div className="row automation-limits">
              <label>
                When 5-hour usage is above
                <input aria-label="5-hour usage limit percent" className="field" type="number" min={0} max={100} placeholder={caps.fiveHour ? "no limit" : "n/a"} disabled={!caps.fiveHour} value={caps.fiveHour ? fiveHour : ""} onChange={(e) => setFiveHour(e.target.value)} />%
              </label>
              <label>
                {caps.weekLabel}
                <input aria-label={choice.engine === "grok" ? "Credit usage limit percent" : "Weekly usage limit percent"} className="field" type="number" min={0} max={100} placeholder={caps.week ? "no limit" : "n/a"} disabled={!caps.week} value={caps.week ? week : ""} onChange={(e) => setWeek(e.target.value)} />%
              </label>
            </div>
            <span className="muted small">{caps.note}</span>
          </div>
          {(error || problem) && (
            <div className="small automation-reason" role="alert">
              {error ?? problem}
            </div>
          )}
          <div className="row">
            <button type="button" className="btn-primary" disabled={!!problem || saving || !state.connected} onClick={() => void save()}>
              {saving ? "Saving" : "Save automation"}
            </button>
            <button type="button" className="btn-text" onClick={() => actions.go({ page: "automations" })}>
              Cancel
            </button>
            <span className="spacer" />
            {editing && !confirmDelete && (
              <button type="button" className="btn-text danger" onClick={() => setConfirmDelete(true)}>
                Delete automation
              </button>
            )}
            {editing && confirmDelete && (
              <>
                <span className="muted small">Its runs stay as sessions.</span>
                <button
                  type="button"
                  className="btn danger"
                  onClick={() => {
                    actions.deleteAutomation(host0, existing!.id);
                    actions.go({ page: "automations" });
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
