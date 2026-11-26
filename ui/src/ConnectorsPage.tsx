// The Connectors page: every MCP server (claude.ai connectors, plugins,
// config files, ChatGPT apps) each engine account can use on each host, whether
// it works, and a way to sign it in. Opening the page asks yardd to list them;
// for Claude that starts Claude Code with no prompt, so it spends no usage.
import { EngineIcon } from "./EngineIcon";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { AnchoredPopover } from "./AnchoredPopover";
import { SelectPicker } from "./SelectPicker";
import { accountChoices, DEFAULT_ACCOUNT } from "./accounts";
import { actionFor, CONNECTOR_PAGE_SIZE, filterServers, type ConnectorFilter, displayName, missingFromCatalog, newServerProblem, removable, shortError, sortServers, STATUS_LABEL, summary } from "./connectors";
import type { Engine, McpServer } from "./protocol";
import { accountsOn, hostName, hostOnline, mcpAuthKey, mcpKey, type Actions, type State } from "./store";
import { Icon, MenuButton, PLUS } from "./App";
import { loadHost } from "./sessionDefaults";
import { ENGINE_NAME } from "./engines";
import { SlackCard } from "./SlackCard";

const REFRESH = "M21 12a9 9 0 1 1-3-6.7M21 4v5h-5";
const ENGINES: Engine[] = ["claude", "codex"];

