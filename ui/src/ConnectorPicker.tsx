// Which connectors (MCP servers) an agent's or automation's sessions get
// (yardd's scope.rs). The list is the account's, as the Connectors page shows
// it; `value` null is the default for a record saved before the list existed:
// every server but Metabase. Claude and Codex have a list per account; the
// other engines share the set Switchyard holds for them on the host.
import { useEffect } from "react";
import { connectorOn, displayName, pickableConnectors, shortError, toggleConnector } from "./connectors";
import type { ConnectorEngine, Engine } from "./protocol";
import { hostOnline, mcpKey, type Actions, type State } from "./store";

/** Engines that list their own servers per account; the others get Switchyard's. */
const OWN_ENGINES: Engine[] = ["claude", "codex"];

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
  const own = OWN_ENGINES.includes(engine);
  const listEngine: ConnectorEngine = own ? engine : "switchyard";
  const listAccount = own ? account : "default";
  const status = state.mcp[mcpKey(host, listEngine, listAccount)];
  const online = hostOnline(state, host);
  useEffect(() => {
    if (online && !status) actions.mcpStatus(host, listEngine, listAccount);
  }, [online, host, listEngine, listAccount, !!status]);

  const names = pickableConnectors(status?.servers, value);
  const on = names.filter((n) => connectorOn(value, n));
  const servers = status?.servers ?? [];
  return (
    <>
      {status?.loading && !status.servers && <span className="muted small">Loading the account's connectors…</span>}
      {status?.error && !status.servers && <span className="small automation-reason">{shortError(status.error)}</span>}
      {!status && !online && <span className="muted small">The host is offline, so its connectors can't be listed.</span>}
      {status?.servers && !names.length && <span className="muted small">{own ? "This account has no connectors." : "No connectors yet: add one on the Connectors page."}</span>}
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
