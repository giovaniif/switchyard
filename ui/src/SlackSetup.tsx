// Putting an agent in Slack as an app of its own, so people @mention it by
// name, step by step as Orca's bots do: a configuration token once per
// workspace, the agent's app made from it, the install approved in Slack
// (which comes back to Switchyard on its own when it has an https address),
// then one app-level token for its Socket Mode connection. A public address
// for Slack's events (YARD_SLACK_PUBLIC_URL, opt-in, not offered here) drops
// that last step. yardd's slack.rs does the work; this dialog only shows the
// steps it still needs. The owner's own Slack, on the Connectors page, goes
// the same way with an app of his (SlackCard.tsx).
import { useState } from "react";
import type { Agent, SlackAgentApp } from "./protocol";
import type { Actions, SlackAppOp, State } from "./store";
import { Icon } from "./App";
import { setupSteps, tokensNeeded } from "./slack";

/** Slack, drawn as the hash of its logo. */
export const SLACK_MARK = "M9.5 4.5v15M14.5 4.5v15M4.5 9.5h15M4.5 14.5h15";
const CLOSE = "M6 6l12 12M18 6L6 18";
const APPS = "https://api.slack.com/apps";

/** Where the agent's app stands, in words. */
export function slackConnection(s: SlackAgentApp | undefined): string | null {
  if (!s || s.step !== "ready") return null;
  if (s.state === "connected") return "Connected";
  if (s.state === "error") return `Not connected: ${s.error ?? "Slack refused the connection"}`;
  if (s.state === "events") return `Slack sends its messages to ${s.eventsUrl}`;
  return "Connecting…";
}

