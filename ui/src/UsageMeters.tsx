import { EngineIcon } from "./EngineIcon";
import { useEffect, useRef, useState } from "react";
import { AnchoredPopover } from "./AnchoredPopover";
import type { Engine, EngineStates, Usage } from "./protocol";
import { formatReset, fullResets, fullResetsText, metersFor, noLimitsNote, percentText, reportsLimits, resetText, stripGroups, type Meter, type UsageSource } from "./usage";
import { Icon } from "./App";
import { DEFAULT_ACCOUNT } from "./accounts";
import { ENGINES, ENGINE_LABEL, ENGINE_NAME } from "./engines";

const REFRESH = "M19.8 12a7.8 7.8 0 1 1-2.4-5.7M19.8 3.9v4.8H15";
const CLOSE = "m6 6 12 12M18 6 6 18";
const DOWN = "m6 9 6 6 6-6";
const RIGHT = "m9 6 6 6-6 6";
const CLOCK = "M12 8v4l2 2m6-2a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z";
const ALERT = "M12 7v6m0 3v.1m9-4.1a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z";
const LIMIT_ENGINES = ENGINES.filter(reportsLimits);
const timeOfDay = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const day = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });

function usageWindow(m: Meter) {
  if (m.title) return m.title;
  if (m.label === "Week") return "Weekly · all models";
  if (m.label.startsWith("Week ·")) return `Weekly${m.label.slice(4)}`;
  return `${m.label} window`;
}

/** "5-hour resets in 1 h 35 min": the sidebar row's second line. */
const shortReset = (m: Meter, now: number) => (m.resetsAt == null ? "" : `${m.label} ${formatReset(m.resetsAt, now)}`);

/** Every account's windows and resets, one line each, for the row's tooltip. */
function resetSummary(rows: ReturnType<typeof subscriptionRows>, now: number) {
  return rows
    .filter((r) => r.loggedIn !== false && r.meters.length)
    .flatMap((r) => [r.name, ...r.meters.map((m) => `  ${m.label}: ${percentText(m.percent)} · ${resetText(m.resetsAt, now)}`)])
    .join("\n");
}

/** "2 full resets available · first expires Dec 5 (in 9 d)", for Codex. */
function FullResets({ engine, usage, now }: { engine: Engine; usage: Usage; now: number }) {
  const r = fullResets(engine, usage, now);
  if (!r) return null;
  const expiry = r.expiresAt == null ? "" : `${r.count === 1 ? "Expires" : "First expires"} ${day.format(r.expiresAt)} · ${formatReset(r.expiresAt, now).replace(/^resets /, "")}`;
  return <div className="usage-full-resets"><span>{fullResetsText(r)}</span>{expiry && <span className="muted">{expiry}</span>}</div>;
}

function UsageAvatar({ engine, small }: { engine: Engine; small?: boolean }) {
  return <span className={`usage-avatar usage-avatar-${engine}${small ? " usage-avatar-small" : ""}`} aria-hidden="true">
    <EngineIcon engine={engine} size={small ? 11 : 16} />
  </span>;
}

function useNow() {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(t);
  }, []);
  return now;
}
export const Bar = ({ meter }: { meter: Meter }) => (
  <div
    className="bar-track"
    role="progressbar"
    aria-label={`${meter.label} used`}
    aria-valuenow={Math.round(meter.percent)}
    aria-valuemin={0}
    aria-valuemax={100}
  >
    <div className={`bar-fill level-${meter.level}`} style={{ width: `${meter.percent}%` }} />
  </div>
);

export type SubscriptionSource = UsageSource & { name?: string; engines?: EngineStates; online?: boolean };
export type UsageAccount = { host: string; engine: Engine; account: string };

/** Keep the existing cross-host deduplication and freshness rules. Identity
 * and host badges explain which login each report belongs to. */
export function subscriptionRows(sources: SubscriptionSource[], now: number) {
  const groups = stripGroups(sources, [...LIMIT_ENGINES]);
  for (const source of sources)
    for (const engine of LIMIT_ENGINES) {
      if (source.engines?.[engine]?.installed && !groups.some((g) => g.engine === engine && g.account === null))
        groups.push({ engine, account: null, host: source.host, usage: {}, reported: false });
    }
  return groups.map((g) => {
    const available = sources.filter((s) =>
      g.account === null
        ? !!s.engines?.[g.engine]?.installed || !!(s.usage as Record<string, unknown>)[g.engine]
        : s.accounts.some((a) => a.engine === g.engine && a.name.toLowerCase() === g.account!.toLowerCase()),
    );
    const reportSource = sources.find((s) => s.host === g.host);
    const account =
      g.account === null ? undefined : reportSource?.accounts.find((a) => a.engine === g.engine && a.name.toLowerCase() === g.account!.toLowerCase());
    const login = account ? reportSource?.engines?.accounts?.[account.id] : reportSource?.engines?.[g.engine];
    return {
      ...g,
      key: `${g.engine}:${g.account ?? "default"}`,
      name: `${ENGINE_NAME[g.engine]} · ${g.account ?? "Default"}`,
      loggedIn: login?.loggedIn,
      identity: login?.account || (login?.loggedIn === false ? "Needs sign-in" : ""),
      hosts: available,
      meters: metersFor(g.engine, g.usage, now),
    };
  });
}

