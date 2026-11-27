// The Connectors page: one card per service (Linear, Sentry, ...) across every
// engine account on a host, whether it works there, and a way to sign it in.
// Each account's MCP servers (claude.ai connectors, plugins, config files,
// ChatGPT apps, and the set Switchyard holds for OpenCode, Grok and Gemini)
// are merged by service in connectors.ts. Opening the page asks yardd to list
// them; for Claude that starts Claude Code with no prompt, so it spends no usage.
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { EngineIcon } from "./EngineIcon";
import { SelectPicker } from "./SelectPicker";
import { accountChoices, DEFAULT_ACCOUNT } from "./accounts";
import {
  accountKey, accountText, availableToAdd, connectorOn, displayName, entryFor, groupServices, knownFor, newServerProblem, pickableConnectors, removable, searchServices, sectionOf,
  serviceAccounts, serviceId, shortError, STATUS_LABEL, targetFor, whereText,
  type ConnectorAccount, type ConnectTarget, type Service, type ServiceSection,
} from "./connectors";
import { BRAND_MARKS } from "./connectorBrands";
import type { Agent, Automation, Engine, McpAuthState } from "./protocol";
import { accountsOn, hostName, hostOnline, mcpAuthKey, mcpKey, type Actions, type State } from "./store";
import { CHEVRON_RIGHT, Icon, MenuButton, PLUS, REFRESH, useEscape } from "./App";
import { AgentAvatar } from "./AgentsPage";
import { loadHost } from "./sessionDefaults";
import { ENGINE_NAME } from "./engines";
import { SlackCard } from "./SlackCard";

const SEARCH = "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14ZM20 20l-3.5-3.5";
const CLOSE = "M18 6 6 18M6 6l12 12";
const CHECK = "M5 12.5l4.5 4.5L19 7.5";
const OUT = "M14 5h5v5M19 5l-8 8M11 6H6v12h12v-5";
const CLOCK = "M12 7v5l3 2M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z";

/** Engines that run their own MCP sign-ins; each account has its own list. */
const OWN: Engine[] = ["claude", "codex"];
/** Engines Switchyard holds connectors for: one set per host. */
const HELD: Engine[] = ["opencode", "grok", "gemini"];

/** A sign-in the page is walking through: one service on one or more accounts,
 *  one after another. Kept here, not in the card, so the card can move between
 *  sections (Available to Connected) without losing it. */
interface Flow {
  service: Service;
  /** Where the card stays until the flow is dismissed. */
  section: ServiceSection;
  targets: ConnectTarget[];
  /** "pick": choosing accounts; "run": at `targets[at]`; "done": all through. */
  phase: "pick" | "run" | "done";
  at: number;
  /** What the current target is waiting on. */
  step?: "add" | "auth";
  /** Account keys left out while picking. */
  skipped: string[];
}

