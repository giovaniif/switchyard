// The owner's own Slack in Switchyard: the sidebar's Slack section (his
// direct messages, group DMs and imported channels: favorites first, unread
// counts, a filter and an order he picks) and a conversation's page (messages
// drawn as Slack draws them: pictures, grouped by author and day, attachments
// as cards, Load older, threads in a pane he can resize, reactions, and a
// reply box with formatting that posts as him). yardd's slack/inbox.rs keeps
// them; Slack's text is untrusted and drawn as React elements only
// (slackText.ts).
import { Fragment, useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { SlackAttachment, SlackConversation, SlackMessage, SlackPage as Page, SlackPerson } from "./protocol";
import type { Actions, Route, State } from "./store";
import { Icon, MenuButton, Resizer, SectionToggle, useWidth } from "./App";
import { AnchoredPopover } from "./AnchoredPopover";
import { SLACK_MARK } from "./SlackSetup";
import { parseSlack, emojiChar, type SlackNode } from "./slackText";
import {
  applyFormat, applyLink, callableAgents, calledAgents, composerKey, initials, linkUrl, mentionablePeople, mentionedPeople, mentionQuery, pageKey, readListPrefs,
  sidebarConversations, startsGroup, suggestMentions, topLevel, SLACK_SHOW_LABEL, SLACK_SORT_LABEL, type CallableAgent, type SlackFormat, type SlackListPrefs,
} from "./slackInbox";

const CLOSE = "M6 6l12 12M18 6L6 18";
const ARROW_UP = "M12 19V5M5 12l7-7 7 7";
const CHECK = "m5 12 5 5 9-11";
const FILTER = "M4 6h16M7 12h10M10 18h4";
const STAR = "M12 3.5l2.6 5.4 5.9.8-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.8Z";
/** Conversations the sidebar lists before Show all. */
const SIDEBAR_LIMIT = 8;
/** Reactions offered on a message: these first, the rest under More. */
const QUICK = ["thumbsup", "white_check_mark", "eyes"];
const MORE = ["joy", "pray", "tada", "heart", "fire", "raised_hands"];
const LIST_KEY = "yard.slack.list";
const THREAD_W = 400;

/** A picture's address the page may load: what yardd lets through, checked again. */
const pictured = (src: string | undefined) => (src && /^(https:\/\/|http:\/\/127\.0\.0\.1:)/.test(src) ? src : undefined);

/** A person's (or an app's) picture; their initials when there is none, or it does not load. */
export function SlackFace({ name, src, className = "" }: { name: string; src?: string; className?: string }) {
  const [broken, setBroken] = useState<string | null>(null);
  const url = pictured(src);
  if (url && broken !== url) return <img className={`slack-face slack-face-img ${className}`} src={url} alt="" loading="lazy" referrerPolicy="no-referrer" onError={() => setBroken(url)} />;
  return <span className={`slack-face ${className}`} aria-hidden="true">{initials(name)}</span>;
}

function ConversationFace({ c }: { c: SlackConversation }) {
  if (c.kind === "im") return <SlackFace name={c.name} src={c.face} />;
  return <span className={`slack-face slack-face-${c.kind}`} aria-hidden="true">{c.kind === "mpim" ? c.name.split(",").length + 1 : "#"}</span>;
}

export function SlackSidebar({ state, actions, route, open: sectionOpen, onToggle }: { state: State; actions: Actions; route: Route; open: boolean; onToggle(): void }) {
  const [all, setAll] = useState(false);
  const [prefs, setPrefs] = useState<SlackListPrefs>(() => readListPrefs(localStorage.getItem(LIST_KEY)));
  const [menu, setMenu] = useState(false);
  const options = useRef<HTMLButtonElement>(null);
  const list = state.slackInbox;
  const sync = state.slackSync;
  // The first import shows here from its start, before anything is loaded.
  const importing = !!sync?.running && sync.first;
  if (!list.length && !importing) return null;
  const open = route.page === "slack" ? `${route.team}/${route.channel}` : null;
  const { shown, hidden } = sidebarConversations(list, prefs, SIDEBAR_LIMIT, open, all);
  const unread = list.reduce((n, c) => n + c.unread, 0);
  const set = (next: SlackListPrefs) => {
    setPrefs(next);
    try {
      localStorage.setItem(LIST_KEY, JSON.stringify(next));
    } catch {
      /* the choice just isn't remembered */
    }
  };
  const choice = <K extends keyof SlackListPrefs>(key: K, labels: Record<SlackListPrefs[K], string>) =>
    (Object.keys(labels) as SlackListPrefs[K][]).map((value) => (
      <button key={value} type="button" className="picker-item" role="menuitemradio" aria-checked={prefs[key] === value} onClick={() => set({ ...prefs, [key]: value })}>
        <span>{labels[value]}</span>
        {prefs[key] === value && <Icon d={CHECK} size={13} />}
      </button>
    ));
  return (
    <div className="agent-dms slack-dms" role="group" aria-label="Slack">
      <div className="section-head">
        <SectionToggle label="Slack" open={sectionOpen} onToggle={onToggle}>
          {sectionOpen && prefs.show !== "all" && <span className="muted section-filter">{prefs.show === "unread" ? "Unread" : "Favorites"}</span>}
          {!sectionOpen && unread > 0 && (
            <span className="slack-unread" aria-label={`${unread} unread`}>
              {unread > 99 ? "99+" : unread}
            </span>
          )}
        </SectionToggle>
        {sectionOpen && (
          <button ref={options} className={`btn-icon section-options ${prefs.show !== "all" || prefs.sort !== "recent" ? "on" : ""}`} aria-label="Slack list options" title="Filter and sort" aria-haspopup="menu" aria-expanded={menu} onClick={() => setMenu(!menu)}>
            <Icon d={FILTER} size={14} />
          </button>
        )}
      </div>
      {menu && (
        <AnchoredPopover anchor={options} label="Slack list options" variant="select-panel" onClose={() => setMenu(false)}>
          <div className="popover-heading"><strong>Show</strong></div>
          <div className="choice-menu" role="menu" aria-label="Show">{choice("show", SLACK_SHOW_LABEL)}</div>
          <div className="popover-heading"><strong>Sort</strong></div>
          <div className="choice-menu" role="menu" aria-label="Sort">{choice("sort", SLACK_SORT_LABEL)}</div>
        </AnchoredPopover>
      )}
      {sectionOpen && importing && sync && (
        <div className="slack-importing" role="status">
          <span className="muted small">{sync.total ? `Importing ${sync.done} of ${sync.total}` : "Finding your conversations"}</span>
          <SyncBar done={sync.done} total={sync.total} />
        </div>
      )}
      {sectionOpen &&
        shown.map((c) => (
          <div key={`${c.team}/${c.id}`} className="session-item">
            <button
              className={`session-row slack-row ${open === `${c.team}/${c.id}` ? "active" : ""} ${c.unread > 0 ? "unread" : ""}`}
              title={c.preview ? `${c.previewFrom ? `${c.previewFrom}: ` : ""}${c.preview}` : c.name}
              onClick={() => actions.go({ page: "slack", team: c.team, channel: c.id })}
            >
              <span className="row-engine">
                <ConversationFace c={c} />
              </span>
              {/* A channel's # is its mark already. */}
              <span className="session-title">{c.kind === "channel" || c.kind === "group" ? c.name.replace(/^#/, "") : c.name}</span>
              {c.favorite && (
                <span className="slack-star" role="img" aria-label="Favorite">
                  <Icon d={STAR} size={11} />
                </span>
              )}
              {c.unread > 0 && (
                <span className="slack-unread" aria-label={`${c.unread} unread`}>
                  {c.unread > 99 ? "99+" : c.unread}
                </span>
              )}
            </button>
          </div>
        ))}
      {sectionOpen && !shown.length && list.length > 0 && (
        <p className="muted small slack-none">
          {prefs.show === "unread" ? "Nothing unread." : "No favorites yet. Star a conversation from its page."}{" "}
          <button className="btn-text btn-small" onClick={() => set({ ...prefs, show: "all" })}>
            Show all
          </button>
        </p>
      )}
      {sectionOpen && hidden > 0 && (
        <button className="list-head list-toggle" onClick={() => setAll(true)}>
          Show all {list.length}
        </button>
      )}
      {sectionOpen && all && prefs.show === "all" && list.length > SIDEBAR_LIMIT && (
        <button className="list-head list-toggle" onClick={() => setAll(false)}>
          Show fewer
        </button>
      )}
    </div>
  );
}

/** How far a load is: a bar, moving on its own while the total is not known. */
export function SyncBar({ done, total }: { done: number; total: number }) {
  return (
    <span className={`slack-sync-bar ${total ? "" : "unknown"}`} role="progressbar" aria-label="Import progress" aria-valuemin={0} aria-valuemax={total || undefined} aria-valuenow={total ? done : undefined}>
      <span style={{ width: total ? `${Math.round((done / total) * 100)}%` : undefined }} />
    </span>
  );
}

export function SlackConversationPage({ state, actions, team, channel }: { state: State; actions: Actions; team: string; channel: string }) {
  const conversation = state.slackInbox.find((c) => c.team === team && c.id === channel);
  const page = state.slackPages[pageKey(team, channel)];
  const [thread, setThread] = useState<string | null>(null);
  const [threadW, setThreadW] = useWidth("yard.slack.thread.w", THREAD_W);
  const me = state.slack?.me?.userId ?? "";
  const connected = state.slack?.me?.step === "ready";
  const callable = callableAgents(state.agents, state.slack?.agentApps, team);
  // The workspace's people, for @: asked once per connection.
  const [people, setPeople] = useState<SlackPerson[]>([]);
  useEffect(() => {
    let live = true;
    if (connected) void actions.slackPeople().then((list) => live && setPeople(list));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connected, state.connected, team]);
  const mentionable = mentionablePeople(people);
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
    const importing = !!state.slackSync?.running;
    return (
      <div className="hosts slack-page">
        <header className="bar">
          <MenuButton />
          <h1>Slack</h1>
        </header>
        <div className="scroll">
          <p className="muted slack-empty">
            {importing ? "Your Slack is still importing. This conversation shows once it is loaded." : state.slackInbox.length ? "This conversation is not in your Slack inbox." : "Connect your Slack under Connectors to see your direct messages here."}
          </p>
        </div>
      </div>
    );
  }
  const kind = { im: "Direct message", mpim: "Group direct message", channel: "Channel", group: "Private channel" }[conversation.kind];
  const favorite = !!conversation.favorite;
  return (
    <div className={`hosts slack-page ${thread ? "slack-has-thread" : ""}`} style={{ "--slack-thread-w": `${threadW}px` } as CSSProperties}>
      <div className="slack-main">
        <header className="bar">
          <MenuButton />
          {(conversation.kind === "im" || conversation.kind === "mpim") && <ConversationFace c={conversation} />}
          <h1>{conversation.name}</h1>
          <span className="muted small slack-kind">{kind}</span>
          <button
            className={`btn-icon slack-favorite ${favorite ? "on" : ""}`}
            aria-pressed={favorite}
            aria-label={favorite ? `Remove ${conversation.name} from favorites` : `Add ${conversation.name} to favorites`}
            title={favorite ? "Remove from favorites" : "Add to favorites"}
            onClick={() => actions.slackFavorite(team, channel, !favorite)}
          >
            <Icon d={STAR} size={15} />
          </button>
        </header>
        <MessageList
          page={page}
          me={me}
          onOlder={page?.more && page.messages.length ? () => actions.slackLoad(team, channel, { before: page.messages[0].ts }) : undefined}
          onThread={setThread}
          react={(ts, name, add) => actions.slackReact(team, channel, ts, name, add)}
          filter={topLevel}
        />
        <SlackComposer key={`${team}/${channel}`} placeholder={`Message ${conversation.name}`} disabled={!connected} agents={callable} people={mentionable} send={(text, mentions, who) => actions.slackSend(team, channel, text, undefined, mentions, who)} />
      </div>
      {thread && (
        <aside className="slack-thread" aria-label="Thread">
          <Resizer width={threadW} min={300} max={720} initial={THREAD_W} dir={-1} cssVar="--slack-thread-w" label="Resize the thread" onWidth={setThreadW} />
          <ThreadPane state={state} actions={actions} team={team} channel={channel} ts={thread} me={me} connected={connected} agents={callable} people={mentionable} name={conversation.name} onClose={() => setThread(null)} />
        </aside>
      )}
    </div>
  );
}

function ThreadPane({ state, actions, team, channel, ts, me, connected, agents, people, name, onClose }: { state: State; actions: Actions; team: string; channel: string; ts: string; me: string; connected: boolean; agents: CallableAgent[]; people: CallableAgent[]; name: string; onClose(): void }) {
  const page = state.slackPages[pageKey(team, channel, ts)];
  useEffect(() => {
    actions.slackLoad(team, channel, { thread: ts });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [team, channel, ts]);
  return (
    <>
      <header className="bar slack-thread-head">
        <h2>Thread</h2>
        <span className="muted small slack-kind">{name}</span>
        <span className="spacer" />
        <button className="btn-icon" aria-label="Close thread" onClick={onClose}>
          <Icon d={CLOSE} size={14} />
        </button>
      </header>
      <MessageList page={page} me={me} react={(m, name, add) => actions.slackReact(team, channel, m, name, add)} thread />
      <SlackComposer key={ts} placeholder="Reply in the thread" disabled={!connected} agents={agents} people={people} send={(text, mentions, who) => actions.slackSend(team, channel, text, ts, mentions, who)} />
    </>
  );
}

function MessageList({ page, me, onOlder, onThread, react, filter, thread }: {
  page: Page | undefined;
  me: string;
  onOlder?: () => void;
  onThread?: (ts: string) => void;
  react(ts: string, name: string, add: boolean): Promise<{ error?: string }>;
  filter?: (m: SlackMessage) => boolean;
  /** A thread: its first message stands apart from the replies. */
  thread?: boolean;
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
  const faces = page?.faces ?? {};
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
        // In a thread the first message is the one answered: the replies start a group of their own.
        const head = startsGroup(prev, m) || (!!thread && i === 1);
        return (
          <Fragment key={m.ts}>
            {day && <div className="slack-day" role="separator"><span>{dayLabel(m.ts)}</span></div>}
            {thread && i === 1 && <div className="slack-day slack-reply-count" role="separator"><span>{messages.length === 2 ? "1 reply" : `${messages.length - 1} replies`}</span></div>}
            <MessageRow m={m} head={head} name={name(m)} face={m.icon || faces[m.user]} users={users} me={me} onThread={onThread} react={react} picked={picked === m.ts} onPick={() => setPicked(picked === m.ts ? null : m.ts)} />
          </Fragment>
        );
      })}
    </div>
  );
}