/** The meter the closed trigger shows: the fullest one across accounts. An
 *  account that needs sign-in keeps its last report, but the popover hides it
 *  behind "Sign in to see usage", so the trigger leaves it out too. */
export function triggerMeter(rows: ReturnType<typeof subscriptionRows>): Meter | undefined {
  return fullest(rows)?.meter;
}

function fullest(rows: ReturnType<typeof subscriptionRows>) {
  return rows
    .filter((r) => r.loggedIn !== false)
    .flatMap((row) => row.meters.map((meter) => ({ row, meter })))
    .sort((a, b) => b.meter.percent - a.meter.percent)[0];
}

/** The account the popover opens on when no session picks one: the one whose
 *  meter the trigger shows, so the number you clicked is on screen. Default
 *  sorts first even with nothing reported. */
export function leadRow(rows: ReturnType<typeof subscriptionRows>) {
  return fullest(rows)?.row ?? rows[0];
}

/** An account under Other accounts: every window it reports, each with its
 *  reset, so the folded accounts are as readable as the one shown first. */
export function OtherAccountRow({ row, now, onSignIn }: { row: ReturnType<typeof subscriptionRows>[number]; now: number; onSignIn(): void }) {
  return (
    <section className="usage-other-account" aria-label={row.name}>
      <div className="usage-other-name">
        <UsageAvatar engine={row.engine} small />
        <strong>{row.name}</strong>
        {row.loggedIn === false
          ? <button className="btn-text usage-link" aria-label={`Sign in — ${row.name}`} onClick={onSignIn}>Sign in</button>
          : !row.meters.length && <span className="muted">Not reported</span>}
      </div>
      {row.loggedIn !== false && row.meters.map((m) => (
        <div key={m.key} className={`usage-other-meter level-${m.level}`}>
          <span>{percentText(m.percent)} of {m.label}</span>
          <span className="muted">{resetText(m.resetsAt, now)}</span>
        </div>
      ))}
      {row.loggedIn !== false && <FullResets engine={row.engine} usage={row.usage} now={now} />}
    </section>
  );
}