export function ConnectorsPage({ state, actions, usage }: { state: State; actions: Actions; usage: ReactNode }) {
  const hosts = ["local", ...state.hosts.map((h) => h.id)];
  const [pickedHost, setHost] = useState(loadHost);
  const host = hosts.includes(pickedHost) ? pickedHost : hosts[0];
  const online = hostOnline(state, host);
  const engines = state.engines[host];

  const accounts = useMemo<ConnectorAccount[]>(() => {
    const own = OWN.filter((engine) => engines?.[engine]?.installed !== false).flatMap((engine) =>
      accountChoices(accountsOn(state, host), engine).map((a) => {
        const login = a.id === DEFAULT_ACCOUNT ? engines?.[engine] : engines?.accounts?.[a.id];
        return { engine, account: a.id, name: a.name, engineName: ENGINE_NAME[engine], signedOut: login?.loggedIn === false };
      }),
    );
    const held = HELD.filter((engine) => engines?.[engine]?.installed);
    return held.length ? [...own, { engine: "switchyard" as const, account: DEFAULT_ACCOUNT, name: held.map((e) => ENGINE_NAME[e]).join(", "), engineName: held.map((e) => ENGINE_NAME[e]).join(" · ") }] : own;
  }, [engines, state.accounts[host]]);

  const [scope, setScope] = useState("all");
  const shown = accounts.filter((a) => scope === "all" || accountKey(a) === scope);
  const keyOf = (a: ConnectorAccount) => mcpKey(host, a.engine, a.account);

  // Ask for each listable account once per visit, one at a time, and again on "Check again".
  const [recheck, setRecheck] = useState<string[]>([]);
  const listable = accounts.filter((a) => !a.signedOut);
  const loading = listable.some((a) => state.mcp[keyOf(a)]?.loading);
  const pending = listable.find((a) => !state.mcp[keyOf(a)] || recheck.includes(keyOf(a)));
  useEffect(() => {
    if (!online || loading || !pending) return;
    actions.mcpStatus(host, pending.engine, pending.account);
    setRecheck((keys) => keys.filter((k) => k !== keyOf(pending)));
  }, [actions, host, loading, online, pending && keyOf(pending)]);

  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [flows, setFlows] = useState<Record<string, Flow>>({});
  useEffect(() => {
    setFlows({});
    setOpen(null);
    setScope("all");
  }, [host]);

  const listed = groupServices(shown.map((account) => ({ account, servers: state.mcp[keyOf(account)]?.servers ?? [] })));
  // A flow's service that no account lists yet (just added, or a typed
  // address) still has its card, where the flow started.
  const services = [...listed, ...[...availableToAdd(listed), ...Object.values(flows).map((f) => f.service)].filter((s, i, all) => !listed.some((l) => l.id === s.id) && all.findIndex((x) => x.id === s.id) === i)];
  const found = searchServices(services, query);
  const sectionFor = (s: Service): ServiceSection => flows[s.id]?.section ?? (s.entries.length ? sectionOf(s) : "available");
  const inSection = (section: ServiceSection) => found.filter((s) => sectionFor(s) === section);

  // Only where the account's servers are known: one that could not be listed might have it already.
  const startable = (s: Service) => shown.filter((a) => state.mcp[keyOf(a)]?.servers).map((a) => targetFor(s, a)).filter((t): t is ConnectTarget => !!t && (t.kind === "auth" || t.kind === "add"));
  const start = (s: Service, only?: ConnectorAccount) => {
    const targets = startable(s).filter((t) => !only || accountKey(t.account) === accountKey(only));
    if (!targets.length) return;
    setFlows((f) => ({ ...f, [s.id]: { service: s, section: sectionFor(s), targets, phase: targets.length > 1 ? "pick" : "run", at: 0, skipped: [] } }));
  };
  const patch = (id: string, change: Partial<Flow> | null) =>
    setFlows((f) => {
      if (!f[id]) return f;
      if (!change) {
        const { [id]: _gone, ...rest } = f;
        return rest;
      }
      return { ...f, [id]: { ...f[id], ...change } };
    });

  // Walk every running flow: add the server where it is missing, sign it in, then the next account.
  useEffect(() => {
    for (const [id, flow] of Object.entries(flows)) {
      if (flow.phase !== "run") continue;
      const t = flow.targets[flow.at];
      if (!t) {
        patch(id, { phase: "done", step: undefined });
        continue;
      }
      const key = mcpAuthKey(host, t.account.engine, t.account.account, t.server);
      if (!flow.step) {
        if (t.kind === "add") actions.mcpAdd(host, t.account.engine, t.account.account, t.server, t.url);
        else actions.mcpAuth(host, t.account.engine, t.account.account, t.server);
        patch(id, { step: t.kind === "add" ? "add" : "auth" });
      } else if (flow.step === "add" && state.mcpChange[key]?.state === "done") {
        actions.mcpChangeClear(host, t.account.engine, t.account.account, t.server);
        actions.mcpAuth(host, t.account.engine, t.account.account, t.server);
        patch(id, { step: "auth" });
      } else if (flow.step === "auth" && state.mcpAuth[key]?.state === "done") {
        actions.mcpAuthClear(host, t.account.engine, t.account.account, t.server);
        patch(id, flow.at + 1 < flow.targets.length ? { at: flow.at + 1, step: undefined } : { phase: "done", step: undefined });
      }
    }
  }, [flows, state.mcpAuth, state.mcpChange, host]);

  const flowProps = (s: Service) => ({
    flow: flows[s.id],
    state,
    actions,
    host,
    onPick: (key: string) => patch(s.id, { skipped: flows[s.id].skipped.includes(key) ? flows[s.id].skipped.filter((k) => k !== key) : [...flows[s.id].skipped, key] }),
    onContinue: () => patch(s.id, { phase: "run", targets: flows[s.id].targets.filter((t) => !flows[s.id].skipped.includes(accountKey(t.account))) }),
    onRetry: () => patch(s.id, { step: undefined }),
    onSkip: () => patch(s.id, flows[s.id].at + 1 < flows[s.id].targets.length ? { at: flows[s.id].at + 1, step: undefined } : { phase: "done", step: undefined }),
    onClose: () => {
      const f = flows[s.id];
      const t = f?.targets[f.at];
      if (t && f.phase === "run") {
        actions.mcpAuthClear(host, t.account.engine, t.account.account, t.server);
        actions.mcpChangeClear(host, t.account.engine, t.account.account, t.server);
      }
      patch(s.id, null);
    },
  });
  const cardProps = (s: Service) => ({ service: s, accounts: shown, ...flowProps(s), targets: startable(s), onConnect: () => start(s), onOpen: () => setOpen(s.id), onReconnect: (t: ConnectTarget) => actions.mcpReconnect(host, t.account.engine, t.account.account, t.server) });

  const refresh = (
    <button className="btn-icon cx-refresh" aria-label={loading ? "Checking…" : "Check again"} title={loading ? "Checking…" : "Check again"} disabled={!online || loading || !listable.length} onClick={() => setRecheck(listable.map(keyOf))}>
      <Icon d={REFRESH} size={15} />
    </button>
  );
  const checking = online && listable.some((a) => !state.mcp[keyOf(a)]?.servers && !state.mcp[keyOf(a)]?.error);
  const needs = inSection("needs");
  const connected = inSection("connected");
  const available = inSection("available");
  const opened = open ? services.find((s) => s.id === open) : undefined;
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
          <div className="cx-head">
            <div className="cx-head-text">
              <h1>Connectors</h1>
              <p className="muted">The services your sessions and agents can use. Connect one once per account and every session on that account has it.</p>
            </div>
            {hosts.length > 1 && <SelectPicker label="Connector host" value={host} onChange={setHost} options={hosts.map((id) => ({ value: id, label: hostName(state, id) }))} />}
            {refresh}
          </div>
          <div className="cx-tools">
            {accounts.length > 1 && (
              <div className="cx-scope" role="group" aria-label="Connectors by account">
                <button className="cx-chip" aria-pressed={scope === "all"} onClick={() => setScope("all")}>All accounts</button>
                {accounts.map((a) => (
                  <button key={accountKey(a)} className="cx-chip" aria-pressed={scope === accountKey(a)} onClick={() => setScope(accountKey(a))}>
                    <AccountMark account={a} size={13} />
                    {scopeLabel(a, accounts)}
                  </button>
                ))}
              </div>
            )}
            <label className="cx-search">
              <Icon d={SEARCH} size={14} />
              <input className="field" type="search" aria-label="Search connectors" placeholder="Search connectors" value={query} onChange={(e) => setQuery(e.target.value)} />
            </label>
          </div>

          {!online && <p className="cx-notice muted" role="status">Connect to {hostName(state, host)} to view or change its connectors.</p>}
          {online && shown.filter((a) => a.signedOut).map((a) => (
            <p key={accountKey(a)} className="cx-notice cx-notice-warn" role="status">
              <span>{accountText(a)} is logged out on {hostName(state, host)}, so its connectors cannot be listed.</span>
              <button className="btn btn-small" onClick={() => actions.go({ page: "hosts", host, tab: "engines" })}>Log in</button>
            </p>
          ))}
          {online && shown.filter((a) => state.mcp[keyOf(a)]?.error && !state.mcp[keyOf(a)]?.servers).map((a) =>
            // A yardd from before it held connectors itself refuses the engine.
            a.engine === "switchyard" && /listed for Claude and Codex|unknown engine/i.test(state.mcp[keyOf(a)]!.error!) ? (
              <p key={accountKey(a)} className="cx-notice muted" role="status">Update Switchyard on {hostName(state, host)} to connect services for {a.name}.</p>
            ) : (
              <p key={accountKey(a)} className="cx-notice bad" role="alert">{accountText(a)}: {shortError(state.mcp[keyOf(a)]!.error)}</p>
            ),
          )}
          {checking && !services.some((s) => s.entries.length) && <p className="cx-notice muted" role="status">Checking connectors…</p>}

          {needs.length > 0 && (
            <Section title="Needs you" count={needs.length}>
              <div className="cx-grid">{needs.map((s) => <ServiceCard key={s.id} {...cardProps(s)} />)}</div>
            </Section>
          )}
          {connected.length > 0 && (
            <Section title="Connected" count={connected.length}>
              <div className="cx-grid">{connected.map((s) => <ServiceCard key={s.id} {...cardProps(s)} />)}</div>
            </Section>
          )}
          {online && (available.length > 0 || !query.trim()) && (
            <Section title="Available" note="Each signs in on the service’s own page.">
              <div className="cx-grid">
                {available.map((s) => (flows[s.id] ? <ServiceCard key={s.id} {...cardProps(s)} /> : <AvailableCard key={s.id} {...cardProps(s)} />))}
                {!query.trim() && shown.some((a) => !a.signedOut) && (
                  <div className="cx-avail cx-avail-own">
                    <span className="cx-tile cx-tile-md cx-tile-empty" aria-hidden="true"><Icon d={PLUS} size={16} /></span>
                    <span className="cx-avail-text"><strong>Your own server</strong><span className="muted small">Any https MCP address</span></span>
                    <button className="btn btn-small" aria-haspopup="dialog" onClick={() => setAdding(true)}>Add</button>
                  </div>
                )}
              </div>
            </Section>
          )}
          {query.trim() && !found.length && (
            <div className="cx-empty muted" role="status">
              No connectors match your search.
              <button className="btn-text btn-small" onClick={() => setQuery("")}>Clear search</button>
            </div>
          )}

          {state.slack && (
            <Section title="Messaging" note="Your own Slack, and who can talk to your agents there.">
              <div className="connector-apps"><SlackCard state={state} actions={actions} /></div>
            </Section>
          )}
        </div>
      </div>
      {opened && (
        <ServiceSheet
          service={opened} accounts={shown.filter((a) => a.signedOut || state.mcp[keyOf(a)]?.servers)} state={state} actions={actions} host={host} {...flowProps(opened)}
          onConnect={(a) => start(opened, a)} onDone={() => setOpen(null)} onRecheck={() => setRecheck(serviceAccounts(opened).map(keyOf))}
        />
      )}
      {adding && (
        <OwnServer
          accounts={shown.filter((a) => !a.signedOut)} taken={services} onClose={() => setAdding(false)}
          onAdd={(name, url, account) => {
            const known = knownFor({ name, source: "user" });
            const service: Service = { id: known?.id ?? serviceId(name), name: known?.name ?? name, about: known?.about ?? "", brand: known?.brand, known, entries: [] };
            setFlows((f) => ({ ...f, [service.id]: { service, section: "available", targets: [{ kind: "add", account, server: name, url }], phase: "run", at: 0, skipped: [] } }));
            setAdding(false);
          }}
        />
      )}
    </div>
  );
}