function MessageRow({ m, head, name, face, users, me, onThread, react, picked, onPick }: {
  m: SlackMessage;
  head: boolean;
  name: string;
  face?: string;
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
      <span className="slack-gutter">{head ? <SlackFace name={name} src={face} /> : <span className="slack-time-hover">{clock(m.ts)}</span>}</span>
      <div className="slack-content">
        {head && (
          <div className="slack-author">
            <strong>{name}</strong>
            {(m.icon || m.subtype === "bot_message") && <span className="slack-app-tag">APP</span>}
            <span className="muted small">{clock(m.ts)}</span>
          </div>
        )}
        {m.deleted ? (
          <p className="slack-text muted">This message was deleted.</p>
        ) : (
          <>
            {(m.text || m.edited) && (
              <div className="slack-text">
                {render(parseSlack(m.text), users)}
                {m.edited && <span className="muted small"> (edited)</span>}
              </div>
            )}
            {m.attachments?.map((a, i) => <Attachment key={i} a={a} users={users} />)}
          </>
        )}
        {m.files.length > 0 && (
          <div className="slack-files">
            {m.files.map((f, i) => {
              const what = f.kind?.startsWith("image/") ? "Image" : f.kind?.startsWith("video/") ? "Video" : f.kind?.startsWith("audio/") ? "Audio" : "File";
              const words = (
                <>
                  <span className="slack-file-kind">{what}</span>
                  {f.name}
                </>
              );
              // Its page in Slack: the file itself is not kept or loaded here.
              return f.url && /^https:\/\//.test(f.url) ? (
                <a key={i} className="slack-file small" href={f.url} target="_blank" rel="noopener noreferrer" title="Open in Slack">
                  {words}
                </a>
              ) : (
                <span key={i} className="slack-file small">{words}</span>
              );
            })}
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
          <button className="slack-replies" onClick={() => onThread(m.ts)}>
            <strong>{m.replyCount === 1 ? "1 reply" : `${m.replyCount} replies`}</strong>
            <span className="muted small">View thread</span>
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

/** An attachment as Slack draws one: a coloured bar, then its author, title
 *  (a link when it has one), text, fields and footer. */
function Attachment({ a, users }: { a: SlackAttachment; users: Record<string, string> }) {
  const color = a.color === "good" ? "var(--ok)" : a.color === "warning" ? "var(--warn)" : a.color === "danger" ? "var(--bad-ink)" : a.color && /^#[0-9a-f]{6}$/i.test(a.color) ? a.color : undefined;
  const link = a.titleLink && /^https?:\/\//i.test(a.titleLink) ? a.titleLink : null;
  return (
    <>
      {a.pretext && <div className="slack-text">{render(parseSlack(a.pretext), users)}</div>}
      <div className="slack-attachment" style={color ? ({ "--bar": color } as CSSProperties) : undefined}>
        {a.author && <div className="slack-attachment-author">{a.author}</div>}
        {a.title && (
          <div className="slack-attachment-title">
            {link ? (
              <a href={link} target="_blank" rel="noopener noreferrer">
                {render(parseSlack(a.title), users)}
              </a>
            ) : (
              render(parseSlack(a.title), users)
            )}
          </div>
        )}
        {a.text && <div className="slack-text">{render(parseSlack(a.text), users)}</div>}
        {a.fields && a.fields.length > 0 && (
          <dl className="slack-fields">
            {a.fields.map((f, i) => (
              <div key={i} className={f.short ? "short" : ""}>
                {f.title && <dt>{f.title}</dt>}
                <dd className="slack-text">{render(parseSlack(f.value), users)}</dd>
              </div>
            ))}
          </dl>
        )}
        {a.footer && <div className="slack-attachment-footer muted small">{a.footer}</div>}
      </div>
    </>
  );
}

const FORMATS: { format: SlackFormat; label: string; d: string; keys?: string }[] = [
  { format: "bold", label: "Bold", d: "M7 5h6a3.5 3.5 0 0 1 0 7H7ZM7 12h7a3.5 3.5 0 0 1 0 7H7Z", keys: "b" },
  { format: "italic", label: "Italic", d: "M10 5h8M6 19h8M14 5l-4 14", keys: "i" },
  { format: "strike", label: "Strikethrough", d: "M4 12h16M16 7.5C15.5 6 14 5 12 5c-2.5 0-4 1.3-4 3 0 1 .5 1.8 1.5 2.5M8 16.500c.5 1.500 2 2.500 4 2.500 2.500 0 4-1.300 4-3 0-.700-.200-1.300-.700-1.800" },
  { format: "code", label: "Code", d: "M8 8l-4 4 4 4M16 8l4 4-4 4" },
  { format: "block", label: "Code block", d: "M4 5h16v14H4ZM9 10l-2 2 2 2M15 10l2 2-2 2" },
  { format: "quote", label: "Quote", d: "M5 5v14M10 8h9M10 12h9M10 16h6" },
  { format: "bullets", label: "Bulleted list", d: "M9 7h11M9 12h11M9 17h11M4.5 7h.01M4.5 12h.01M4.5 17h.01" },
  { format: "numbers", label: "Numbered list", d: "M10 7h10M10 12h10M10 17h10M4 5.500l1.500-.500v4M4 14.500c0-1 2.500-1 2.500.200 0 .800-2.500 1.800-2.500 3.300h2.500" },
];
const LINK = "M10 14a4 4 0 0 0 5.700 0l3-3a4 4 0 0 0-5.700-5.700l-1 1M14 10a4 4 0 0 0-5.700 0l-3 3a4 4 0 0 0 5.700 5.700l1-1";

function SlackComposer({ placeholder, disabled, agents, people, send }: { placeholder: string; disabled: boolean; agents: CallableAgent[]; people: CallableAgent[]; send(text: string, mentions: string[], people: string[]): Promise<{ error?: string }> }) {
  const [text, setText] = useState("");
  const [caret, setCaret] = useState(0);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  // The selection to restore once a picked handle or a format is in the box:
  // placed right after the redraw, before the next key can land.
  const selectAfter = useRef<[number, number] | null>(null);
  useLayoutEffect(() => {
    const at = selectAfter.current;
    if (at === null || !box.current) return;
    selectAfter.current = null;
    box.current.focus();
    box.current.setSelectionRange(at[0], at[1]);
  }, [text]);
  // The box grows with what is written, up to its CSS limit.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [text]);
  // The @ list: its highlighted row, and the text it was closed on by Escape
  // (it stays closed until he types).
  const [active, setActive] = useState(0);
  const [dismissed, setDismissed] = useState<string | null>(null);
  // People he picked from the list: of two with one name, the picked one is meant.
  const [picked, setPicked] = useState<string[]>([]);
  const listId = useId();
  const query = agents.length || people.length ? mentionQuery(text, caret) : null;
  const suggestions = query === null || dismissed === text ? [] : suggestMentions(agents, people, query);
  const highlighted = Math.min(active, suggestions.length - 1);
  useEffect(() => setActive(0), [query, suggestions.length]);
  // A long list scrolls to keep the highlighted row in sight.
  useEffect(() => {
    if (suggestions.length) document.getElementById(`${listId}-${highlighted}`)?.scrollIntoView({ block: "nearest" });
  }, [listId, highlighted, suggestions.length]);
  const called = calledAgents(text, agents);
  const mentioned = mentionedPeople(text, people, picked);
  const edit = (next: { text: string; start: number; end: number }) => {
    setText(next.text);
    setCaret(next.end);
    selectAfter.current = [next.start, next.end];
  };
  const pick = (a: CallableAgent) => {
    const start = text.slice(0, caret).lastIndexOf("@");
    const next = `${text.slice(0, start)}@${a.handle} ${text.slice(caret)}`;
    const at = start + a.handle.length + 2;
    if (a.person) setPicked((was) => [...was.filter((id) => id !== a.id), a.id]);
    edit({ text: next, start: at, end: at });
  };
  const selection = (): [number, number] => [box.current?.selectionStart ?? text.length, box.current?.selectionEnd ?? text.length];
  const format = (f: SlackFormat) => {
    const [start, end] = selection();
    edit(applyFormat(text, start, end, f));
  };
  // The link form: its address, and the selection it will replace.
  const [link, setLink] = useState<{ url: string; label: string; start: number; end: number } | null>(null);
  const linkButton = useRef<HTMLButtonElement>(null);
  const openLink = () => {
    const [start, end] = selection();
    setLink({ url: "", label: text.slice(start, end), start, end });
  };
  const url = link ? linkUrl(link.url) : null;
  const addLink = () => {
    if (!link || !url) return;
    edit(applyLink(text, link.start, link.end, url, link.label));
    setLink(null);
  };
  const go = async () => {
    const t = text.trim();
    if (!t || sending) return;
    setSending(true);
    setError(null);
    const r = await send(t, calledAgents(t, agents), mentionedPeople(t, people, picked).map((p) => p.id));
    setSending(false);
    if (r.error) setError(r.error);
    else {
      setText("");
      setPicked([]);
    }
  };
  const hint = [
    called.length ? `Calls ${agents.filter((a) => called.includes(a.id)).map((a) => a.name).join(", ")}: it answers here if its app is in this conversation` : "",
    mentioned.length ? `Mentions ${mentioned.map((p) => p.name).join(", ")}` : "",
  ].filter(Boolean).join(" · ");
  return (
    <div className="slack-compose">
      {suggestions.length > 0 && (
        <div className="mention-list slack-mention-list" id={listId} role="listbox" aria-label="People and agents">
          {suggestions.map((a, i) => (
            <button
              key={`${a.person ? "p" : "a"}-${a.id}`}
              id={`${listId}-${i}`}
              role="option"
              aria-selected={i === highlighted}
              tabIndex={-1}
              className={`picker-item menu-item ${i === highlighted ? "active" : ""}`}
              // Keep the caret in the box while the pick is made.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => pick(a)}
            >
              {a.person && <SlackFace name={a.name} src={a.face} />}
              <strong>{a.name}</strong>
              {!a.person && <span className="muted small">@{a.handle}</span>}
            </button>
          ))}
        </div>
      )}
      <div className="composer slack-composer">
        <div className="slack-format" role="toolbar" aria-label="Formatting">
          {FORMATS.map((f) => (
            <button key={f.format} type="button" className="btn-icon" aria-label={f.label} title={f.label} disabled={disabled} onMouseDown={(e) => e.preventDefault()} onClick={() => format(f.format)}>
              <Icon d={f.d} size={15} />
            </button>
          ))}
          <button ref={linkButton} type="button" className="btn-icon" aria-label="Link" title="Link" aria-haspopup="dialog" aria-expanded={!!link} disabled={disabled} onClick={openLink}>
            <Icon d={LINK} size={15} />
          </button>
        </div>
        {link && (
          <AnchoredPopover anchor={linkButton} label="Add a link" onClose={() => setLink(null)}>
            <form
              className="slack-link-form"
              onSubmit={(e) => {
                e.preventDefault();
                addLink();
              }}
            >
              <label>
                <span className="muted small">Text</span>
                <input className="field" value={link.label} placeholder="What it says" onChange={(e) => setLink({ ...link, label: e.target.value })} />
              </label>
              <label>
                <span className="muted small">Link</span>
                <input className="field" autoFocus value={link.url} placeholder="https://" inputMode="url" onChange={(e) => setLink({ ...link, url: e.target.value })} />
              </label>
              <div className="composer-row">
                <span className="spacer" />
                <button type="button" className="btn" onClick={() => setLink(null)}>
                  Cancel
                </button>
                <button type="submit" className="btn-primary" disabled={!url}>
                  Add link
                </button>
              </div>
            </form>
          </AnchoredPopover>
        )}
        <textarea
          ref={box}
          rows={1}
          aria-label={placeholder}
          placeholder={disabled ? "Your Slack is not connected" : placeholder}
          value={text}
          disabled={disabled}
          aria-autocomplete={agents.length || people.length ? "list" : undefined}
          aria-expanded={agents.length || people.length ? suggestions.length > 0 : undefined}
          aria-controls={suggestions.length ? listId : undefined}
          aria-activedescendant={suggestions.length ? `${listId}-${highlighted}` : undefined}
          onChange={(e) => {
            setText(e.target.value);
            setCaret(e.target.selectionStart ?? e.target.value.length);
            setDismissed(null);
          }}
          onSelect={(e) => setCaret(e.currentTarget.selectionStart ?? 0)}
          onKeyDown={(e) => {
            // Slack's own keys for the marks.
            const mark = (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey ? FORMATS.find((f) => f.keys === e.key.toLowerCase()) : undefined;
            if (mark) {
              e.preventDefault();
              format(mark.format);
              return;
            }
            const k = composerKey({ key: e.key, shiftKey: e.shiftKey, composing: e.nativeEvent.isComposing }, suggestions, highlighted, query);
            if (!k) return;
            e.preventDefault();
            if (k.do === "move") setActive(k.to);
            else if (k.do === "pick") pick(k.agent);
            else if (k.do === "send") void go();
            else {
              // Only the list closes, not the thread pane or the drawer behind it.
              e.stopPropagation();
              setDismissed(text);
            }
          }}
        />
        <div className="composer-row">
          <span className="muted small">{hint || "Sends as you, in Slack · @ to mention someone"}</span>
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