export function UsageStrip({ sources, selected, onManageAccounts, onManage, onRefresh, compact = false }: {
  sources: SubscriptionSource[];
  selected?: UsageAccount;
  compact?: boolean;
  onManageAccounts(): void;
  onManage(host: string): void;
  onRefresh(host: string, engine: Engine, account: string): Promise<{ error?: string }>;
}) {
  const now = useNow();
  const rows = subscriptionRows(sources, now);
  const selectedHost = sources.find((source) => source.host === selected?.host);
  const selectedEntry = selectedHost?.accounts.find((account) => account.id === selected?.account);
  const selectedName = selectedEntry?.name ?? "Default";
  const chosen = selectedHost && selected
    ? rows.find((row) => row.engine === selected.engine && (row.account ?? "Default").toLowerCase() === selectedName.toLowerCase())
    : undefined;
  const active = chosen ?? (selectedHost ? undefined : leadRow(rows));
  const others = rows.filter((row) => row !== active);
  const activeLabel = selectedHost && selected ? `${ENGINE_NAME[selected.engine]} · ${selectedName}` : active?.name;
  const activeName = active?.account ?? (selectedHost ? selectedName : "Default");
  const accountKind = active?.account != null || selectedEntry ? "Named account" : "Default account";
  const activeHost = active ? sources.find((source) => source.host === active.host && source.online !== false) ?? active.hosts.find((source) => source.online !== false) : undefined;
  const activeEngine = active?.engine ?? selected?.engine;
  const accountId = active?.account == null ? DEFAULT_ACCOUNT : activeHost?.accounts.find((account) => account.engine === active.engine && account.name.toLowerCase() === active.account!.toLowerCase())?.id;
  const canRefresh = !!(activeHost && activeEngine && accountId && reportsLimits(activeEngine) && active?.loggedIn !== false);
  const [open, setOpen] = useState(false);
  const [showOthers, setShowOthers] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [failure, setFailure] = useState("");
  const [lastRefresh, setLastRefresh] = useState<{ host: string; engine: Engine; account: string; at: number }>();
  const anchor = useRef<HTMLButtonElement>(null);
  const highest = triggerMeter(rows);
  const silent = silentEngines(sources);
  const close = () => { setOpen(false); setShowOthers(false); setFailure(""); };
  const refresh = async () => {
    if (!canRefresh || refreshing || !activeHost || !activeEngine || !accountId) return;
    setRefreshing(true);
    setFailure("");
    const result = await onRefresh(activeHost.host, activeEngine, accountId);
    setFailure(result.error ?? "");
    if (!result.error) setLastRefresh({ host: activeHost.host, engine: activeEngine, account: accountId, at: Date.now() });
    setRefreshing(false);
  };
  const manage = (host: string) => { close(); onManage(host); };
  const signIns = others.filter((row) => row.loggedIn === false).length;
  const refreshTime = lastRefresh && lastRefresh.host === activeHost?.host && lastRefresh.engine === activeEngine && lastRefresh.account === accountId
    ? lastRefresh.at : undefined;
  return (
    <div className={compact ? "mobile-usage" : "usage-dock"}>
      <button
        ref={anchor}
        className={`btn-quiet ${compact ? "mobile-usage-trigger" : "usage-trigger"}`}
        aria-label="Subscription usage"
        title={resetSummary(rows, now) || undefined}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => open ? close() : setOpen(true)}
      >
        {compact && highest ? (
          <><span className="usage-chip-track" aria-hidden="true"><span style={{ width: `${highest.percent}%` }} /></span><span>{percentText(highest.percent)}</span></>
        ) : (
          <>
            <Icon d={CLOCK} size={14} />
            {compact ? <span>Usage</span> : (
              <span className="usage-trigger-text">
                <span className="usage-trigger-top">
                  <span>Usage</span><span className="spacer" />
                  <span className={highest?.level === "high" ? "bad" : highest?.level === "warn" ? "warn" : "muted"}>{highest ? `${percentText(highest.percent)} used` : "View limits"}</span>
                </span>
                {highest && shortReset(highest, now) && <span className="muted usage-trigger-reset">{shortReset(highest, now)}</span>}
              </span>
            )}
            {!compact && <Icon d={RIGHT} size={12} />}
          </>
        )}
      </button>
      {open && (
        <AnchoredPopover anchor={anchor} label="Subscription usage" variant="usage-popover" onClose={close}>
          <div className="popover-heading">
            <span className="usage-sheet-handle" aria-hidden="true" />
            <strong>Usage</strong>
            <span className="spacer" />
            <button className="btn-icon" aria-label="Refresh usage" title="Refresh usage" disabled={!canRefresh || refreshing} onClick={refresh}>
              <span className={refreshing ? "usage-refreshing" : undefined}><Icon d={REFRESH} size={15} width={2.4} /></span>
            </button>
            <button className="btn-icon usage-close" aria-label="Close usage popover" onClick={close}><Icon d={CLOSE} size={16} width={2.4} /></button>
          </div>
          <div className="usage-popover-body">
            {failure && (
              <div className="usage-error" role="alert">
                <Icon d={ALERT} size={16} />
                <span><strong>Couldn’t refresh usage.</strong> {failure}</span>
                <button className="btn-text usage-link" onClick={refresh}>Try again</button>
              </div>
            )}
            {activeLabel && activeEngine ? (
              <section className="compact-subscription usage-active" aria-label={activeLabel}>
                <div className="usage-account-line">
                  <UsageAvatar engine={activeEngine} />
                  <div className="usage-account-title">
                    <strong>{activeName}</strong>
                    <span className="muted">{ENGINE_LABEL[activeEngine]} · {accountKind}</span>
                  </div>
                  {active?.loggedIn === false && <span className="usage-near-limit">Needs sign-in</span>}
                </div>
                {selectedHost?.online === false && <div className="usage-explanation">The selected host is offline.</div>}
                {active?.loggedIn === false ? (
                  <div className="usage-explanation">
                    <strong>Sign in to see usage</strong>
                    <p>Switchyard needs a working sign-in to read this account’s limits.</p>
                    <button className="btn-primary" onClick={() => manage(active.host)}>Sign in</button>
                  </div>
                ) : active?.meters.length ? (
                  <div className="usage-limits" aria-busy={refreshing}>
                    {active.meters.map((m) => {
                      const label = usageWindow(m);
                      return <div key={m.key} className={`compact-meter${m.percent >= 80 ? " usage-meter-near" : ""}`}>
                        <span className="usage-window">{label}</span><strong className="usage-percent">{percentText(m.percent)} used</strong>
                        <Bar meter={{ ...m, label }} />
                        <div className="usage-meter-meta">
                          <span className="muted usage-reset">{resetText(m.resetsAt, now)}</span>
                          {m.percent >= 80 && <span className="usage-near-limit">Near limit</span>}
                        </div>
                      </div>;
                    })}
                    <FullResets engine={active.engine} usage={active.usage} now={now} />
                  </div>
                ) : (
                  <div className="usage-explanation">
                    <strong>No limits reported yet</strong>
                    <p>{reportsLimits(activeEngine) ? "Usage appears when the provider reports this account’s limits." : "This engine does not report subscription limits."}</p>
                  </div>
                )}
              </section>
            ) : <p className="usage-explanation">No subscription limits reported yet.</p>}
            {others.length > 0 && (
              <div className="usage-others">
                <button className="btn-quiet usage-other-toggle" aria-expanded={showOthers} onClick={() => setShowOthers(!showOthers)}>
                  <span>Other accounts</span><span className="spacer" />
                  <span className="muted">{signIns ? `${signIns} need sign-in` : others.length}</span>
                  <Icon d={DOWN} size={14} />
                </button>
                {showOthers && others.map((row) => <OtherAccountRow key={row.key} row={row} now={now} onSignIn={() => manage(row.host)} />)}
              </div>
            )}
            {silent.length > 0 && <p className="muted small usage-silent">{noLimitsNote(silent).replace(" subscription limits", " limits")}</p>}
          </div>
          <div className="popover-footer">
            <span role="status">{refreshing ? "Refreshing…" : refreshTime ? `Updated at ${timeOfDay.format(refreshTime)}` : "Shared across hosts"}</span>
            <button className="btn-text usage-link" onClick={() => { close(); onManageAccounts(); }}>Manage accounts</button>
          </div>
        </AnchoredPopover>
      )}
    </div>
  );
}