/** "Personal" where the engine's mark says the rest; "Codex" for an engine's only account. */
function scopeLabel(a: ConnectorAccount, all: ConnectorAccount[]): string {
  if (a.engine === "switchyard") return a.engineName;
  return all.filter((b) => b.engine === a.engine).length > 1 ? a.name : a.engineName;
}

function Section({ title, count, note, children }: { title: string; count?: number; note?: string; children: ReactNode }) {
  return (
    <section className="cx-section" aria-label={title}>
      <div className="cx-section-head">
        <h2>{title}</h2>
        {count != null && <span className="muted cx-count">{count}</span>}
        {note && <p className="muted small">{note}</p>}
      </div>
      {children}
    </section>
  );
}

/** A service's face: its brand mark on a tint of its colour, or its initial. */
export function ServiceTile({ service, size = "lg" }: { service: Pick<Service, "name" | "brand" | "id">; size?: "sm" | "md" | "lg" | "xl" }) {
  const mark = service.brand ? BRAND_MARKS[service.brand] : undefined;
  if (!mark) {
    // The same service keeps its colour: picked from its id.
    const tint = [...service.id].reduce((n, c) => (n * 31 + c.charCodeAt(0)) % 8, 0);
    return <span className={`cx-tile cx-tile-${size} cx-tile-letter cx-tint-${tint}`} aria-hidden="true">{(service.name.match(/[A-Za-z0-9]/)?.[0] ?? "?").toUpperCase()}</span>;
  }
  return (
    <span className={`cx-tile cx-tile-${size} cx-brand-${service.brand}`} aria-hidden="true">
      <svg viewBox="0 0 24 24" fill="currentColor"><path d={mark} /></svg>
    </span>
  );
}

