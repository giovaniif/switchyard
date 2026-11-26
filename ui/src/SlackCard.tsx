// The Slack connector on the Connectors page: the owner connects his own
// Slack (an app of his with user scopes: a configuration token, Approve in
// Slack, one connection token), sees its connection, and decides who may
// talk to agents through their own apps. yardd keeps the tokens; this page
// only ever sees their kind and last four characters.
import { useState } from "react";
import type { SlackView } from "./protocol";
import type { Actions, SlackAppOp, State } from "./store";
import { Icon } from "./App";
import { SLACK_MARK, TokenField } from "./SlackSetup";
import { allowedSummary, mySteps, slackStatus } from "./slack";

const APPS = "https://api.slack.com/apps";

export function SlackCard({ state, actions }: { state: State; actions: Actions }) {
  const view = state.slack;
  const [confirm, setConfirm] = useState(false);
  if (!view) return null;
  const status = slackStatus(view);
  const me = view.me;
  const attention = me?.state === "error";
  return (
    <section className={`host-card slack-card ${attention ? "connectors-card-attention" : ""}`} aria-label="Slack">
      <header className="connectors-card-head">
        <span className="connector-account-icon connector-account-slack" aria-hidden="true">#</span>
        <div className="connector-account-name">
          <strong>Slack</strong>
          <span className={`small ${status.tone === "bad" ? "bad" : "muted"} slack-status-line`}>{status.line}</span>
        </div>
        <span className={`connector-account-state slack-state-${status.tone}`} role="status">{status.label}</span>
        {attention && (
          <button className="btn btn-small" onClick={() => actions.slack("slack.me.retry")}>
            Retry
          </button>
        )}
      </header>
      {me?.step !== "ready" && <ConnectMe view={view} actions={actions} />}
      <ul className="connector-list">
        {view.requests.map((r) => (
          <li key={r.id} className="connector">
            <div className="connector-row">
              <span className="connector-id">
                <span className="dot connector-dot-needs-auth" aria-hidden="true" />
                <span className="connector-name">{r.name}</span>
                <span className="muted small connector-via">{r.place === "Direct message" ? "wrote to an agent directly" : `wrote to an agent in ${r.place}`} · {r.id}</span>
              </span>
              <span className="connector-side">
                <span className="connector-status connector-status-needs-auth">Not allowed</span>
                <button className="btn btn-small" onClick={() => actions.slack("slack.allow", { user: r.id })} aria-label={`Allow ${r.name}`}>
                  Allow
                </button>
                <button className="btn-text btn-small" onClick={() => actions.slack("slack.dismiss", { user: r.id })} aria-label={`Dismiss ${r.name}`}>
                  Dismiss
                </button>
              </span>
            </div>
          </li>
        ))}
        {view.allowed.map((p) => (
          <AllowedRow key={p.id} person={p} onRemove={() => actions.slack("slack.disallow", { user: p.id })} />
        ))}
        {!view.allowed.length && !view.requests.length && (
          <li className="connector">
            <div className="connector-row">
              <span className="connector-id">
                <span className="dot connector-dot-pending" aria-hidden="true" />
                <span className="connector-name">Who can talk to agents</span>
              </span>
            </div>
            <p className="muted small slack-hint">{allowedSummary(view)}</p>
          </li>
        )}
      </ul>
      {me && (
        <footer className="connector-card-footer">
          <span className="muted small slack-tokens">{[me.userToken, me.appToken].filter(Boolean).join(" · ") || me.teamName}</span>
          <span className="slack-footer-actions">
            <button
              className={`btn-text btn-small ${confirm ? "bad" : ""}`}
              onClick={() => (confirm ? actions.slackApp("slack.me.remove", {}) : setConfirm(true))}
              onBlur={() => setConfirm(false)}
              aria-label={confirm ? "Confirm disconnecting your Slack" : "Disconnect your Slack"}
            >
              {confirm ? "Confirm: forget it and delete its app" : "Disconnect"}
            </button>
          </span>
        </footer>
      )}
    </section>
  );
}

function AllowedRow({ person, onRemove }: { person: { id: string; name: string }; onRemove(): void }) {
  const [confirm, setConfirm] = useState(false);
  return (
    <li className="connector">
      <div className="connector-row">
        <span className="connector-id">
          <span className="dot connector-dot-connected" aria-hidden="true" />
          <span className="connector-name">{person.name}</span>
          <span className="muted small connector-via">{person.id}</span>
        </span>
        <span className="connector-side">
          <span className="connector-status">Can talk to agents</span>
          <button
            className={`btn-text btn-small ${confirm ? "bad" : ""}`}
            onClick={() => (confirm ? onRemove() : setConfirm(true))}
            onBlur={() => setConfirm(false)}
            aria-label={confirm ? `Confirm removing ${person.name}` : `Remove ${person.name}`}
          >
            {confirm ? "Confirm" : "Remove"}
          </button>
        </span>
      </div>
    </li>
  );
}

/** Connecting the owner's own Slack, the step he is at: the workspace's
 *  configuration token (one click when an agent's app has it already), the
 *  install approved in Slack (or his user token pasted, with no https
 *  address), then the app-level token for its connection. */