/** Engines installed on a shown host that report no limits at all. */
function silentEngines(sources: SubscriptionSource[]): Engine[] {
  return ENGINES.filter((e) => !reportsLimits(e) && sources.some((s) => s.engines?.[e]?.installed));
}

export function Subscriptions({ sources, onManage }: { sources: SubscriptionSource[]; onManage(host?: string): void }) {
  const now = useNow();
  const rows = subscriptionRows(sources, now);
  return (
    <div className="subscriptions">
      <div className="subscription-columns muted small" aria-hidden="true">
        <span>Account</span>
        <span>Reported limits</span>
      </div>
      {rows.map((r) => (
        <section className="subscription-row" key={r.key} aria-label={r.name}>
          <div className="subscription-identity">
            <strong>{r.name}</strong>
            {r.identity && <p className="muted small">{r.identity}</p>}
            <div className="subscription-hosts">
              {r.hosts.map((s) => (
                <span key={s.host} className="account-host-badge">
                  {s.name ?? s.host}
                </span>
              ))}
            </div>
          </div>
          {/* The last report outlives a lost sign-in; the popover hides it too. */}
          {r.loggedIn === false ? (
            <p className="muted small">
              Sign in to see usage.{" "}
              {/* The sign-in state comes from r.host, so that host's logins hold the account. */}
              <button className="btn-text usage-link" aria-label={`Sign in — ${r.name}`} onClick={() => onManage(r.host)}>Sign in</button>
            </p>
          ) : r.meters.length ? (
            <div className="subscription-meters">
              {r.meters.map((m) => (
                <div key={m.key} className={`subscription-meter level-${m.level}`}>
                  <span className="muted small">{m.label}</span>
                  <strong className="usage-percent">{percentText(m.percent)} used</strong>
                  <Bar meter={m} />
                  <span className="muted small usage-reset">{resetText(m.resetsAt, now)}</span>
                </div>
              ))}
              <FullResets engine={r.engine} usage={r.usage} now={now} />
            </div>
          ) : (
            <p className="muted small">No limits reported yet</p>
          )}
        </section>
      ))}
      {!rows.length && (
        <div className="focused-empty">
          <strong>No subscription limits reported yet</strong>
          <p className="muted">Usage appears here when Claude, Codex or Grok reports it.</p>
        </div>
      )}
      <p className="muted small subscription-note">One limit per account, shared across its hosts. {noLimitsNote()}</p>
      <button className="btn-text" onClick={() => onManage()}>
        Manage engine logins →
      </button>
    </div>
  );
}
