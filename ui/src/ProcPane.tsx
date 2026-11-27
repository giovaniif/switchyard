import { useEffect, useState } from "react";
import { hostErrorText, hostOf, hostOnline, type Actions, type State } from "./store";
import type { ProcRow, SessionMeta } from "./protocol";
import { ENGINE_HINT, procsBlank, procSummary, procTree, memText, pruneKills, rowActions, shortCommand, type KillSignal, type KillState } from "./procs";
import { duration } from "./thread";
import { Icon } from "./App";

/** How often the list is asked for while the tab is visible. The host sampler
 *  that produces it runs every 3 s. */
const REFRESH_MS = 3000;
/** An armed Stop or Force kill lapses, like Delete and Remove host. */
const ARMED_MS = 4000;
const CLOSE = "M18 6 6 18M6 6l12 12";

/** The Processes tab: what the session is running right now. */
export function ProcPane(props: { meta: SessionMeta; state: State; actions: Actions; visible: boolean }) {
  const { meta, state, actions, visible } = props;
  const rows = state.procs[meta.id];
  const connected = hostOnline(state, hostOf(meta));
  // The tab's last kill of each pid; yardd keeps no record of them.
  const [kills, setKills] = useState<Record<number, KillState>>({});

  useEffect(() => {
    if (!visible || !connected) return;
    actions.loadProcs(meta.id);
    const timer = setInterval(() => actions.loadProcs(meta.id), REFRESH_MS);
    return () => clearInterval(timer);
  }, [visible, connected, meta.id]);
  useEffect(() => setKills({}), [meta.id]);
  useEffect(() => {
    if (rows) setKills((k) => pruneKills(k, rows));
  }, [rows]);

  const kill = async (row: ProcRow, signal: KillSignal, tree: boolean) => {
    const base = { signal, tree, start: row.start };
    setKills((k) => ({ ...k, [row.pid]: { ...base, phase: "sending" } }));
    const reply = await actions.killProc(meta.id, row.pid, row.start, signal, tree);
    const done: KillState =
      "error" in reply
        ? { ...base, phase: "done", result: "failed", reason: hostErrorText(state, reply.hostId, reply.error) }
        : { ...base, phase: "done", result: reply.result, reason: reply.reason };
    setKills((k) => ({ ...k, [row.pid]: done }));
    actions.loadProcs(meta.id);
  };

  const blank = procsBlank(rows, connected);
  if (blank !== null || !rows) return <div className="scroll pane-body" style={{ display: visible ? "flex" : "none" }}><div className="muted">{blank}</div></div>;
  const tree = procTree(rows);
  return (
    <div className="scroll pane-body" style={{ display: visible ? "flex" : "none" }}>
      <div className="muted small">{procSummary(rows)}</div>
      <table className="proc-table">
        <thead>
          <tr>
            <th>PID</th>
            <th>Command</th>
            <th className="num">CPU</th>
            <th className="num">Memory</th>
            <th className="num">Running</th>
            <th aria-label="Stop" />
          </tr>
        </thead>
        <tbody>
          {tree.map(({ row, depth }) => (
            <ProcLine key={row.pid} row={row} depth={depth} offers={rowActions(row, rows, kills[row.pid])} onKill={(signal, t) => kill(row, signal, t)} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** One process, its stop controls, and a line under it when a stop has
 *  something to say. Stopping asks for a second click. */
function ProcLine(props: { row: ProcRow; depth: number; offers: ReturnType<typeof rowActions>; onKill(signal: KillSignal, tree: boolean): void }) {
  const { row, depth, offers, onKill } = props;
  const [armed, setArmed] = useState<"stop" | "force" | null>(null);
  // The column clips a command to a few characters on a phone, and a touch
  // screen never shows the title: a tap shows the whole line, wrapped.
  const [whole, setWhole] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(null), ARMED_MS);
    return () => clearTimeout(timer);
  }, [armed]);
  // A finished request changes what is offered; an old arming must not carry over.
  useEffect(() => setArmed(null), [offers.stop, offers.force]);
  // The second click of a double click lands on the ask the first one just
  // put in the x's place ("Stop?", "Force kill?"). It is not an answer: the
  // ask stays up. A separate click, however quick, still answers.
  const fire = (e: { detail: number }, signal: KillSignal, tree: boolean) => {
    if (e.detail >= 2) return;
    setArmed(null);
    onKill(signal, tree);
  };
  const label = `${row.pid} ${shortCommand(row.cmd, 40)}`;
  // Two choices side by side are wider than the pane leaves the last column:
  // they go on a line under the row, as a note does, so the table keeps its widths.
  const choices = armed === "stop" && offers.stop && offers.withChildren;

  return (
    <>
      <tr className={whole ? "proc-row whole" : "proc-row"}>
        <td className="muted">{row.pid}</td>
        <td className="proc-cmd" title={whole ? undefined : row.cmd}>
          <button type="button" className="proc-cmd-toggle" aria-expanded={whole} style={{ paddingLeft: `${depth * 12}px` }} onClick={() => setWhole(!whole)}>
            {depth === 0 && (
              <span className="proc-tag" title={row.engine ? ENGINE_HINT : undefined}>
                {row.origin}
              </span>
            )}
            <code>{whole ? row.cmd : shortCommand(row.cmd)}</code>
          </button>
        </td>
        <td className="num">{Math.round(row.cpu)}%</td>
        <td className="num">{memText(row.memBytes)}</td>
        <td className="num">{duration(row.ageSecs * 1000)}</td>
        <td className="proc-act">
          {choices ? null : armed === "stop" && offers.stop ? (
            <button className="btn-text danger" aria-label={`Confirm stop process ${label}`} onClick={(e) => fire(e, "term", false)}>
              Stop?
            </button>
          ) : armed === "force" && offers.force ? (
            <button className="btn-text danger" aria-label={`Confirm force kill process ${label}`} onClick={(e) => fire(e, "kill", offers.forceTree)}>
              Force kill?
            </button>
          ) : offers.force ? (
            <button className="btn-text danger" aria-label={`Force kill process ${label}`} onClick={() => setArmed("force")}>
              Force kill
            </button>
          ) : offers.stop ? (
            <button className="btn-icon proc-kill" aria-label={`Stop process ${label}`} title="Stop (SIGTERM)" onClick={() => setArmed("stop")}>
              <Icon d={CLOSE} size={12} />
            </button>
          ) : null}
        </td>
      </tr>
      {choices && (
        <tr className="proc-choices">
          <td />
          <td colSpan={5}>
            <button className="btn-text danger" aria-label={`Confirm stop process ${label}`} onClick={(e) => fire(e, "term", false)}>
              Stop
            </button>
            <button className="btn-text danger" aria-label={`Stop process ${label} with its children`} onClick={(e) => fire(e, "term", true)}>
              Stop with children
            </button>
          </td>
        </tr>
      )}
      {offers.note && (
        <tr className="proc-note">
          <td />
          <td colSpan={5} className={offers.note.bad ? "danger" : "muted"} role="status">
            {offers.note.text}
          </td>
        </tr>
      )}
    </>
  );
}