function ConnectMe({ view, actions }: { view: SlackView; actions: Actions }) {
  const me = view.me;
  const workspaces = view.workspaces ?? [];
  const [other, setOther] = useState(false);
  const [paste, setPaste] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const origin = window.location.origin;
  const run = async (op: SlackAppOp, fields: Record<string, unknown>) => {
    setBusy(true);
    setError(null);
    const r = await actions.slackApp(op, fields);
    setBusy(false);
    if (r.error) setError(r.error);
  };
  const { steps, at } = mySteps(view);
  let body: React.ReactNode;
  if (!me && workspaces.length > 0 && !other) {
    body = (
      <>
        <p>Switchyard reads your direct messages and the channels you choose, as you, through a Slack app of your own. Choose the workspace:</p>
        <div className="slack-workspaces" role="group" aria-label="Slack workspaces">
          {workspaces.map((w) => (
            <button key={w.teamId} className="picker-item menu-item" disabled={busy} onClick={() => void run("slack.me.create", { teamId: w.teamId, origin })}>
              <Icon d={SLACK_MARK} size={14} />
              <span>{w.teamName}</span>
            </button>
          ))}
        </div>
        <div className="row slack-alt">
          <button className="btn-text" onClick={() => setOther(true)}>
            Another workspace
          </button>
        </div>
      </>
    );
  } else if (!me) {
    body = (
      <>
        <p>Switchyard reads your direct messages and the channels you choose, as you, through a Slack app of your own. Slack makes it from a configuration token:</p>
        <a className="btn login-open" href={APPS} target="_blank" rel="noopener noreferrer">
          Get config token
        </a>
        <ol className="login-steps">
          <li>In Slack app settings, find “Your App Configuration Tokens” and choose Generate Token.</li>
          <li>Select your workspace.</li>
          <li>Copy the Refresh Token (it starts with xoxe-) and paste it below.</li>
        </ol>
        <TokenField label="Configuration refresh token" placeholder="xoxe-…" busy={busy} button="Create my Slack app" onSubmit={(t) => void run("slack.grant", { token: t, me: true, origin })} />
      </>
    );
  } else if (me.step === "install" && me.authorizeUrl && !paste) {
    body = (
      <>
        <p>Your Slack app is made. Approve it in Slack, signed in as yourself. Then one token is left, here.</p>
        <a className="btn-primary login-open" href={me.authorizeUrl} target="_blank" rel="noopener noreferrer">
          Approve in Slack
        </a>
        <p className="muted small">The link works once, for 30 minutes. If your workspace needs an admin to approve new apps, Slack sends them your request; once they approve it, choose New link and approve again.</p>
        <div className="row slack-alt">
          <button className="btn-text" disabled={busy} onClick={() => void run("slack.me.authorize", { origin })}>
            New link
          </button>
          <button className="btn-text" onClick={() => setPaste(true)}>
            Paste your user token instead
          </button>
        </div>
      </>
    );
  } else if (me.step === "install") {
    body = (
      <>
        <p>Your Slack app is made. Install it in your workspace:</p>
        <ol className="login-steps">
          <li>
            Open its{" "}
            <a href={`${APPS}/${me.appId}/install-on-team`} target="_blank" rel="noopener noreferrer">
              Install App page
            </a>{" "}
            and choose Install to {me.teamName || "your workspace"}, then Allow.
          </li>
          <li>Copy the User OAuth Token (it starts with xoxp-) and paste it below.</li>
        </ol>
        <TokenField label="User token" placeholder="xoxp-…" busy={busy} button="Next" onSubmit={(t) => void run("slack.me.userToken", { token: t })} />
        {!me.canRedirect && <p className="muted small">Slack sends you back to Switchyard on its own once Switchyard has an https address.</p>}
      </>
    );
  } else {
    body = (
      <>
        <p>Approved. Last, Switchyard keeps a connection open to Slack, which takes an app-level token. Slack only makes those by hand, on the app's Basic Information page:</p>
        <a className="btn login-open" href={`${APPS}/${me.appId}/general`} target="_blank" rel="noopener noreferrer">
          Open Basic Information
        </a>
        <ol className="login-steps">
          <li>Under App-Level Tokens, choose Generate Token and Scopes.</li>
          <li>Name it switchyard, Add Scope connections:write, then Generate.</li>
          <li>Copy the token (it starts with xapp-) and paste it below.</li>
        </ol>
        <TokenField label="App-level token" placeholder="xapp-…" busy={busy} button="Connect" onSubmit={(t) => void run("slack.me.appToken", { token: t })} />
      </>
    );
  }
  return (
    <div className="slack-setup">
      <ol className="slack-progress" aria-label="Steps">
        {steps.map((label, i) => (
          <li key={label} className={i < at ? "done" : i === at ? "now" : ""} aria-current={i === at ? "step" : undefined}>
            {label}
          </li>
        ))}
      </ol>
      {body}
      {error && (
        <p className="small automation-reason" role="alert">
          {error}
        </p>
      )}
      <p className="muted small">Tokens stay on this host, readable by you alone.</p>
    </div>
  );
}