/** The engine's mark; Switchyard's set shows the first engine it serves. */
function AccountMark({ account, size }: { account: ConnectorAccount; size: number }) {
  const engine: Engine = account.engine === "switchyard" ? (HELD.find((e) => account.engineName.startsWith(ENGINE_NAME[e])) ?? "opencode") : account.engine;
  return <EngineIcon engine={engine} size={size} />;
}

/** Where a service works: one mark per account. Solid: connected. A dot:
 *  listed there but signed out or broken. Dashed: not there. */
function AccountMarks({ service, accounts }: { service: Service; accounts: ConnectorAccount[] }) {
  if (accounts.length < 2) return null;
  return (
    <span className="cx-who">
      {accounts.map((a) => {
        const status = entryFor(service, a)?.server.status;
        const tone = status === "connected" ? "on" : status === "failed" ? "bad" : status === "needs-auth" ? "warn" : "off";
        const says = status ? (STATUS_LABEL[status] ?? status).toLowerCase() : "not added";
        return (
          <span key={accountKey(a)} className={`cx-acct cx-acct-${tone}`} title={`${accountText(a)}: ${says}`}>
            <AccountMark account={a} size={12} />
            <span className="sr-only">{accountText(a)}: {says}</span>
          </span>
        );
      })}
    </span>
  );
}

interface FlowProps {
  flow?: Flow;
  state: State;
  actions: Actions;
  host: string;
  onPick(key: string): void;
  onContinue(): void;
  onRetry(): void;
  onSkip(): void;
  onClose(): void;
}

interface CardProps extends FlowProps {
  service: Service;
  accounts: ConnectorAccount[];
  targets: ConnectTarget[];
  onConnect(): void;
  onOpen(): void;
  onReconnect(target: ConnectTarget): void;
}

