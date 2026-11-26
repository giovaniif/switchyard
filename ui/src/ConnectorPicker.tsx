// Which connectors (MCP servers) an agent's or automation's sessions get
// (yardd's scope.rs). The list is the account's, as the Connectors page shows
// it; `value` null is the default for a record saved before the list existed:
// every server but Metabase.
import { useEffect } from "react";
import { connectorOn, displayName, pickableConnectors, shortError, toggleConnector } from "./connectors";
import type { Engine } from "./protocol";
import { hostOnline, mcpKey, type Actions, type State } from "./store";

/** Engines whose sessions yardd scopes; the others get every server. */
export const SCOPED_ENGINES: Engine[] = ["claude", "codex"];

export function ConnectorPicker({ state, actions, host, engine, account, value, onChange, who }: {
  state: State;
  actions: Actions;
  host: string;
  engine: Engine;
  account: string;
  value: string[] | null;
  onChange(list: string[]): void;
  /** "It" or "Each run", for the hint. */
  who: string;
}) {
  const scoped = SCOPED_ENGINES.includes(engine);
  const status = state.mcp[mcpKey(host, engine, account)];
  const online = hostOnline(state, host);
  useEffect(() => {
    if (scoped && online && !status) actions.mcpStatus(host, engine, account);
  }, [scoped, online, host, engine, account, !!status]);
  if (!scoped) return <span className="muted small">{who} gets every connector of the account: only Claude and Codex sessions can be limited.</span>;

  const names = pickableConnectors(status?.servers, value);
  const on = names.filter((n) => connectorOn(value, n));
  const servers = status?.servers ?? [];
  return (
    <>
      {status?.loading && !status.servers && <span className="muted small">Loading the account's connectors…</span>}
      {status?.error && !status.servers && <span className="small automation-reason">{shortError(status.error)}</span>}
      {!status && !online && <span className="muted small">The host is offline, so its connectors can't be listed.</span>}
      {status?.servers && !names.length && <span className="muted small">This account has no connectors.</span>}
      {names.length > 0 && (
        <div className="connector-picks" role="group" aria-label="Connectors">
          {names.map((name) => {
            const server = servers.find((s) => s.name === name);
            const shown = server ? displayName(server) : { name, via: "no longer listed" };
            return (
              <label key={name} className="check" title={name}>
                <input type="checkbox" checked={connectorOn(value, name)} onChange={() => onChange(toggleConnector(value, names, name))} />
                <span className="connector-pick-name">{shown.name}</span>
                <span className="muted small">{shown.via}</span>
              </label>
            );
          })}
        </div>
      )}
      <span className="muted small">
        {on.length ? `${who} can use ${on.length === names.length ? "all of these" : `${on.length} of ${names.length}`}` : `${who} uses no connectors`}
        {value === null ? " (the default: every connector but Metabase)." : "."} Switchyard's own tools are always there.
      </span>
    </>
  );
}
