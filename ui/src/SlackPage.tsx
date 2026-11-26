// The owner's own Slack in Switchyard: the sidebar's Slack section (his
// direct messages and group DMs, newest first, unread counts) and a
// conversation's page (messages grouped by author and day, Load older,
// threads in a pane, reactions, and a reply box that posts as him). yardd's
// slack/inbox.rs keeps them; Slack's text is untrusted and drawn as React
// elements only (slackText.ts).
import { Fragment, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { SlackMessage, SlackPage as Page } from "./protocol";
import type { Actions, Route, State } from "./store";
import { Icon, MenuButton } from "./App";
import { SLACK_MARK } from "./SlackSetup";
import { parseSlack, emojiChar, type SlackNode } from "./slackText";
import { callableAgents, calledAgents, initials, mentionQuery, pageKey, shownConversations, startsGroup, topLevel, type CallableAgent } from "./slackInbox";

const CLOSE = "M6 6l12 12M18 6L6 18";
const ARROW_UP = "M12 19V5M5 12l7-7 7 7";
/** Conversations the sidebar lists before Show all. */
const SIDEBAR_LIMIT = 8;
/** Reactions offered on a message: these first, the rest under More. */
const QUICK = ["thumbsup", "white_check_mark", "eyes"];
const MORE = ["joy", "pray", "tada", "heart", "fire", "raised_hands"];

export function SlackSidebar({ state, actions, route }: { state: State; actions: Actions; route: Route }) {
  const [all, setAll] = useState(false);
  const list = state.slackInbox;
  if (!list.length) return null;
  const open = route.page === "slack" ? `${route.team}/${route.channel}` : null;
  const shown = all ? list : shownConversations(list, SIDEBAR_LIMIT, open);
  return (
    <div className="agent-dms slack-dms" role="group" aria-label="Slack">
      <div className="list-head">Slack</div>
      {shown.map((c) => (
        <div key={`${c.team}/${c.id}`} className="session-item">
          <button
            className={`session-row slack-row ${open === `${c.team}/${c.id}` ? "active" : ""} ${c.unread > 0 ? "unread" : ""}`}
            title={c.preview ? `${c.previewFrom ? `${c.previewFrom}: ` : ""}${c.preview}` : c.name}
            onClick={() => actions.go({ page: "slack", team: c.team, channel: c.id })}
          >
            <span className="row-engine">
              <span className={`slack-face slack-face-${c.kind}`} aria-hidden="true">{c.kind === "mpim" ? c.name.split(",").length + 1 : c.kind === "channel" || c.kind === "group" ? "#" : initials(c.name)}</span>
            </span>
            <span className="session-title">{c.name}</span>
            {c.unread > 0 && (
              <span className="slack-unread" aria-label={`${c.unread} unread`}>
                {c.unread > 99 ? "99+" : c.unread}
              </span>
            )}
          </button>
        </div>
      ))}
      {list.length > shown.length && (
        <button className="list-head list-toggle" onClick={() => setAll(true)}>
          Show all {list.length}
        </button>
      )}
      {all && list.length > SIDEBAR_LIMIT && (
        <button className="list-head list-toggle" onClick={() => setAll(false)}>
          Show fewer
        </button>
      )}
    </div>
  );
}

export function SlackConversationPage({ state, actions, team, channel }: { state: State; actions: Actions; team: string; channel: string }) {
  const conversation = state.slackInbox.find((c) => c.team === team && c.id === channel);
  const page = state.slackPages[pageKey(team, channel)];
  const [thread, setThread] = useState<string | null>(null);
  const me = state.slack?.me?.userId ?? "";
  const connected = state.slack?.me?.step === "ready";
  const callable = callableAgents(state.agents, state.slack?.agentApps, team);
  useEffect(() => {
    setThread(null);
    actions.slackLoad(team, channel);
    // A conversation opened is loaded once; live messages keep it current.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [team, channel, state.connected]);
  // Read up to the newest message shown, here and in Slack.
  const newest = page?.messages.filter(topLevel).at(-1)?.ts;
  useEffect(() => {
    if (newest && conversation && newest > conversation.lastRead) actions.slackRead(team, channel, newest);
  }, [newest, conversation?.lastRead]);
  if (!conversation) {
    return (
      <div className="hosts slack-page">
        <header className="bar">
          <MenuButton />
          <h1>Slack</h1>
        </header>
        <div className="scroll">
          <p className="muted slack-empty">{state.slackInbox.length ? "This conversation is not in your Slack inbox." : "Connect your Slack under Connectors to see your direct messages here."}</p>
        </div>
      </div>
    );
  }
  const kind = { im: "Direct message", mpim: "Group direct message", channel: "Channel", group: "Private channel" }[conversation.kind];
  return (
    <div className={`hosts slack-page ${thread ? "slack-has-thread" : ""}`}>
      <header className="bar">
        <MenuButton />
        <Icon d={SLACK_MARK} size={16} />
        <h1>{conversation.name}</h1>
        <span className="muted small slack-kind">{kind}</span>
      </header>
      <div className="slack-body">
        <div className="slack-main">
          <MessageList
            page={page}
            me={me}
            onOlder={page?.more && page.messages.length ? () => actions.slackLoad(team, channel, { before: page.messages[0].ts }) : undefined}
            onThread={setThread}
            react={(ts, name, add) => actions.slackReact(team, channel, ts, name, add)}
            filter={topLevel}
          />
          <SlackComposer key={`${team}/${channel}`} placeholder={`Message ${conversation.name}`} disabled={!connected} agents={callable} send={(text, mentions) => actions.slackSend(team, channel, text, undefined, mentions)} />
        </div>
        {thread && <ThreadPane state={state} actions={actions} team={team} channel={channel} ts={thread} me={me} connected={connected} agents={callable} onClose={() => setThread(null)} />}
      </div>
    </div>
  );
}

function ThreadPane({ state, actions, team, channel, ts, me, connected, agents, onClose }: { state: State; actions: Actions; team: string; channel: string; ts: string; me: string; connected: boolean; agents: CallableAgent[]; onClose(): void }) {
  const page = state.slackPages[pageKey(team, channel, ts)];
  useEffect(() => {
    actions.slackLoad(team, channel, { thread: ts });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [team, channel, ts]);
  return (
    <aside className="slack-thread" aria-label="Thread">
      <header className="slack-thread-head">
        <strong>Thread</strong>
        <span className="spacer" />
        <button className="btn-icon" aria-label="Close thread" onClick={onClose}>
          <Icon d={CLOSE} size={14} />
        </button>
      </header>
      <MessageList page={page} me={me} react={(m, name, add) => actions.slackReact(team, channel, m, name, add)} />
      <SlackComposer key={ts} placeholder="Reply in the thread" disabled={!connected} agents={agents} send={(text, mentions) => actions.slackSend(team, channel, text, ts, mentions)} />
    </aside>
  );
}

function MessageList({ page, me, onOlder, onThread, react, filter }: {
  page: Page | undefined;
  me: string;
  onOlder?: () => void;
  onThread?: (ts: string) => void;
  react(ts: string, name: string, add: boolean): Promise<{ error?: string }>;
  filter?: (m: SlackMessage) => boolean;
}) {
  const box = useRef<HTMLDivElement>(null);
  const messages = (page?.messages ?? []).filter(filter ?? (() => true));
  const last = messages.at(-1)?.ts;
  const first = messages[0]?.ts;
  const atBottom = useRef(true);
  // New messages keep the view at the bottom when it was there; an older page
  // added on top keeps the reader where they were.
  const height = useRef(0);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    if (atBottom.current) el.scrollTop = el.scrollHeight;
  }, [last]);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    if (!atBottom.current && height.current) el.scrollTop += el.scrollHeight - height.current;
    height.current = el.scrollHeight;
  }, [first]);
  const users = page?.users ?? {};
  const name = (m: SlackMessage) => users[m.user] || m.username || m.user || "Someone";
  // On a touch screen a tap picks the message whose actions show.
  const [picked, setPicked] = useState<string | null>(null);
  return (
    <div
      className="slack-messages"
      ref={box}
      onScroll={(e) => {
        const el = e.currentTarget;
        atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        height.current = el.scrollHeight;
      }}
    >
      {onOlder && (
        <button className="btn-text slack-older" disabled={page?.loading} onClick={onOlder}>
          {page?.loading ? "Loading…" : "Load older messages"}
        </button>
      )}
      {!page && <p className="muted small slack-empty">Loading…</p>}
      {page && !messages.length && !page.loading && <p className="muted small slack-empty">No messages in the last 60 days.</p>}
      {messages.map((m, i) => {
        const prev = messages[i - 1];
        const day = !prev || new Date(Number(prev.ts) * 1000).toDateString() !== new Date(Number(m.ts) * 1000).toDateString();
        const head = startsGroup(prev, m);
        return (
          <Fragment key={m.ts}>
            {day && <div className="slack-day" role="separator"><span>{dayLabel(m.ts)}</span></div>}
            <MessageRow m={m} head={head} name={name(m)} users={users} me={me} onThread={onThread} react={react} picked={picked === m.ts} onPick={() => setPicked(picked === m.ts ? null : m.ts)} />
          </Fragment>
        );
      })}
    </div>
  );
}