function ServiceCard(props: CardProps) {
  const { service, accounts, flow } = props;
  const section = flow?.section ?? sectionOf(service);
  const broken = accounts.map((a) => entryFor(service, a)).find((e) => e?.server.status === "failed");
  const retry = broken && targetFor(service, broken.account);
  const widgets = service.entries.some((e) => e.server.appTools > 0);
  const state = flow && flow.phase !== "done" ? "busy" : service.entries.some((e) => e.server.status === "connected") && !broken ? "ok" : broken ? "bad" : service.entries.some((e) => e.server.status === "pending") ? "busy" : null;
  const label = state === "busy" ? (flow ? "Signing in" : "Connecting") : state === "ok" ? "Connected" : state === "bad" ? "Not working" : null;
  const own = service.entries.length > 0 && !service.known;
  return (
    <article className={`cx-card ${section === "needs" ? "cx-card-bad" : ""}`} aria-label={service.name}>
      <div className="cx-top">
        <ServiceTile service={service} />
        {label && <span className={`cx-state cx-state-${state}`}>{label}</span>}
      </div>
      <h3 className="cx-name">
        <button className="cx-open" onClick={props.onOpen} aria-haspopup="dialog">{service.name}</button>
        {widgets && <span className="connector-badge" title="Some of its tools come with a widget, such as a chart, shown in the thread">widgets</span>}
      </h3>
      {!flow && (broken?.server.error ? <p className="cx-err">{shortError(broken.server.error)}</p> : <p className="cx-desc muted">{service.about || (own ? `${displayName(service.entries[0].server).via}${service.entries[0].server.origin ? ` · ${hostOf(service.entries[0].server.origin)}` : ""}` : "")}</p>)}
      {flow && <FlowPanel {...props} />}
      {(!flow || flow.phase === "done") && (
        <div className="cx-foot">
          <AccountMarks service={service} accounts={accounts} />
          <span className="cx-where muted small">{broken ? displayName(broken.server).via : whereText(service, accounts)}</span>
          <span className="spacer" />
          {retry?.kind === "retry" && !flow && <button className="btn btn-small" onClick={() => props.onReconnect(retry)}>Retry</button>}
          {!retry && !flow && <span className="cx-chev" aria-hidden="true"><Icon d={CHEVRON_RIGHT} size={14} /></span>}
        </div>
      )}
    </article>
  );
}

