// Putting an agent in Slack as an app of its own, so people @mention it by
// name, step by step as Orca's bots do: a configuration token once per
// workspace, the agent's app made from it, the install approved in Slack,
// then (Switchyard has no cloud for Slack to reach) one app-level token for
// its Socket Mode connection. yardd's slack.rs does the work; this dialog
// only shows the step it reports. The shared Switchyard bot on the
// Connectors page is separate.
import { useState } from "react";
import type { Agent, SlackAgentApp } from "./protocol";
import type { Actions, SlackAppOp, State } from "./store";
import { Icon } from "./App";

/** Slack, drawn as the hash of its logo. */
export const SLACK_MARK = "M9.5 4.5v15M14.5 4.5v15M4.5 9.5h15M4.5 14.5h15";
const CLOSE = "M6 6l12 12M18 6L6 18";
const APPS = "https://api.slack.com/apps";

/** Where the agent's app stands, in words. */
export function slackConnection(s: SlackAgentApp | undefined): string | null {
  if (!s || s.step !== "ready") return null;
  if (s.state === "connected") return "Connected";
  if (s.state === "error") return `Not connected: ${s.error ?? "Slack refused the connection"}`;
  return "Connecting…";
}

function TokenField({ label, placeholder, busy, button, onSubmit }: { label: string; placeholder: string; busy: boolean; button: string; onSubmit(token: string): void }) {
  const [token, setToken] = useState("");
  const id = `slack-${label.replace(/\W+/g, "-").toLowerCase()}`;
  return (
    <form
      className="slack-token"
      onSubmit={(e) => {
        e.preventDefault();
        if (token.trim()) onSubmit(token.trim());
      }}
    >
      <label htmlFor={id}>{label}</label>
      <div className="login-code">
        <input id={id} type="password" autoComplete="off" spellCheck={false} value={token} placeholder={placeholder} onChange={(e) => setToken(e.target.value)} />
        <button className="btn-primary" type="submit" disabled={busy || !token.trim()}>
          {busy ? "Working…" : button}
        </button>
      </div>
    </form>
  );
}