export function TokenField({ label, placeholder, busy, button, onSubmit }: { label: string; placeholder: string; busy: boolean; button: string; onSubmit(token: string): void }) {
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
  const view = state.slack;
  const s = view?.agentApps?.[agent.id];
  const workspaces = view?.workspaces ?? [];
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
  // The install comes back to this page's address when Slack may be sent
  // there, else to another https address Switchyard has.
  const origin = window.location.origin;
  const create = (teamId: string) => run("slack.app.create", { teamId, origin });
  // A new workspace's token, and the agent's app there, in one step.
  const grant = (token: string) => run("slack.grant", { token, origin });
  const handle = s?.handle || agent.name.toLowerCase();
  const http = s ? !!s.eventsUrl : !!view?.publicUrl;
  const onlyOne = tokensNeeded(view) === 1;
  const settings = s ? `${APPS}/${s.appId}/general` : APPS;

  let title = `Add ${agent.name} to Slack`;
  let body: React.ReactNode;
  if (!s && (addWorkspace || workspaces.length === 0)) {
    body = (
      <>
        <p>
          {agent.name} gets its own Slack app, named after it, so people can @mention it in a channel or message it directly. Slack makes the app from a configuration token
          {onlyOne ? ", the only token it takes:" : ":"}
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
          <p>Your Slack app is ready. Approve it in Slack{http ? `: then ${agent.name} is in Slack.` : ". Then one token is left, here."}</p>
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
          {!s.canRedirect && <p className="muted small">Slack sends you back to Switchyard on its own once Switchyard has an https address.</p>}
        </>
      );
  } else if (s.step === "appToken") {
    body = (
      <>
        <p>
          Installed. Last, {agent.name} keeps its own connection open to Slack, which takes an app-level token. Slack only makes those by hand, on the app's Basic Information page:
        </p>
        <a className="btn login-open" href={settings} target="_blank" rel="noopener noreferrer">
          Open Basic Information
        </a>
        <ol className="login-steps">
          <li>Under App-Level Tokens, choose Generate Token and Scopes.</li>
          <li>Name it switchyard, Add Scope connections:write, then Generate.</li>
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
        {s.syncError && (
          <>
            <p className="small automation-reason" role="status">
              Slack still shows {agent.name}'s old name or picture: {s.syncError}
            </p>
            <TokenField label="New configuration refresh token" placeholder="xoxe-…" busy={busy} button="Update Slack" onSubmit={(t) => void grant(t)} />
          </>
        )}
        <p className="muted small">
          Who can ask: the people allowed under{" "}
          <button className="btn-text note-link" onClick={() => actions.go({ page: "connectors" })}>
            Connectors, Slack
          </button>
          . {s.ownerName || "You"} {s.ownerName ? "is" : "are"} on that list. Anyone else who writes to {agent.name} shows there as a request.
        </p>
        <Mentions agent={agent.name} people={s.mentions ?? []} busy={busy} run={run} />
        <Watched agent={agent.name} handle={handle} channels={s.watched ?? []} busy={busy} run={run} />
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
  const steps = setupSteps(view, s);
  const at = !s ? 0 : s.step === "install" ? 1 : s.step === "appToken" ? 2 : steps.length;
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
        {at < steps.length && (
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

/** Who the agent's answers may @mention: anyone else stays plain text. */
function Mentions({ agent, people, busy, run }: { agent: string; people: { id: string; name: string }[]; busy: boolean; run(op: SlackAppOp, fields: Record<string, unknown>): Promise<unknown> }) {
  const [user, setUser] = useState("");
  const [name, setName] = useState("");
  const add = async () => {
    const r = (await run("slack.app.mention", { user: user.trim(), name: name.trim() })) as { error?: string };
    if (!r?.error) {
      setUser("");
      setName("");
    }
  };
  return (
    <div className="slack-mentions">
      <p className="muted small">
        {people.length === 0 ? `${agent} can't @mention anyone in Slack.` : `${agent} can @mention only these people in Slack:`}
      </p>
      {people.length > 0 && (
        <ul className="slack-mention-list">
          {people.map((p) => (
            <li key={p.id} className="row">
              <span>{p.name}</span>
              <span className="muted small">{p.id}</span>
              <span className="spacer" />
              <button className="btn-text danger" disabled={busy} onClick={() => void run("slack.app.unmention", { user: p.id })}>
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <input className="field" aria-label="Slack member ID" placeholder="Member ID, U0123ABCD" value={user} onChange={(e) => setUser(e.target.value)} />
        <input className="field" aria-label="Name" placeholder="Name" value={name} onChange={(e) => setName(e.target.value)} />
        <button className="btn" type="submit" disabled={busy || user.trim() === ""}>
          Allow
        </button>
      </form>
    </div>
  );
}

/** Channels whose new top-level messages and alerts reach the agent without
 *  a mention, as data it triages read-only. Only the owner sets them. */
function Watched({ agent, handle, channels, busy, run }: { agent: string; handle: string; channels: { id: string; name: string }[]; busy: boolean; run(op: SlackAppOp, fields: Record<string, unknown>): Promise<unknown> }) {
  const [channel, setChannel] = useState("");
  const [name, setName] = useState("");
  const add = async () => {
    const r = (await run("slack.app.watch", { channel: channel.trim(), name: name.trim() })) as { error?: string };
    if (!r?.error) {
      setChannel("");
      setName("");
    }
  };
  return (
    <div className="slack-mentions">
      <p className="muted small">
        {channels.length === 0
          ? `${agent} answers only when someone mentions it. Watch a channel and its new messages and alerts reach ${agent} too, a batch at a time: it looks into them without changing anything, and asks you before it acts. Add @${handle} to the channel first.`
          : `${agent} watches these channels: their new messages and alerts reach it, a batch at a time. It looks into them without changing anything, and asks you before it acts.`}
      </p>
      {channels.length > 0 && (
        <ul className="slack-mention-list">
          {channels.map((c) => (
            <li key={c.id} className="row">
              <span>{c.name}</span>
              <span className="muted small">{c.id}</span>
              <span className="spacer" />
              <button className="btn-text danger" disabled={busy} onClick={() => void run("slack.app.unwatch", { channel: c.id })}>
                Stop watching
              </button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <input className="field" aria-label="Slack channel ID" placeholder="Channel ID, C0123ABCD" value={channel} onChange={(e) => setChannel(e.target.value)} />
        <input className="field" aria-label="Channel name" placeholder="Name (optional)" value={name} onChange={(e) => setName(e.target.value)} />
        <button className="btn" type="submit" disabled={busy || channel.trim() === ""}>
          Watch
        </button>
      </form>
    </div>
  );
}