const hostOf = (origin: string) => origin.replace(/^https?:\/\//, "");

function AvailableCard({ service, targets, onConnect, onOpen, accounts }: CardProps) {
  const link = accounts.map((a) => targetFor(service, a)).find((t) => t?.kind === "open");
  const note = service.known?.note;
  return (
    <div className="cx-avail" role="group" aria-label={service.name}>
      <ServiceTile service={service} size="md" />
      <span className="cx-avail-text">
        <strong><button className="cx-open" onClick={onOpen} aria-haspopup="dialog">{service.name}</button></strong>
        <span className="muted small">{service.about || (service.entries[0] ? displayName(service.entries[0].server).via : "")}{note ? ` · ${note}` : ""}</span>
      </span>
      {targets.length > 0 ? (
        <button className="btn btn-small" onClick={onConnect} aria-label={`Connect ${service.name}`}>Connect</button>
      ) : link?.kind === "open" ? (
        <a className="btn btn-small" href={link.url} target="_blank" rel="noreferrer" aria-label={`Connect ${service.name} on ChatGPT`}>Open <Icon d={OUT} size={11} /></a>
      ) : null}
    </div>
  );
}

/** The page is served by the yardd on this very machine (the desktop app, or
 *  a browser on the host itself), so a sign-in redirect to localhost reaches
 *  the CLI waiting for it. Behind a proxy, "local" is a server elsewhere. */
const pageOnLoopback = () => ["127.0.0.1", "localhost", "[::1]"].includes(location.hostname);

/** A flow's current step, drawn inside the card or sheet that started it. */
function FlowPanel({ flow, state, actions, host, onPick, onContinue, onSkip, onClose, onRetry }: FlowProps) {
  if (!flow) return null;
  if (flow.phase === "pick") {
    const left = flow.targets.filter((t) => !flow.skipped.includes(accountKey(t.account)));
    return (
      <div className="cx-pick">
        <span className="muted small">Connect for</span>
        {flow.targets.map((t) => (
          <label key={accountKey(t.account)} className="check">
            <input type="checkbox" checked={!flow.skipped.includes(accountKey(t.account))} onChange={() => onPick(accountKey(t.account))} />
            <AccountMark account={t.account} size={13} />
            <span>{accountText(t.account)}</span>
          </label>
        ))}
        <div className="cx-flow-actions">
          <button className="btn-text btn-small" onClick={onClose}>Cancel</button>
          <span className="spacer" />
          <button className="btn-primary btn-small" disabled={!left.length} onClick={onContinue}>Continue</button>
        </div>
      </div>
    );
  }
  if (flow.phase === "done") {
    return (
      <div className="cx-flow cx-flow-done" role="status">
        <Icon d={CHECK} size={13} />
        <span>Connected. Running sessions have it now.</span>
        <button className="btn-text btn-small" onClick={onClose}>Dismiss</button>
      </div>
    );
  }
  const t = flow.targets[flow.at];
  const key = mcpAuthKey(host, t.account.engine, t.account.account, t.server);
  const change = state.mcpChange[key];
  const auth = state.mcpAuth[key];
  const many = flow.targets.length > 1;
  const who = many ? <span className="muted small cx-flow-who">{accountText(t.account)} · {flow.at + 1} of {flow.targets.length}</span> : null;
  const failed = change?.state === "failed" ? change.error : auth?.state === "failed" ? (auth.error ?? "The sign-in failed.") : null;
  if (failed) {
    return (
      <div className="cx-flow" role="alert">
        {who}
        <span className="bad">{shortError(failed)}</span>
        <div className="cx-flow-actions">
          <button className="btn btn-small" onClick={() => { actions.mcpAuthClear(host, t.account.engine, t.account.account, t.server); actions.mcpChangeClear(host, t.account.engine, t.account.account, t.server); onRetry(); }}>Try again</button>
          {many && flow.at + 1 < flow.targets.length && <button className="btn-text btn-small" onClick={onSkip}>Skip this account</button>}
          <button className="btn-text btn-small" onClick={onClose}>Dismiss</button>
        </div>
      </div>
    );
  }
  if (!auth || auth.state === "starting" || flow.step === "add") {
    return <div className="cx-flow muted" role="status">{who}<span className="cx-spin-line"><span className="cx-spin" aria-hidden="true" />{flow.step === "add" ? `Adding ${flow.service.name}…` : "Getting the sign-in page…"}</span></div>;
  }
  if (auth.state === "sending") return <div className="cx-flow muted" role="status">{who}<span className="cx-spin-line"><span className="cx-spin" aria-hidden="true" />Finishing the sign-in…</span></div>;
  return (
    <div className="cx-flow">
      {who}
      <SignIn auth={auth} name={flow.service.name} host={host} hostLabel={hostName(state, host)} onPaste={(url) => actions.mcpCallback(host, t.account.engine, t.account.account, t.server, url)} onClose={onClose} />
    </div>
  );
}

/** A sign-in that has a page to visit: the link, and for a host the browser
 *  cannot reach, a place to paste the address it ends on. */
export function SignIn({ auth, name, host, hostLabel, onPaste, onClose }: { auth: McpAuthState; name: string; host: string; hostLabel: string; onPaste(url: string): void; onClose(): void }) {
  const [url, setUrl] = useState("");
  return (
    <>
      <span>
        {auth.paste
          ? host === "local" && pageOnLoopback()
            ? `Approve Switchyard on ${name}’s page. It finishes by itself; if it ends on a page that does not load, paste that page’s address here.`
            : `Approve Switchyard on ${name}’s page. Your browser then ends on a page that does not load: it is on ${hostLabel}, not this computer. Paste that page’s address here.`
          : `Connect it on ${name}’s page. This updates by itself once it is connected.`}
      </span>
      {auth.paste && (
        <form
          className="cx-paste connector-paste"
          onSubmit={(e) => {
            e.preventDefault();
            if (url.trim()) onPaste(url.trim());
          }}
        >
          <input className="field" aria-label="Address of the page that did not load" placeholder="http://localhost:…/callback?code=…" value={url} onChange={(e) => setUrl(e.target.value)} />
          <button className="btn btn-small" disabled={!url.trim()}>Finish</button>
        </form>
      )}
      <div className="cx-flow-actions">
        <a className="btn-primary btn-small" href={auth.url} target="_blank" rel="noreferrer">Open the sign-in page <Icon d={OUT} size={11} /></a>
        <button className="btn-text btn-small" onClick={onClose}>Cancel</button>
      </div>
    </>
  );
}

/** Inside a card: how the service stands on each account, which agents and
 *  automations may use it, and taking it out. */
function ServiceSheet(props: FlowProps & { service: Service; accounts: ConnectorAccount[]; onConnect(account: ConnectorAccount): void; onDone(): void; onRecheck(): void }) {
  const { service, accounts, state, actions, host, flow } = props;
  useEscape(true, props.onDone);
  const sheet = useRef<HTMLElement>(null);
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    sheet.current?.focus();
    return () => before?.focus?.();
  }, []);
  const tools = Math.max(0, ...service.entries.map((e) => e.server.tools));
  const origin = service.entries.find((e) => e.server.origin)?.server.origin ?? (service.known?.url ? new URL(service.known.url).origin : undefined);
  const facts = [origin && hostOf(origin), tools > 0 && `${tools} ${tools === 1 ? "tool" : "tools"}`, service.entries.some((e) => e.server.appTools > 0) && "widgets"].filter(Boolean);
  return (
    <div className="cx-sheet-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onDone()}>
      <aside ref={sheet} className="cx-sheet" role="dialog" aria-modal="true" aria-label={service.name} tabIndex={-1}>
        <header className="cx-sheet-head">
          <ServiceTile service={service} size="xl" />
          <div className="cx-sheet-title">
            <h2>{service.name}</h2>
            {service.about && <p className="muted">{service.about}</p>}
            {facts.length > 0 && <p className="muted small">{facts.join(" · ")}</p>}
          </div>
          <button className="btn-icon" aria-label={`Close ${service.name}`} onClick={props.onDone}><Icon d={CLOSE} size={14} /></button>
        </header>
        <div className="cx-sheet-scroll">
          <section className="cx-sheet-sec" aria-label="Accounts">
            <h3>Accounts on {hostName(state, host)}</h3>
            <p className="muted small">Each account signs in on its own. A session uses the account it runs on.</p>
            <ul className="cx-rows">
              {accounts.map((a) => <AccountRow key={accountKey(a)} account={a} service={service} state={state} actions={actions} host={host} busy={!!flow && flow.phase !== "done"} onConnect={() => props.onConnect(a)} />)}
            </ul>
            {flow && <FlowPanel {...props} />}
          </section>
          <WhoMayUse service={service} state={state} actions={actions} host={host} />
        </div>
        <footer className="cx-sheet-foot">
          <button className="btn-text btn-small" onClick={props.onRecheck}>Check again</button>
        </footer>
      </aside>
    </div>
  );
}