export function ConnectorsPage({ state, actions, usage }: { state: State; actions: Actions; usage: ReactNode }) {
  const hosts = ["local", ...state.hosts.map((host) => host.id)];
  const [pickedHost, setHost] = useState(loadHost);
  const host = hosts.includes(pickedHost) ? pickedHost : hosts[0];
  const online = hostOnline(state, host);
  const [filter, setFilter] = useState<Engine | "all">("all");
  const accounts = ENGINES.filter((engine) => state.engines[host]?.[engine]?.installed !== false).flatMap((engine) =>
    accountChoices(accountsOn(state, host), engine).map((account) => ({
      engine, account: account.id, accountName: account.name,
      key: mcpKey(host, engine, account.id),
      login: account.id === DEFAULT_ACCOUNT ? state.engines[host]?.[engine] : state.engines[host]?.accounts?.[account.id],
    })),
  );
  const shown = accounts.filter((account) => filter === "all" || account.engine === filter);
  const [recheck, setRecheck] = useState<string[]>([]);
  const eligible = shown.filter((account) => account.login?.loggedIn !== false);
  const pending = eligible.find((account) => !state.mcp[account.key] || recheck.includes(account.key));
  const loading = eligible.some((account) => state.mcp[account.key]?.loading);
  useEffect(() => {
    if (!online || loading || !pending) return;
    actions.mcpStatus(host, pending.engine, pending.account);
    setRecheck((keys) => keys.filter((key) => key !== pending.key));
  }, [actions, host, loading, online, pending?.key]);

  const refresh = (
    <button className="btn connector-refresh" aria-label={loading ? "Checking…" : "Check again"} disabled={!online || loading || !eligible.length} onClick={() => setRecheck(eligible.map((account) => account.key))}>
      <Icon d={REFRESH} size={13} />
      <span>{loading ? "Checking…" : "Check again"}</span>
    </button>
  );
  return (
    <div className="hosts connectors">
      <header className="bar connectors-mobile-header">
        <MenuButton />
        <h1>Connectors</h1>
        <span className="spacer" />
        {usage}
        {refresh}
      </header>
      <div className="scroll">
        <div className="hosts-inner connectors-inner">
          <div className="connector-page-head">
            <h1>Connectors</h1>
            {refresh}
            <p className="muted connectors-intro">A session can only use the tools connected to the account it runs on.</p>
          </div>
          <div className="connector-scope">
            <div className="connector-engine-filters" role="group" aria-label="Connectors by engine">
              <button className="btn-text btn-small" aria-pressed={filter === "all"} onClick={() => setFilter("all")}>All accounts</button>
              {ENGINES.map((engine) => (
                <button key={engine} className="btn-text btn-small" aria-pressed={filter === engine} onClick={() => setFilter(engine)}>{ENGINE_NAME[engine]}</button>
              ))}
            </div>
            {hosts.length > 1 && (
              <div className="connector-host-select">
                <span className="small muted">Host</span>
                <SelectPicker label="Connector host" value={host} onChange={setHost} options={hosts.map((id) => ({ value: id, label: hostName(state, id) }))} />
              </div>
            )}
          </div>
          <div className="connector-accounts">
            {shown.map(({ key, engine, account, accountName }) => <AccountCard key={key} state={state} actions={actions} host={host} engine={engine} account={account} accountName={accountName} online={online} />)}
            {!shown.length && <p className="muted">No accounts for this engine on {hostName(state, host)}.</p>}
          </div>
          {state.slack && (
            <div className="connector-apps">
              <div className="connector-section-head">
                <h2>Messaging</h2>
                <p className="muted small">Your own Slack, and who can talk to your agents there.</p>
              </div>
              <SlackCard state={state} actions={actions} />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function AccountCard(props: { state: State; actions: Actions; host: string; engine: Engine; account: string; accountName: string; online: boolean }) {
  const { state, actions, host, engine, account, online } = props;
  const status = state.mcp[mcpKey(host, engine, account)];
  const engineState = account === DEFAULT_ACCOUNT ? state.engines[host]?.[engine] : state.engines[host]?.accounts?.[account];
  const servers = sortServers(status?.servers ?? []);
  const [filter, setFilter] = useState<ConnectorFilter>("all");
  const [showControls, setShowControls] = useState(false);
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  // Keep an in-progress or completed sign-in visible until dismissed, even
  // when its new status moves it out of the current filter.
  const authenticating = servers.filter((s) => state.mcpAuth[mcpAuthKey(host, engine, account, s.name)] && filterServers([s], query, "all").length);
  const filtered = sortServers([...new Map([...filterServers(servers, query, filter), ...authenticating].map((s) => [s.name, s] as const)).values()]);
  const pages = Math.max(1, Math.ceil(filtered.length / CONNECTOR_PAGE_SIZE));
  const currentPage = Math.min(page, pages - 1);
  const visible = filtered.slice(currentPage * CONNECTOR_PAGE_SIZE, (currentPage + 1) * CONNECTOR_PAGE_SIZE);
  const filters: { id: ConnectorFilter; label: string }[] = [
    { id: "connected", label: "Connected" },
    { id: "available", label: "Available" },
    { id: "failed", label: "Not working" },
    { id: "all", label: "All" },
  ];
  const reveal = (name: string) => {
    setShowControls(true);
    setFilter("all");
    setQuery(name);
    setPage(0);
  };
  const needsSignIn = engineState?.loggedIn === false;
  const accountStatus = !online ? "Host offline" : needsSignIn ? "Needs sign-in"
    : engineState?.loggedIn ? "Connected" : "Login not reported";
  return (
    <section className={`host-card connectors-card ${needsSignIn ? "connectors-card-attention" : ""}`} aria-label={`${ENGINE_NAME[engine]} · ${props.accountName}`}>
      <header className="connectors-card-head">
        <span className={`connector-account-icon connector-account-${engine}`} aria-hidden="true">
          <EngineIcon engine={engine} size={18} />
        </span>
        <div className="connector-account-name">
          <strong>{props.accountName}</strong>
          <span className="muted small">{ENGINE_NAME[engine]}{engineState?.account ? ` · ${engineState.account}` : ""}</span>
        </div>
        <span className={`connector-account-state ${needsSignIn ? "warn" : ""}`}>{accountStatus}</span>
        <button className="btn btn-small" onClick={() => actions.go({ page: "hosts", host, tab: "engines" })}>{needsSignIn && online ? "Sign in" : "Manage"}</button>
      </header>
      {!online && <p className="muted small connectors-note">Connect to this host to view or change its connectors.</p>}
      {online && needsSignIn && <p className="muted small connectors-note">Sign in to view this account’s connectors.</p>}
      {online && !needsSignIn && !status?.servers && !status?.error && <p className="muted small connectors-note" role="status">Checking connectors…</p>}
      {online && status?.error && <p className="bad small connectors-note">{status.error}</p>}
      {online && !needsSignIn && status?.servers && (
        <>
          {showControls && (
            <div className="connector-filter-controls">
              <div className="connector-filters" role="group" aria-label="Show connectors">
                {filters.map((f) => (
                  <button key={f.id} className="btn-text btn-small" aria-label={f.label} aria-pressed={filter === f.id} onClick={() => { setFilter(f.id); setPage(0); }}>
                    {f.label}<span className="muted">{filterServers(servers, "", f.id).length}</span>
                  </button>
                ))}
              </div>
              <input className="field connector-search" aria-label="Search connectors" type="search" placeholder="Search connectors" value={query} onChange={(e) => { setQuery(e.target.value); setPage(0); }} />
            </div>
          )}
          {visible.length ? (
            <ul className="connector-list">
              {visible.map((s) => <ServerRow key={s.name} server={s} {...props} />)}
            </ul>
          ) : (
            <div className="connectors-empty muted" role="status">
              {query.trim() ? "No connectors match your search." : servers.length === 0 ? "No connectors for this account yet." : filter === "connected" ? "No connected services yet. Choose Available to connect one, or add a connector below." : filter === "failed" ? "No connectors are reporting a problem." : "No connectors in this view."}
              {query && <button className="btn-text btn-small" onClick={() => { setQuery(""); setPage(0); }}>Clear search</button>}
            </div>
          )}
          {filtered.length > CONNECTOR_PAGE_SIZE && (
            <nav className="connector-pagination" aria-label="Connector pages">
              <span className="muted small" aria-live="polite">{currentPage * CONNECTOR_PAGE_SIZE + 1}–{Math.min((currentPage + 1) * CONNECTOR_PAGE_SIZE, filtered.length)} of {filtered.length}</span>
              <span className="spacer" />
              <button className="btn btn-small" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}>Previous</button>
              <button className="btn btn-small" disabled={currentPage === pages - 1} onClick={() => setPage(currentPage + 1)}>Next</button>
            </nav>
          )}
        </>
      )}
      {online && !needsSignIn && status?.servers && (
        <footer className="connector-card-footer">
          <span className="muted small">{summary(servers) || "No tools connected"}</span>
          <button className="btn-text btn-small connector-filter-toggle" aria-expanded={showControls} onClick={() => {
            if (showControls) { setFilter("all"); setQuery(""); setPage(0); }
            setShowControls(!showControls);
          }}>Filter tools</button>
          <AddConnector {...props} servers={servers} onAdded={reveal} />
        </footer>
      )}
    </section>
  );
}

function ServerRow({ server: s, state, actions, host, engine, account }: { server: McpServer; state: State; actions: Actions; host: string; engine: Engine; account: string }) {
  const { name, via } = displayName(s);
  const auth = state.mcpAuth[mcpAuthKey(host, engine, account, s.name)];
  const action = actionFor(s);
  const signIn = () => actions.mcpAuth(host, engine, account, s.name);
  const change = state.mcpChange[mcpAuthKey(host, engine, account, s.name)];
  const [confirm, setConfirm] = useState(false);
  return (
    <li className="connector">
      <div className="connector-row">
        <span className="connector-id">
          <span className={`dot connector-dot-${s.status}`} aria-hidden="true" />
          <span className="connector-name">{name}</span>
          <span className="muted small connector-via">{via}</span>
          {s.appTools > 0 && (
            <span className="connector-badge" title="Some of its tools come with a widget, such as a chart, shown in the thread">
              widgets
            </span>
          )}
        </span>
        <span className="connector-side">
          {s.status === "connected" && s.tools > 0 && <span className="muted small">{s.tools} {s.tools === 1 ? "tool" : "tools"}</span>}
          <span className={`connector-status connector-status-${s.status}`}>{STATUS_LABEL[s.status] ?? s.status}</span>
          {action === "connect" && !auth && (
            <button className="btn btn-small" onClick={signIn}>
              Connect
            </button>
          )}
          {action === "open" && s.installUrl && (
            <a className="btn btn-small" href={s.installUrl} target="_blank" rel="noreferrer">
              Connect on ChatGPT
            </a>
          )}
          {action === "retry" && (
            <button className="btn btn-small" onClick={() => actions.mcpReconnect(host, engine, account, s.name)}>
              Retry
            </button>
          )}
          {removable(s) &&
            (change?.state === "removing" ? (
              <span className="muted small">Removing…</span>
            ) : (
              <button
                className={`btn-text btn-small ${confirm ? "bad" : ""}`}
                onClick={() => (confirm ? actions.mcpRemove(host, engine, account, s.name) : setConfirm(true))}
                onBlur={() => setConfirm(false)}
                aria-label={confirm ? `Confirm removing ${name}` : `Remove ${name}`}
              >
                {confirm ? "Confirm" : "Remove"}
              </button>
            ))}
        </span>
      </div>
      {change?.state === "failed" && <div className="bad small connector-error">{change.error}</div>}
      {s.status === "failed" && s.error && <div className="muted small connector-error">{shortError(s.error)}</div>}
      {auth && <SignIn host={host} hostLabel={hostName(state, host)} auth={auth} onRetry={signIn} onPaste={(url) => actions.mcpCallback(host, engine, account, s.name, url)} onClose={() => actions.mcpAuthClear(host, engine, account, s.name)} />}
    </li>
  );
}

/** The page is served by the yardd on this very machine (the desktop app, or
 *  a browser on the host itself), so a sign-in redirect to localhost reaches
 *  the CLI waiting for it. Behind a proxy, "local" is a server elsewhere. */
const pageOnLoopback = () => ["127.0.0.1", "localhost", "[::1]"].includes(location.hostname);

function SignIn(props: {
  host: string;
  hostLabel: string;
  auth: State["mcpAuth"][string];
  onRetry(): void;
  onPaste(url: string): void;
  onClose(): void;
}) {
  const { auth } = props;
  const [url, setUrl] = useState("");
  if (auth.state === "starting") return <div className="connector-auth muted small">Getting the sign-in page…</div>;
  if (auth.state === "sending") return <div className="connector-auth muted small">Finishing the sign-in…</div>;
  if (auth.state === "done")
    return (
      <div className="connector-auth connector-auth-done small">
        Connected. Running sessions pick it up now.
        <button className="btn-text" onClick={props.onClose}>
          Dismiss
        </button>
      </div>
    );
  if (auth.state === "failed")
    return (
      <div className="connector-auth small">
        <span className="bad">{auth.error ?? "The sign-in failed."}</span>
        <button className="btn-text" onClick={props.onRetry}>
          Try again
        </button>
        <button className="btn-text" onClick={props.onClose}>
          Dismiss
        </button>
      </div>
    );
  return (
    <div className="connector-auth small">
      <a className="btn-primary btn-small" href={auth.url} target="_blank" rel="noreferrer">
        Open the sign-in page
      </a>
      {auth.paste ? (
        <form
          className="connector-paste"
          onSubmit={(e) => {
            e.preventDefault();
            if (url.trim()) props.onPaste(url.trim());
          }}
        >
          <label className="muted" htmlFor="connector-paste">
            {props.host === "local" && pageOnLoopback()
              ? "When you approve, the page finishes by itself. If it ends on a page that does not load, paste that page's address here."
              : `When you approve, your browser ends on a page that does not load: it is on ${props.hostLabel}, not this computer. Copy that page's address and paste it here.`}
          </label>
          <div className="row">
            <input id="connector-paste" className="field" placeholder="http://localhost:…/callback?code=…" value={url} onChange={(e) => setUrl(e.target.value)} />
            <button className="btn btn-small" disabled={!url.trim()}>
              Finish
            </button>
          </div>
        </form>
      ) : (
        <span className="muted">Connect it on that page. This updates by itself once it is connected.</span>
      )}
      <button className="btn-text" onClick={props.onClose}>
        Cancel
      </button>
    </div>
  );
}

/** "Add connector": a catalog server (Linear first) or any https address,
 *  added to this account's own config; its sign-in opens right after. */
function AddConnector(props: { state: State; actions: Actions; host: string; engine: Engine; account: string; accountName: string; servers: McpServer[]; onAdded(name: string): void }) {
  const { state, actions, host, engine, account } = props;
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const [catalogSearch, setCatalogSearch] = useState("");
  const [service, setService] = useState<{ name: string; id: string; url: string } | null>(null);
  const [other, setOther] = useState(false);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [pending, setPending] = useState<string | null>(null);
  const change = pending ? state.mcpChange[mcpAuthKey(host, engine, account, pending)] : undefined;
  const close = () => {
    setOpen(false);
    setOther(false);
    setCatalogSearch("");
    setService(null);
    setName("");
    setUrl("");
    if (pending) actions.mcpChangeClear(host, engine, account, pending);
    setPending(null);
  };
  // Added: go straight to its sign-in, as Connect would.
  useEffect(() => {
    if (pending && change?.state === "done") {
      props.onAdded(pending);
      actions.mcpAuth(host, engine, account, pending);
      close();
    }
  }, [change?.state]);
  const add = (n: string, u: string) => {
    if (pending) actions.mcpChangeClear(host, engine, account, pending);
    setPending(n);
    actions.mcpAdd(host, engine, account, n, u);
  };
  const missing = missingFromCatalog(props.servers);
  const problem = other && (name || url) ? newServerProblem(name.trim(), url.trim()) : null;
  const busy = change?.state === "adding";
  return (
    <div className="connector-add-slot">
      <button ref={anchor} className="btn-text btn-small connector-add" aria-haspopup="dialog" aria-expanded={open} onClick={() => setOpen(true)}>
        <Icon d={PLUS} size={12} />
        Add connector
      </button>
      {open && (
        <AnchoredPopover anchor={anchor} label="Add connector" variant="connector-add-popover" onClose={close}>
          <div className="popover-heading"><strong>{service ? `Connect ${service.name}` : "Add a connector"}</strong></div>
          {service ? (
            <div className="connector-add-step">
              <p className="muted">Add this tool to {ENGINE_NAME[engine]} · {props.accountName} on {hostName(state, host)}.</p>
              <div className="row">
                <button className="btn-text" disabled={busy} onClick={() => setService(null)}>Back</button>
                <button className="btn-primary" disabled={busy} onClick={() => add(service.id, service.url)}>Connect</button>
              </div>
            </div>
          ) : (
            <div className="connector-add-step">
              <input className="field" type="search" aria-label="Search available connectors" placeholder="Search available connectors" value={catalogSearch} onChange={(event) => setCatalogSearch(event.target.value)} />
              <div className="connector-catalog">
                {missing.filter((entry) => entry.name.toLowerCase().includes(catalogSearch.trim().toLowerCase())).map((entry) => (
                  <button key={entry.id} className="btn-quiet picker-item" disabled={busy} onClick={() => setService(entry)}>
                    <span>{entry.name}</span>{entry.note && <span className="muted small">{entry.note}</span>}
                  </button>
                ))}
                {!missing.some((entry) => entry.name.toLowerCase().includes(catalogSearch.trim().toLowerCase())) && <p className="muted small">No available connectors match.</p>}
                <button className="btn-quiet picker-item" disabled={busy} aria-expanded={other} onClick={() => setOther(!other)}>Other address…</button>
              </div>
              {other && (
                <form className="connector-add-step" onSubmit={(event) => {
                  event.preventDefault();
                  if (!newServerProblem(name.trim(), url.trim())) setService({ name: name.trim(), id: name.trim(), url: url.trim() });
                }}>
                  <input className="field" aria-label="Connector name" placeholder="Name" value={name} onChange={(event) => setName(event.target.value)} />
                  <input className="field" aria-label="Connector address" placeholder="HTTPS address" value={url} onChange={(event) => setUrl(event.target.value)} />
                  {problem && <span className="muted small">{problem}</span>}
                  <button className="btn" disabled={busy || !name.trim() || !url.trim() || !!problem}>Continue</button>
                </form>
              )}
            </div>
          )}
          {busy && <p className="muted small" role="status">Adding {pending}…</p>}
          {change?.state === "failed" && <p className="bad small" role="alert">{change.error}</p>}
          <div className="popover-footer">
            <button className="btn-text" onClick={close}>Cancel</button>
          </div>
        </AnchoredPopover>
      )}
    </div>
  );
}