function MessageRow({ m, head, name, users, me, onThread, react, picked, onPick }: {
  m: SlackMessage;
  head: boolean;
  name: string;
  users: Record<string, string>;
  me: string;
  onThread?: (ts: string) => void;
  react(ts: string, name: string, add: boolean): Promise<{ error?: string }>;
  picked: boolean;
  onPick(): void;
}) {
  const [more, setMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const toggle = async (r: string, add: boolean) => {
    setMore(false);
    const res = await react(m.ts, r, add);
    setError(res.error ?? null);
  };
  return (
    <div
      className={`slack-message ${head ? "slack-message-head" : ""} ${picked ? "picked" : ""}`}
      onClick={(e) => {
        if (!(e.target as HTMLElement).closest("button, a")) onPick();
      }}
    >
      <span className="slack-gutter">{head ? <span className="slack-face" aria-hidden="true">{initials(name)}</span> : <span className="slack-time-hover">{clock(m.ts)}</span>}</span>
      <div className="slack-content">
        {head && (
          <div className="slack-author">
            <strong>{name}</strong>
            <span className="muted small">{clock(m.ts)}</span>
          </div>
        )}
        {m.deleted ? (
          <p className="slack-text muted">This message was deleted.</p>
        ) : (
          <div className="slack-text">
            {render(parseSlack(m.text), users)}
            {m.edited && <span className="muted small"> (edited)</span>}
          </div>
        )}
        {m.files.length > 0 && (
          <div className="slack-files">
            {m.files.map((f, i) => (
              <span key={i} className="slack-file muted small">
                {f.name}
              </span>
            ))}
          </div>
        )}
        {m.reactions.length > 0 && (
          <div className="slack-reactions">
            {m.reactions.map((r) => {
              const mine = r.users.includes(me);
              return (
                <button
                  key={r.name}
                  className={`slack-reaction ${mine ? "mine" : ""}`}
                  aria-pressed={mine}
                  aria-label={`${r.name}, ${r.users.length}`}
                  title={r.users.map((u) => users[u] || u).join(", ")}
                  onClick={() => void toggle(r.name, !mine)}
                >
                  {emojiChar(r.name) ?? `:${r.name}:`} <span>{r.users.length}</span>
                </button>
              );
            })}
          </div>
        )}
        {onThread && m.replyCount > 0 && (
          <button className="btn-text btn-small slack-replies" onClick={() => onThread(m.ts)}>
            {m.replyCount === 1 ? "1 reply" : `${m.replyCount} replies`}
          </button>
        )}
        {error && <p className="small automation-reason" role="alert">{error}</p>}
      </div>
      {!m.deleted && (
        <div className="slack-actions" role="group" aria-label="Message actions">
          {(more ? [...QUICK, ...MORE] : QUICK).map((r) => (
            <button key={r} className="btn-icon slack-action" aria-label={`React ${r}`} title={`:${r}:`} onClick={() => void toggle(r, !m.reactions.some((x) => x.name === r && x.users.includes(me)))}>
              {emojiChar(r)}
            </button>
          ))}
          {!more && (
            <button className="btn-icon slack-action" aria-label="More reactions" onClick={() => setMore(true)}>
              +
            </button>
          )}
          {onThread && (
            <button className="btn-text btn-small" onClick={() => onThread(m.ts)}>
              Reply in thread
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function SlackComposer({ placeholder, disabled, agents, send }: { placeholder: string; disabled: boolean; agents: CallableAgent[]; send(text: string, mentions: string[]): Promise<{ error?: string }> }) {
  const [text, setText] = useState("");
  const [caret, setCaret] = useState(0);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  // Where the caret goes once a picked handle is in the box: placed right
  // after the redraw, before the next key can land.
  const caretAfter = useRef<number | null>(null);
  useLayoutEffect(() => {
    const at = caretAfter.current;
    if (at === null || !box.current) return;
    caretAfter.current = null;
    box.current.focus();
    box.current.setSelectionRange(at, at);
  }, [text]);
  const query = agents.length ? mentionQuery(text, caret) : null;
  const suggestions = query === null ? [] : agents.filter((a) => a.handle.includes(query) || a.name.toLowerCase().includes(query));
  const called = calledAgents(text, agents);
  const pick = (a: CallableAgent) => {
    const start = text.slice(0, caret).lastIndexOf("@");
    const next = `${text.slice(0, start)}@${a.handle} ${text.slice(caret)}`;
    setText(next);
    const at = start + a.handle.length + 2;
    setCaret(at);
    caretAfter.current = at;
  };
  const go = async () => {
    const t = text.trim();
    if (!t || sending) return;
    setSending(true);
    setError(null);
    const r = await send(t, calledAgents(t, agents));
    setSending(false);
    if (r.error) setError(r.error);
    else setText("");
  };
  return (
    <div className="slack-compose">
      {suggestions.length > 0 && (
        <div className="mention-list slack-mention-list" role="listbox" aria-label="Agents">
          {suggestions.map((a) => (
            <button key={a.id} role="option" aria-selected={false} className="picker-item menu-item" onMouseDown={(e) => e.preventDefault()} onClick={() => pick(a)}>
              <strong>{a.name}</strong>
              <span className="muted small">@{a.handle}</span>
            </button>
          ))}
        </div>
      )}
      <div className="composer">
        <textarea
          ref={box}
          rows={2}
          aria-label={placeholder}
          placeholder={disabled ? "Your Slack is not connected" : placeholder}
          value={text}
          disabled={disabled}
          onChange={(e) => {
            setText(e.target.value);
            setCaret(e.target.selectionStart ?? e.target.value.length);
          }}
          onSelect={(e) => setCaret(e.currentTarget.selectionStart ?? 0)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              if (suggestions.length === 1) pick(suggestions[0]);
              else void go();
            }
          }}
        />
        <div className="composer-row">
          <span className="muted small">
            {called.length
              ? `Calls ${agents.filter((a) => called.includes(a.id)).map((a) => a.name).join(", ")}: it answers here if its app is in this conversation`
              : agents.length
                ? "Sends as you, in Slack · @ to call an agent"
                : "Sends as you, in Slack"}
          </span>
          <button className="send" aria-label="Send" disabled={disabled || sending || !text.trim()} onClick={() => void go()}>
            {sending ? <span className="send-wait" aria-label="Sending" /> : <Icon d={ARROW_UP} width={2.5} />}
          </button>
        </div>
      </div>
      {error && <p className="small automation-reason" role="alert">{error}</p>}
    </div>
  );
}

/** The parsed text as React elements: never HTML. */
function render(nodes: SlackNode[], users: Record<string, string>): ReactNode[] {
  return nodes.map((n, i) => {
    switch (n.t) {
      case "text":
        return <Fragment key={i}>{n.text}</Fragment>;
      case "bold":
        return <strong key={i}>{render(n.children, users)}</strong>;
      case "italic":
        return <em key={i}>{render(n.children, users)}</em>;
      case "strike":
        return <s key={i}>{render(n.children, users)}</s>;
      case "code":
        return <code key={i}>{n.text}</code>;
      case "pre":
        return <pre key={i} className="slack-pre">{n.text}</pre>;
      case "quote":
        return <blockquote key={i} className="slack-quote">{render(n.children, users)}</blockquote>;
      case "link":
        return (
          <a key={i} href={n.href} target="_blank" rel="noopener noreferrer">
            {render(n.children, users)}
          </a>
        );
      case "mention":
        return (
          <span key={i} className="slack-mention">
            {n.kind === "user" ? `@${users[n.id] || n.label || n.id}` : n.label}
          </span>
        );
      case "emoji":
        return <span key={i} title={`:${n.name}:`}>{n.char ?? `:${n.name}:`}</span>;
    }
  });
}

function clock(ts: string): string {
  return new Date(Number(ts) * 1000).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function dayLabel(ts: string): string {
  const d = new Date(Number(ts) * 1000);
  const today = new Date();
  const yesterday = new Date(Date.now() - 86_400_000);
  if (d.toDateString() === today.toDateString()) return "Today";
  if (d.toDateString() === yesterday.toDateString()) return "Yesterday";
  return d.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric", ...(d.getFullYear() !== today.getFullYear() ? { year: "numeric" } : {}) });
}