function AccountRow({ account, service, state, actions, host, busy, onConnect }: { account: ConnectorAccount; service: Service; state: State; actions: Actions; host: string; busy: boolean; onConnect(): void }) {
  const entry = entryFor(service, account);
  const target = targetFor(service, account);
  const [confirm, setConfirm] = useState(false);
  const change = entry ? state.mcpChange[mcpAuthKey(host, account.engine, account.account, entry.server.name)] : undefined;
  const status = entry?.server.status;
  const via = account.signedOut ? "logged out" : entry ? displayName(entry.server).via : "not added";
  return (
    <li className="cx-row">
      <span className="cx-row-mark"><AccountMark account={account} size={15} /></span>
      <span className="cx-row-name">
        <strong>{account.engine === "switchyard" || account.account === DEFAULT_ACCOUNT ? account.engineName : account.name}</strong>
        <span className="muted small">{account.engine === "switchyard" ? "held by Switchyard" : account.account === DEFAULT_ACCOUNT ? via : `${account.engineName} · ${via}`}</span>
        {status === "failed" && entry?.server.error && <span className="bad small">{shortError(entry.server.error)}</span>}
        {change?.state === "failed" && <span className="bad small">{change.error}</span>}
      </span>
      {status && status !== "needs-auth" && <span className={`cx-row-state small cx-row-${status}`}>{STATUS_LABEL[status] ?? status}</span>}
      {(target?.kind === "auth" || target?.kind === "add") && <button className="btn btn-small" disabled={busy} onClick={onConnect} aria-label={`Connect ${service.name} for ${accountText(account)}`}>Connect</button>}
      {target?.kind === "open" && <a className="btn btn-small" href={target.url} target="_blank" rel="noreferrer">Connect on ChatGPT</a>}
      {target?.kind === "retry" && <button className="btn btn-small" onClick={() => actions.mcpReconnect(host, account.engine, account.account, target.server)}>Retry</button>}
      {entry && removable(entry.server) &&
        (change?.state === "removing" ? (
          <span className="muted small">Removing…</span>
        ) : (
          <button
            className={`btn-text btn-small ${confirm ? "bad" : ""}`}
            // Remove turns into Confirm in this same button, so a double
            // click's second click (detail 2) lands on it: not the
            // confirmation asked for. A separate click still removes.
            onClick={(e) => {
              if (!confirm) setConfirm(true);
              else if (e.detail < 2) actions.mcpRemove(host, account.engine, account.account, entry.server.name);
            }}
            onBlur={() => setConfirm(false)}
            aria-label={confirm ? `Confirm removing ${service.name} from ${accountText(account)}` : `Remove ${service.name} from ${accountText(account)}`}
          >
            {confirm ? "Confirm" : "Remove"}
          </button>
        ))}
    </li>
  );
}

/** The account whose connectors an agent's or automation's sessions get. */
const accountOfRun = (engine: Engine, account: string | undefined) =>
  OWN.includes(engine) ? { engine, account: account || DEFAULT_ACCOUNT } : { engine: "switchyard" as const, account: DEFAULT_ACCOUNT };

/** Which agents and automations on this host may use the service: the same
 *  `mcp` list their own forms save, seen from the connector's side. */
