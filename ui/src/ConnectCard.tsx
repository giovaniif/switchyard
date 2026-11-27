// The connect card in a conversation: an agent asked for a service it cannot
// reach (Switchyard's `request_connector` tool), and the owner signs it in
// right there, on the account the session runs on. Connecting or skipping
// resolves the request, and yardd tells the agent as its next message.
import { useEffect, useState } from "react";
import { accountName, DEFAULT_ACCOUNT } from "./accounts";
import { accountText, connectorOn, entryFor, groupServices, knownFor, pickableConnectors, serviceId, shortError, targetFor, type ConnectorAccount, type Service } from "./connectors";
import { ServiceTile, SignIn } from "./ConnectorsPage";
import { EngineIcon } from "./EngineIcon";
import { ENGINE_NAME } from "./engines";
import type { Engine, Item, SessionMeta } from "./protocol";
import { accountsOn, hostName, hostOf, hostOnline, mcpAuthKey, mcpKey, type Actions, type State } from "./store";
import { Icon } from "./App";

const CHECK = "M5 12.5l4.5 4.5L19 7.5";
/** Engines that sign their own servers in; the others use Switchyard's set. */
const OWN: Engine[] = ["claude", "codex"];

type Card = Extract<Item, { kind: "connector" }>;

export function ConnectCard({ item, meta, state, actions }: { item: Card; meta: SessionMeta; state: State; actions: Actions }) {
  const host = hostOf(meta);
  const online = hostOnline(state, host);
  const own = OWN.includes(meta.engine);
  const account: ConnectorAccount = own
    ? { engine: meta.engine, account: meta.account || DEFAULT_ACCOUNT, name: accountName(accountsOn(state, host), meta.engine, meta.account || DEFAULT_ACCOUNT), engineName: ENGINE_NAME[meta.engine] }
    : { engine: "switchyard", account: DEFAULT_ACCOUNT, name: ENGINE_NAME[meta.engine], engineName: ENGINE_NAME[meta.engine] };
  const status = state.mcp[mcpKey(host, account.engine, account.account)];
  const open = !item.outcome;
  useEffect(() => {
    if (open && online && !status) actions.mcpStatus(host, account.engine, account.account);
  }, [open, online, host, account.engine, account.account, !!status]);

  // The service the agent named, as this account lists it, or as the page knows it.
  const known = knownFor({ name: item.service, source: "user" });
  const id = known?.id ?? serviceId(item.service);
  const service: Service = groupServices([{ account, servers: status?.servers ?? [] }]).find((s) => s.id === id) ?? { id, name: known?.name ?? item.service, about: known?.about ?? "", brand: known?.brand, known, entries: [] };
  const entry = entryFor(service, account);
  const target = status?.servers ? targetFor(service, account) : null;

  // An agent's or automation's own list may leave the service out: only the owner adds it.
  const agent = meta.agent ? state.agents.find((a) => a.id === meta.agent) : undefined;
  const names = service.entries.map((e) => e.server.name);
  const offForAgent = !!agent && names.length > 0 && !names.some((n) => connectorOn(agent.mcp ?? null, n));
  const [error, setError] = useState<string | null>(null);
  const allowAgent = async (also: string[] = names) => {
    if (!agent) return true;
    const picked = agent.mcp ?? null;
    if (also.every((n) => connectorOn(picked, n))) return true;
    const all = pickableConnectors(status?.servers, picked);
    const now = picked ?? all.filter((n) => connectorOn(null, n));
    const r = await actions.saveAgent({ id: agent.id, mcp: [...now, ...also.filter((n) => !now.includes(n))] });
    if (r.error) setError(r.error);
    return !r.error;
  };

  // The sign-in, walked like the Connectors page walks one: add it where it is missing, then sign in.
  const [step, setStep] = useState<null | "add" | "auth">(null);
  const key = target && (target.kind === "auth" || target.kind === "add") ? mcpAuthKey(host, account.engine, account.account, target.server) : "";
  const change = key ? state.mcpChange[key] : undefined;
  const auth = key ? state.mcpAuth[key] : undefined;
  const resolve = (outcome: "connected" | "skipped") => actions.connectorResolve(meta.id, item.requestId, service.name, outcome);
  const finish = async (server: string) => {
    if (await allowAgent([server])) resolve("connected");
  };
  useEffect(() => {
    if (!open || !step || !target || (target.kind !== "auth" && target.kind !== "add")) return;
    if (step === "add" && change?.state === "done") {
      actions.mcpChangeClear(host, account.engine, account.account, target.server);
      actions.mcpAuth(host, account.engine, account.account, target.server);
      setStep("auth");
    } else if (step === "auth" && auth?.state === "done") {
      actions.mcpAuthClear(host, account.engine, account.account, target.server);
      setStep(null);
      void finish(target.server);
    }
  }, [open, step, change?.state, auth?.state]);
  // Signed in elsewhere while the card waited (the Connectors page, another window).
  const connectedNow = entry?.server.status === "connected";

  const start = () => {
    if (!target) return;
    setError(null);
    if (target.kind === "add") {
      actions.mcpAdd(host, account.engine, account.account, target.server, target.url);
      setStep("add");
    } else if (target.kind === "auth") {
      actions.mcpAuth(host, account.engine, account.account, target.server);
      setStep("auth");
    }
  };
  const cancel = () => {
    if (target && (target.kind === "auth" || target.kind === "add")) {
      actions.mcpAuthClear(host, account.engine, account.account, target.server);
      actions.mcpChangeClear(host, account.engine, account.account, target.server);
    }
    setStep(null);
  };

  if (item.outcome === "connected") {
    const tools = entry?.server.tools ?? 0;
    return (
      <div className="connect-line" role="status">
        <ServiceTile service={service} size="sm" />
        <span><strong className="connect-ok"><Icon d={CHECK} size={12} /> {service.name} connected</strong>{tools > 0 ? ` · ${tools} ${tools === 1 ? "tool" : "tools"}` : ""}</span>
      </div>
    );
  }
  if (item.outcome === "skipped") {
    return (
      <div className="connect-line muted" role="status">
        <ServiceTile service={service} size="sm" />
        <span>{service.name} not connected. The agent was told.</span>
      </div>
    );
  }

  const where = `${accountText(account)} on ${hostName(state, host)}`;
  const failed = change?.state === "failed" ? change.error : auth?.state === "failed" ? (auth.error ?? "The sign-in failed.") : error;
  const title = offForAgent && connectedNow ? `Let ${agent!.name} use ${service.name}?` : `Connect ${service.name}`;
  let body;
  if (!online) {
    body = <p className="muted small">Connect to {hostName(state, host)} to sign {service.name} in.</p>;
  } else if (failed) {
    body = (
      <div className="cx-flow" role="alert">
        <span className="bad">{shortError(failed)}</span>
        <div className="cx-flow-actions">
          <button className="btn btn-small" onClick={() => { cancel(); start(); }}>Try again</button>
          <button className="btn-text btn-small" onClick={() => { cancel(); resolve("skipped"); }}>Not now</button>
        </div>
      </div>
    );
  } else if (step && (!auth || auth.state === "starting" || step === "add" || auth.state === "sending")) {
    body = <div className="cx-flow muted" role="status"><span className="cx-spin-line"><span className="cx-spin" aria-hidden="true" />{step === "add" ? `Adding ${service.name}…` : auth?.state === "sending" ? "Finishing the sign-in…" : "Getting the sign-in page…"}</span></div>;
  } else if (step && auth) {
    body = (
      <div className="cx-flow">
        <SignIn auth={auth} name={service.name} host={host} hostLabel={hostName(state, host)} onPaste={(url) => target && actions.mcpCallback(host, account.engine, account.account, target.server, url)} onClose={cancel} />
      </div>
    );
  } else if (!status?.servers && !status?.error) {
    body = <p className="muted small" role="status">Checking {accountText(account)}’s connectors…</p>;
  } else if (connectedNow) {
    body = (
      <div className="connect-actions">
        <button className="btn-primary" onClick={() => void finish(entry!.server.name)}>{offForAgent ? "Allow" : "Continue"}</button>
        <button className="btn-text" onClick={() => resolve("skipped")}>{offForAgent ? "Deny" : "Not now"}</button>
        <span className="muted small connect-hint">{offForAgent ? `Connected on ${accountText(account)}, off for this agent.` : `${service.name} is already connected.`}</span>
      </div>
    );
  } else if (target?.kind === "auth" || target?.kind === "add") {
    body = (
      <div className="connect-actions">
        <button className="btn-primary" onClick={start}>Connect {service.name}</button>
        <button className="btn-text" onClick={() => resolve("skipped")}>Not now</button>
      </div>
    );
  } else if (target?.kind === "open") {
    body = (
      <div className="connect-actions">
        <a className="btn-primary" href={target.url} target="_blank" rel="noreferrer">Connect on ChatGPT</a>
        <button className="btn" onClick={() => actions.mcpStatus(host, account.engine, account.account)}>Check again</button>
        <button className="btn-text" onClick={() => resolve("skipped")}>Not now</button>
      </div>
    );
  } else {
    body = (
      <>
        <p className="muted small">{status?.error ? shortError(status.error) : `${accountText(account)} has no connector called ${service.name}, and Switchyard does not know its address.`}</p>
        <div className="connect-actions">
          <button className="btn" onClick={() => actions.go({ page: "connectors" })}>Open Connectors</button>
          {target?.kind === "retry" && <button className="btn" onClick={() => actions.mcpReconnect(host, account.engine, account.account, target.server)}>Retry</button>}
          <button className="btn-text" onClick={() => resolve("skipped")}>Not now</button>
        </div>
      </>
    );
  }
  return (
    <section className="connect" aria-label={title}>
      <div className="connect-head">
        <ServiceTile service={service} />
        <div className="connect-title">
          <strong>{title}</strong>
          <span className="muted small connect-where">
            {own ? <EngineIcon engine={meta.engine} size={11} /> : null}
            {own ? where : `Held by Switchyard for ${ENGINE_NAME[meta.engine]} on ${hostName(state, host)}`}
          </span>
        </div>
      </div>
      {item.reason && (
        <p className="connect-why">
          <span className="muted">Why the agent wants it</span>
          {item.reason}
        </p>
      )}
      {body}
    </section>
  );
}
