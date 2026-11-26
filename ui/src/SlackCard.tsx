// The Slack connector on the Connectors page: setup (a manifest to paste at
// api.slack.com and the two tokens), the connection's state, who may talk to
// agents, and which agent answers a message that names none. yardd keeps the
// tokens; this page only ever sees their last four characters.
import { useState } from "react";
import type { SlackView } from "./protocol";
import type { Actions, State } from "./store";
import { SelectPicker } from "./SelectPicker";
import { allowedSummary, createAppUrl, manifestText, slackStatus, tokenProblem } from "./slack";

export function SlackCard({ state, actions }: { state: State; actions: Actions }) {
  const view = state.slack;
  const [replacing, setReplacing] = useState(false);
  const [confirm, setConfirm] = useState(false);
  if (!view) return null;
  const status = slackStatus(view);
  const agents = state.agents;
  const nameOf = (id: string | null) => agents.find((a) => a.id === id)?.name;
  const agentOptions = [{ value: "", label: "None: ask which agent" }, ...agents.map((a) => ({ value: a.id, label: a.name }))];
  return (
    <section className={`host-card slack-card ${view.state === "error" ? "connectors-card-attention" : ""}`} aria-label="Slack">
      <header className="connectors-card-head">
        <span className="connector-account-icon connector-account-slack" aria-hidden="true">#</span>
        <div className="connector-account-name">
          <strong>Slack</strong>
          <span className={`small ${status.tone === "bad" ? "bad" : "muted"} slack-status-line`}>{status.line}</span>
        </div>
        <span className={`connector-account-state slack-state-${status.tone}`} role="status">{status.label}</span>
        {view.state === "error" && (
          <button className="btn btn-small" onClick={() => actions.slack("slack.retry")}>
            Retry
          </button>
        )}
      </header>
      {!view.configured || replacing ? (
        <SlackSetup view={view} actions={actions} replacing={view.configured} onDone={() => setReplacing(false)} />
      ) : (
        <>
          <ul className="connector-list">
            {view.requests.map((r) => (
              <li key={r.id} className="connector">
                <div className="connector-row">
                  <span className="connector-id">
                    <span className="dot connector-dot-needs-auth" aria-hidden="true" />
                    <span className="connector-name">{r.name}</span>
                    <span className="muted small connector-via">{r.place === "Direct message" ? "wrote by direct message" : `wrote in ${r.place}`} · {r.id}</span>
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
                <p className="muted small slack-hint">{allowedSummary(view)} Anyone else who writes to it is ignored.</p>
              </li>
            )}
            <li className="connector">
              <div className="connector-row">
                <span className="connector-id">
                  <span className="dot dot-spacer" aria-hidden="true" />
                  <span className="connector-name">Default agent</span>
                  <span className="muted small connector-via">for messages that name no agent</span>
                </span>
                <span className="connector-side">
                  <SelectPicker label="Default agent" value={view.defaultAgent ?? ""} options={agentOptions} onChange={(agent) => actions.slack("slack.route", { agent: agent || null })} />
                </span>
              </div>
            </li>
            {view.channels.map((ch) => (
              <li key={ch.id} className="connector">
                <div className="connector-row">
                  <span className="connector-id">
                    <span className="dot dot-spacer" aria-hidden="true" />
                    <span className="connector-name">{ch.name}</span>
                    <span className="muted small connector-via">channel</span>
                  </span>
                  <span className="connector-side">
                    <SelectPicker
                      label={`Agent for ${ch.name}`}
                      value={ch.agent ?? ""}
                      options={[{ value: "", label: view.defaultAgent && nameOf(view.defaultAgent) ? `Default (${nameOf(view.defaultAgent)})` : "Default" }, ...agents.map((a) => ({ value: a.id, label: a.name }))]}
                      onChange={(agent) => actions.slack("slack.route", { channel: ch.id, agent: agent || null })}
                    />
                  </span>
                </div>
              </li>
            ))}
          </ul>
          <footer className="connector-card-footer">
            <span className="muted small slack-tokens">
              {view.appToken} · {view.botToken}
            </span>
            <span className="slack-footer-actions">
              <button className="btn-text btn-small" onClick={() => setReplacing(true)}>
                Replace tokens
              </button>
              <button
                className={`btn-text btn-small ${confirm ? "bad" : ""}`}
                onClick={() => (confirm ? actions.slack("slack.disconnect") : setConfirm(true))}
                onBlur={() => setConfirm(false)}
                aria-label={confirm ? "Confirm disconnecting Slack" : "Disconnect Slack"}
              >
                {confirm ? "Confirm" : "Disconnect"}
              </button>
            </span>
          </footer>
        </>
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

/** The guide and the token form: shown until both tokens are set, and to replace them. */
function SlackSetup({ view, actions, replacing, onDone }: { view: SlackView; actions: Actions; replacing: boolean; onDone(): void }) {
  const [appToken, setAppToken] = useState("");
  const [botToken, setBotToken] = useState("");
  const [copied, setCopied] = useState(false);
  const [showManifest, setShowManifest] = useState(false);
  const appProblem = tokenProblem("app", appToken);
  const botProblem = tokenProblem("bot", botToken);
  // Replacing: either token alone; the other is kept.
  const ready = replacing ? (appToken.trim() || botToken.trim()) && !appProblem && !botProblem : appToken.trim() && botToken.trim() && !appProblem && !botProblem;
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(manifestText());
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setShowManifest(true);
    }
  };
  return (
    <div className="slack-setup">
      {!replacing && (
        <ol className="slack-steps">
          <li>
            <span>
              Create a Slack app from this manifest. It turns on Socket Mode, so this host needs no public address.
            </span>
            <span className="slack-step-actions">
              <a className="btn btn-small" href={createAppUrl()} target="_blank" rel="noreferrer">
                Create on Slack
              </a>
              <button className="btn btn-small" onClick={copy}>
                {copied ? "Copied" : "Copy manifest"}
              </button>
              <button className="btn btn-small" aria-expanded={showManifest} onClick={() => setShowManifest(!showManifest)}>
                {showManifest ? "Hide manifest" : "Show manifest"}
              </button>
            </span>
            {showManifest && <pre className="slack-manifest">{manifestText()}</pre>}
          </li>
          <li><span>Install it to your workspace, then copy the Bot User OAuth Token from OAuth &amp; Permissions.</span></li>
          <li><span>Under Basic Information, App-Level Tokens, generate a token with the connections:write scope.</span></li>
          <li><span>Paste both here. Then send the bot a direct message and allow yourself on this card.</span></li>
        </ol>
      )}
      <form
        className="slack-token-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (!ready) return;
          actions.slack("slack.tokens", { appToken: appToken.trim(), botToken: botToken.trim() });
          setAppToken("");
          setBotToken("");
          onDone();
        }}
      >
        <label className="slack-field">
          <span className="small">App-level token</span>
          <input className="field" type="password" autoComplete="off" spellCheck={false} placeholder={view.appToken || "xapp-…"} value={appToken} onChange={(e) => setAppToken(e.target.value)} />
          {appProblem && <span className="bad small">{appProblem}</span>}
        </label>
        <label className="slack-field">
          <span className="small">Bot token</span>
          <input className="field" type="password" autoComplete="off" spellCheck={false} placeholder={view.botToken || "xoxb-…"} value={botToken} onChange={(e) => setBotToken(e.target.value)} />
          {botProblem && <span className="bad small">{botProblem}</span>}
        </label>
        <div className="slack-form-actions">
          <span className="muted small">Kept on this host only, readable by you alone.</span>
          {replacing && (
            <button type="button" className="btn-text btn-small" onClick={onDone}>
              Cancel
            </button>
          )}
          <button className="btn-primary" disabled={!ready}>
            {replacing ? "Save" : "Connect"}
          </button>
        </div>
      </form>
    </div>
  );
}