export function SlackSetup({ state, actions, agent, onClose }: { state: State; actions: Actions; agent: Agent; onClose(): void }) {
  const s = state.slack?.agentApps?.[agent.id];
  const workspaces = state.slack?.workspaces ?? [];
  const [addWorkspace, setAddWorkspace] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [paste, setPaste] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const run = async (op: SlackAppOp, fields: Record<string, unknown>) => {
    setBusy(true);
    setError(null);
    const r = await actions.slackApp(op, { agentId: agent.id, ...fields });
    setBusy(false);
    if (r.error) setError(r.error);
    return r;
  };
  // The install comes back to this page's address, when Slack can be sent there.
  const origin = window.location.origin;
  const create = (teamId: string) => run("slack.app.create", { teamId, origin });
  // A new workspace's token, and the agent's app there, in one step.
  const grant = (token: string) => run("slack.grant", { token, origin });
  const handle = s?.handle || agent.name.toLowerCase();

  let title = `Add ${agent.name} to Slack`;
  let body: React.ReactNode;
  if (!s && (addWorkspace || workspaces.length === 0)) {
    body = (
      <>
        <p>
          {agent.name} gets its own Slack app, named after it, so people can @mention it in a channel or message it directly. Slack makes the app from a configuration token:
        </p>
        <a className="btn login-open" href={APPS} target="_blank" rel="noopener noreferrer">
          Get config token
        </a>
        <ol className="login-steps">
          <li>In Slack app settings, find “Your App Configuration Tokens” and choose Generate Token.</li>
          <li>Select the workspace for your agent.</li>
          <li>Copy the Refresh Token (it starts with xoxe-) and paste it below.</li>
        </ol>
        <TokenField label="Configuration refresh token" placeholder="xoxe-…" busy={busy} button="Create Slack app" onSubmit={(t) => void grant(t)} />
        <p className="muted small">Switchyard keeps this token on this machine and renews it, so your next agents in this workspace need one click.</p>
      </>
    );
  } else if (!s) {
    body = (
      <>
        <p>Choose a Slack workspace. {agent.name} gets its own app there, named after it.</p>
        <div className="slack-workspaces" role="group" aria-label="Slack workspaces">
          {workspaces.map((w) => (
            <button key={w.teamId} className="picker-item menu-item" disabled={busy} onClick={() => void create(w.teamId)}>
              <Icon d={SLACK_MARK} size={14} />
              <span>{w.teamName}</span>
            </button>
          ))}
        </div>
        <button className="btn-text" onClick={() => setAddWorkspace(true)}>
          Add another workspace
        </button>
      </>
    );
  } else if (s.step === "install") {
    const installPage = `${APPS}/${s.appId}/install-on-team`;
    const pasted = (
      <>
        <ol className="login-steps">
          <li>
            Open its{" "}
            <a href={installPage} target="_blank" rel="noopener noreferrer">
              Install App page
            </a>{" "}
            and choose Install to {s.teamName || "your workspace"}, then Allow.
          </li>
          <li>Copy the Bot User OAuth Token (it starts with xoxb-) and paste it below.</li>
        </ol>
        <TokenField label="Bot token" placeholder="xoxb-…" busy={busy} button="Next" onSubmit={(t) => void run("slack.app.botToken", { token: t })} />
      </>
    );
    body =
      s.authorizeUrl && !paste ? (
        <>
          <p>Your Slack app is ready. Approve it in Slack to finish.</p>
          <a className="btn-primary login-open" href={s.authorizeUrl} target="_blank" rel="noopener noreferrer">
            Approve in Slack
          </a>
          <p className="muted small">The link works once, for 30 minutes. Sign in to Slack as yourself there. If your workspace needs an admin to approve new apps, {agent.name} joins once they do.</p>
          <div className="row slack-alt">
            <button className="btn-text" disabled={busy} onClick={() => void run("slack.app.authorize", { origin })}>
              New link
            </button>
            <button className="btn-text" onClick={() => setPaste(true)}>
              Paste its bot token instead
            </button>
          </div>
        </>
      ) : (
        <>
          <p>Your Slack app is ready. Install it in your workspace:</p>
          {pasted}
          {!s.canRedirect && <p className="muted small">Slack can send you back here on its own only when Switchyard is open over https.</p>}
        </>
      );
  } else if (s.step === "appToken") {
    body = (
      <>
        <p>
          One more token. Slack cannot reach this machine, so {agent.name} keeps its own connection open to Slack, and that takes an app-level token, which Slack only makes by
          hand:
        </p>
        <ol className="login-steps">
          <li>
            Open{" "}
            <a href={`${APPS}/${s.appId}/general`} target="_blank" rel="noopener noreferrer">
              {agent.name}'s app settings
            </a>
            .
          </li>
          <li>Under App-Level Tokens choose Generate Token and Scopes. Name it switchyard, add the scope connections:write, and Generate.</li>
          <li>Copy the token (it starts with xapp-) and paste it below.</li>
        </ol>
        <TokenField label="App-level token" placeholder="xapp-…" busy={busy} button="Connect" onSubmit={(t) => void run("slack.app.appToken", { token: t })} />
      </>
    );
  } else {
    title = `${agent.name} is in Slack`;
    const connection = slackConnection(s);
    body = (
      <>
        <p>
          Installed by {s.ownerName || "you"}
          {s.teamName ? ` in ${s.teamName}` : ""}. Mention @{handle} in any channel it is in, or message it directly; it answers in the thread. Add it to a channel with /invite @{handle}.
        </p>
        <p className={`slack-connection small ${s.state === "connected" ? "is-live" : ""}`}>
          <span className="slack-dot" aria-hidden="true" />
          {connection}
        </p>
        <p className="muted small">
          Who can ask: the people allowed under{" "}
          <button className="btn-text note-link" onClick={() => actions.go({ page: "connectors" })}>
            Connectors, Slack
          </button>
          . {s.ownerName || "You"} {s.ownerName ? "is" : "are"} on that list. Anyone else who writes to {agent.name} shows there as a request.
        </p>
        <div className="row slack-alt">
          {confirmRemove ? (
            <>
              <span className="muted small">Its Slack app is deleted.</span>
              <button className="btn danger" disabled={busy} onClick={() => void run("slack.app.remove", {})}>
                Remove from Slack
              </button>
              <button className="btn-text" onClick={() => setConfirmRemove(false)}>
                Keep it
              </button>
            </>
          ) : (
            <button className="btn-text danger" onClick={() => setConfirmRemove(true)}>
              Remove from Slack
            </button>
          )}
          <span className="spacer" />
          <button className="btn-primary" onClick={onClose}>
            Done
          </button>
        </div>
      </>
    );
  }
  const steps = ["Token", "Approve", "Connect"];
  const at = !s ? 0 : s.step === "install" ? 1 : s.step === "appToken" ? 2 : 3;
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal slack-modal" role="dialog" aria-modal="true" aria-label={title}>
        <header className="modal-head">
          <Icon d={SLACK_MARK} size={16} />
          <h2>{title}</h2>
          <span className="spacer" />
          <button className="btn-icon" aria-label="Close" onClick={onClose}>
            <Icon d={CLOSE} size={14} />
          </button>
        </header>
        {at < 3 && (
          <ol className="slack-progress" aria-label="Steps">
            {steps.map((label, i) => (
              <li key={label} className={i < at ? "done" : i === at ? "now" : ""} aria-current={i === at ? "step" : undefined}>
                {label}
              </li>
            ))}
          </ol>
        )}
        {body}
        {error && (
          <p className="small automation-reason" role="alert">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}