function WhoMayUse({ service, state, actions, host }: { service: Service; state: State; actions: Actions; host: string }) {
  const [saving, setSaving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const agents = state.agents.filter((a) => (a.host ?? "local") === host && !a.setup);
  const automations = state.automations[host] ?? [];
  if (!agents.length && !automations.length) return null;
  const row = (id: string, who: { engine: Engine; account?: string; mcp?: string[] | null }, save: (mcp: string[]) => Promise<{ error?: string }>) => {
    const acct = accountOfRun(who.engine, who.account);
    const names = service.entries.filter((e) => accountKey(e.account) === accountKey(acct)).map((e) => e.server.name);
    const picked = who.mcp ?? null;
    const on = names.some((n) => connectorOn(picked, n));
    const toggle = async () => {
      const all = pickableConnectors(state.mcp[mcpKey(host, acct.engine, acct.account)]?.servers, picked);
      const now = picked ?? all.filter((n) => connectorOn(null, n));
      setSaving(id);
      setError(null);
      const r = await save(on ? now.filter((n) => !names.includes(n)) : [...now, ...names.filter((n) => !now.includes(n))]);
      setSaving(null);
      if (r.error) setError(r.error);
    };
    return { names, on, toggle };
  };
  return (
    <section className="cx-sheet-sec" aria-label="Who may use it">
      <h3>Who may use it</h3>
      <p className="muted small">Your own sessions always can. Agents and automations only when switched on here.</p>
      <ul className="cx-rows">
        {agents.map((a: Agent) => {
          const r = row(a.id, a, (mcp) => actions.saveAgent({ id: a.id, mcp }));
          return (
            <li key={a.id} className="cx-row">
              <AgentAvatar agent={a} size="sm" still />
              <span className="cx-row-name"><strong>{a.name}</strong><span className="muted small">{r.names.length ? "Agent" : "Agent · not on its account"}</span></span>
              <button className="switch" role="switch" aria-checked={r.on} aria-label={`${a.name} may use ${service.name}`} disabled={!r.names.length || saving === a.id} onClick={r.toggle}><span className="switch-knob" /></button>
            </li>
          );
        })}
        {automations.map((a: Automation) => {
          const r = row(a.id, a, (mcp) => actions.saveAutomation(host, { ...a, mcp }));
          return (
            <li key={a.id} className="cx-row">
              <span className="cx-row-mark"><Icon d={CLOCK} size={14} /></span>
              <span className="cx-row-name"><strong>{a.name}</strong><span className="muted small">{r.names.length ? "Automation" : "Automation · not on its account"}</span></span>
              <button className="switch" role="switch" aria-checked={r.on} aria-label={`${a.name} may use ${service.name}`} disabled={!r.names.length || saving === a.id} onClick={r.toggle}><span className="switch-knob" /></button>
            </li>
          );
        })}
      </ul>
      {error && <p className="bad small" role="alert">{error}</p>}
    </section>
  );
}

/** "Your own server": any https MCP address under a name, added to one
 *  account's config; its sign-in opens right after, in its new card. */
function OwnServer({ accounts, taken, onAdd, onClose }: { accounts: ConnectorAccount[]; taken: Service[]; onAdd(name: string, url: string, account: ConnectorAccount): void; onClose(): void }) {
  useEscape(true, onClose);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [where, setWhere] = useState(accounts[0] ? accountKey(accounts[0]) : "");
  const account = accounts.find((a) => accountKey(a) === where) ?? accounts[0];
  const problem = name || url ? (newServerProblem(name.trim(), url.trim(), account?.engine === "switchyard") ?? (taken.some((s) => s.entries.some((e) => accountKey(e.account) === where && e.server.name.toLowerCase() === name.trim().toLowerCase())) ? "That account already has a connector with this name." : null)) : null;
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <form
        className="modal cx-own" role="dialog" aria-modal="true" aria-label="Add your own server"
        onSubmit={(e) => {
          e.preventDefault();
          if (account && name.trim() && url.trim() && !problem) onAdd(name.trim(), url.trim(), account);
        }}
      >
        <div className="modal-head"><h2>Add your own server</h2></div>
        <p className="muted">A remote MCP server that signs in with OAuth. It is added to one account’s own config, then signed in.</p>
        <input className="field" aria-label="Connector name" placeholder="Name" autoFocus value={name} onChange={(e) => setName(e.target.value)} />
        <input className="field" aria-label="Connector address" placeholder="https://…" value={url} onChange={(e) => setUrl(e.target.value)} />
        {accounts.length > 1 && (
          <div className="cx-own-where">
            <span className="muted small">Add it to</span>
            <SelectPicker label="Account for the new connector" value={where} onChange={setWhere} options={accounts.map((a) => ({ value: accountKey(a), label: accountText(a) }))} />
          </div>
        )}
        {problem && <span className="muted small" role="status">{problem}</span>}
        <div className="row">
          <span className="spacer" />
          <button type="button" className="btn-text" onClick={onClose}>Cancel</button>
          <button className="btn-primary" disabled={!account || !name.trim() || !url.trim() || !!problem}>Add and sign in</button>
        </div>
      </form>
    </div>
  );
}
