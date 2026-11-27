// Shapes shared with yardd (crates/yardd). Keep the two in step.

export type Engine = "claude" | "codex" | "opencode" | "grok" | "gemini";
export type Status = "idle" | "queued" | "running" | "needs_approval" | "error";
export type Decision = "allow" | "allow_always" | "deny";

/** Permission modes, like Claude Code's: ask first, accept edits, plan only, full access. */
export type Permission = "ask" | "acceptEdits" | "plan" | "full";

export const SESSION_TITLE_LIMIT = 60;

export interface SessionMeta {
  id: string;
  title: string;
  /** Where the title came from: absent for the task's first line, `auto` when
   *  a small model named it from the conversation, `user` when typed in. */
  titleSource?: "auto" | "user";
  engine: Engine;
  model: string;
  effort: string;
  serviceTier?: string;
  cwd: string;
  status: Status;
  /** `local`, or the id of the remote host it runs on. */
  host: string;
  /** Set when yardd made a worktree for the session. */
  branch?: string | null;
  repo?: string | null;
  /** The session this one was handed off from. */
  /** How much the engine may do unasked. Missing on sessions from an older yardd: `ask`. */
  permission?: Permission;
  /** `default`, or the id of an added account (see accounts.ts). Missing on sessions from an older yardd. */
  account?: string;
  parent?: string | null;
  /** Kept at the top of the sidebar. */
  pinned?: boolean;
  /** Moved into the sidebar's Archived section. */
  archived?: boolean;
  /** The sidebar group it sits in; a group exists only while a session is in it. */
  group?: string | null;
  /** The automation that started this session: a run, kept out of the sidebar. */
  automation?: string | null;
  /** The agent this session is a chat with (see agents.ts). */
  agent?: string | null;
  slackPlace?: string;
  slackAccess?: { id: string };
  /** An agent's conversation: the start of its last answer, for its card. */
  lastMessage?: string | null;
  /** An agent's conversation starts a fresh engine thread with its next turn, and why. */
  freshContext?: string | null;
  /** The agent asked for the owner (`notify_owner`): kept until a window opens its conversation (`agent.seen`). */
  needsYou?: { message: string; at: number } | null;
  /** Subagents still at work after their turn ended (Agent call ids). */
  background?: string[];
  /** Where the messages the running turn answers came from (yardd owes them its answer). */
  answering?: MessageOrigin[];
  /** When a run's first turn ended; `updatedAt` moves on any later change. */
  finishedAt?: number;
  /** When the latest turn ended (yardd's clock), and how. Missing before the first, and from an older yardd. */
  turnEndedAt?: number | null;
  lastTurn?: "completed" | "failed" | "interrupted" | null;
  /** Messages sent while a turn ran, oldest first. The next one starts when a turn completes; Stop or a failure holds them. */
  pending?: PendingMessage[];
  createdAt: number;
  updatedAt: number;
}

export interface PendingMessage {
  id: string;
  text: string;
  attachments?: Attachment[];
  at: number;
  origin?: MessageOrigin;
}

/** Where a message into a conversation came from, when the owner did not
 *  type it (yardd's origins.rs): another agent's `message_agent` (`agent`),
 *  the answer to one delivered back (`agentReply`), or a Slack message
 *  (`slack`). `hops` counts the chain of agents messaging agents. Absent, or
 *  `user`: typed by the owner. */
export type MessageOrigin =
  | { kind: "user" }
  /** `handle`: the sender's `@handle` (none from a yardd before handles).
   *  `readOnly`: sent on from a turn that worked on a watched Slack notice,
   *  so the turn that reads it is read-only too. */
  | { kind: "agent" | "agentReply"; agentId: string; name: string; handle?: string; hops: number; readOnly?: boolean }
  /** `slack` (and kinds this build does not know); `isSlack` narrows to SlackOrigin. */
  | { kind: string; [field: string]: unknown };

/** A message from Slack: where it was written and by whom (yardd's slack.rs). */
export interface SlackOrigin {
  kind: "slack";
  team: string;
  channel: string;
  /** `#general`, or `Direct message`. */
  channelName: string;
  /** The Slack thread the answer is posted in. */
  threadTs: string;
  ts: string;
  user: string;
  userName: string;
  /** Messages from a channel the agent watches, batched into one notice
   *  (`count` of them; the first is `ts`): data, and its turn is read-only. */
  watched?: boolean;
  count?: number;
}

/** The Slack connection and its settings (yardd's slack.rs). Tokens never
 *  come here: only their kind and last four characters. */
export interface SlackPlace {
  key: string;
  agentId: string;
  sessionId: string;
  name: string;
  at: number;
}

export interface SlackView {
  /** The owner's own Slack app: none until he starts connecting it (older yardds send none). */
  me?: SlackMe | null;
  /** People whose Slack requests may use tools. */
  allowed: { id: string; name: string }[];
  places?: SlackPlace[];
  /** Workspaces with a configuration token: an agent's own app there is one click (older yardds send none). */
  workspaces?: { teamId: string; teamName: string }[];
  /** Agents with a Slack app of their own, by agent id. */
  agentApps?: Record<string, SlackAgentApp>;
  /** The public https address Slack sends agents' new apps' events to; empty: Socket Mode. */
  publicUrl?: string;
  /** Set by YARD_SLACK_PUBLIC_URL where Switchyard runs, so not from here. */
  publicUrlFixed?: boolean;
  /** Whether that address reached this Switchyard: null untried, "" yes, else why not. */
  publicCheck?: string | null;
  /** A new app's install can come back to Switchyard on its own (an https address). */
  canRedirect?: boolean;
  /** Each remote host's side of it, for an agent there to get its own app
   *  there: the host's workspaces and addresses. Its agents' apps are in
   *  `agentApps` under their prefixed ids. */
  hosts?: Record<string, { workspaces?: { teamId: string; teamName: string }[]; canRedirect?: boolean; publicUrl?: string }>;
}

/** The owner's own Slack app (yardd's slack.rs `me`): user scopes, his
 *  direct messages and channels. Its step and connection, never a token. */
export interface SlackMe {
  /** `install`: approve it in Slack; `appToken`: its connection token is missing; `ready`. */
  step: "install" | "appToken" | "ready";
  appId: string;
  teamId: string;
  teamName: string;
  /** The owner's name in Slack, once approved. */
  userName: string;
  /** His Slack user id (older yardds send none). */
  userId?: string;
  /** Slack can send the browser back to Switchyard after the install (an https address). */
  canRedirect: boolean;
  authorizeUrl?: string;
  /** Each token's kind and last four characters (`xoxp-…abcd`), empty when not set. */
  userToken: string;
  appToken: string;
  /** Once ready: the connection. */
  state?: "connecting" | "connected" | "error" | "off";
  /** His handle in Slack, from the connection. */
  handle?: string;
  error?: string | null;
}

/** One of the owner's own Slack conversations (yardd's slack/inbox.rs). */
export interface SlackConversation {
  team: string;
  id: string;
  kind: "im" | "mpim" | "channel" | "group";
  /** A DM: the other person; a group DM: the others; a channel: `#name`. */
  name: string;
  /** A DM's other person's id. */
  peer: string;
  lastTs: string;
  lastRead: string;
  unread: number;
  preview?: string;
  previewFrom?: string;
}

/** A channel the owner is in (`slack.inbox.channels`). */
export interface SlackChannel {
  id: string;
  /** `#name`. */
  name: string;
  private: boolean;
  imported: boolean;
}

/** A Slack message as kept: its text is Slack's markup (see slackText.ts). */
export interface SlackMessage {
  ts: string;
  /** Set on a thread reply: its parent's ts. */
  threadTs?: string;
  user: string;
  /** An integration's or a bot's name, when it has no user. */
  username?: string;
  text: string;
  edited: boolean;
  deleted: boolean;
  replyCount: number;
  reactions: { name: string; users: string[] }[];
  files: { name: string }[];
  subtype?: string;
}

/** A conversation's (or a thread's) loaded messages, oldest first. */
export interface SlackPage {
  messages: SlackMessage[];
  /** Names by user id. */
  users: Record<string, string>;
  /** Older messages may exist (Load older). */
  more: boolean;
  loading: boolean;
}

/** An agent's own Slack app (yardd's slack.rs): its step and connection, never a token. */
export interface SlackAgentApp {
  /** `install`: approve it in Slack; `appToken`: its connection token is missing; `ready`. */
  step: "install" | "appToken" | "ready";
  appId: string;
  teamId: string;
  teamName: string;
  /** Its bot's handle in Slack, made from the agent's name (`code-reviewer`). */
  handle: string;
  /** Its bot's Slack user id (`U…`), which a call from the reply box mentions; empty until installed. Older yardds send none. */
  botUser?: string;
  ownerName: string;
  /** Slack can send the browser back to Switchyard after the install (an https address). */
  canRedirect: boolean;
  authorizeUrl?: string;
  /** Where Slack sends its events over HTTP; empty: Socket Mode, with an app-level token. */
  eventsUrl?: string;
  /** Why its name, description or icon did not reach Slack (a lapsed configuration token). */
  syncError?: string | null;
  /** Which picture of the agent Slack has as its icon (`iconKey` in slack.ts); empty: none yet. */
  iconKey?: string;
  /** The only people its Slack answers may @mention, chosen by the owner. */
  mentions?: { id: string; name: string }[];
  /** Channels whose new top-level messages and alerts reach the agent as data, chosen by the owner. */
  watched?: { id: string; name: string }[];
  /** When ready: `connecting`, `connected` or `error` (Socket Mode); `events` until Slack's first event over HTTP. */
  state?: "connecting" | "connected" | "error" | "events";
  error?: string | null;
}

export type ModelServiceTier = { id: string; name: string; description: string };

export interface ModelInfo {
  id: string;
  name: string;
  efforts: string[];
  defaultEffort?: string;
  isDefault?: boolean;
  serviceTiers?: ModelServiceTier[];
}

/** Per engine; an engine that is not installed on the host may be missing. */
export type Models = Partial<Record<Engine, ModelInfo[]>>;

export interface Host {
  id: string;
  name: string;
  cpu: number;
  cores: number;
  load: number;
  memUsed: number;
  memTotal: number;
  diskUsed: number;
  diskTotal: number;
  cpuHistory: number[];
  /** Why new sessions are being queued; null when the host has room. */
  pressure?: string | null;
  /** Sessions with live processes, by memory, largest first. */
  sessions?: SessionUsage[];
  /** Sessions waiting to start, oldest first. */
  queued?: { id: string; reason: string }[];
  limits?: { minFreeBytes: number; forced: boolean };
  swapUsed?: number;
  swapTotal?: number;
  /** The overload episode going on now (yardd's pressure.rs); null when calm. */
  episode?: PressureEpisode | null;
  /** The latest closed episodes, newest first; `host.pressure` pages older ones. */
  episodes?: PressureEpisode[];
  /** Running turns with no event for 5 minutes while the host is overloaded. */
  stalled?: { id: string; quietSecs: number }[];
}

export type PressureReason = "load" | "memory" | "swap" | "forced";

/** A session among the top users at an episode's worst sample. */
export interface PressureSession {
  id: string;
  title: string;
  engine: Engine;
  /** The automation's name, for one of its runs. */
  automation?: string;
  cpuCores: number;
  memBytes: number;
}

/** A process that belongs to no session, among the top users. */
export interface PressureProc {
  pid: number;
  name: string;
  cmd: string;
  cpuCores: number;
  memBytes: number;
}

/** One stretch of time the host was overloaded. Times are the host's clock. */
export interface PressureEpisode {
  id: string;
  start: number;
  /** null while it lasts. */
  end: number | null;
  lastSeen: number;
  reasons: PressureReason[];
  /** Each one's highest value during the episode. */
  peak: { load: number; cores: number; memUsed: number; memTotal: number; swapUsed: number; swapTotal: number };
  /** Who used the most at its worst sample. */
  top: { at: number; load: number; memUsed: number; swapUsed: number; sessions: PressureSession[]; procs: PressureProc[] };
}

/** A remote yardd this one reaches, as listed by the `hosts` message. */
export interface HostEntry {
  id: string;
  name: string;
  kind: "ssh" | "direct";
  destination?: string;
  remotePort?: number;
  url?: string;
  status: "connecting" | "connected" | "error";
  error?: string;
  /** SSH works but no yardd answered behind the tunnel: offer to install it. */
  noDaemon?: boolean;
  /** Whether agents live on that host (yardd's agents.rs); false: its
   *  Switchyard is older and needs Update Switchyard. Missing until it
   *  connected once. */
  ownAgents?: boolean;
}

export interface SessionUsage {
  id: string;
  cpuCores: number;
  memBytes: number;
}

/** One plan window as an engine reports it. */
export interface RateWindow {
  usedPercent: number;
  windowDurationMins?: number | null;
  /** Epoch seconds. */
  resetsAt?: number | null;
}

/** What each engine says about the plan limits; see ui/src/usage.ts. */
/** A limit window in Claude's shape (Grok's allowance uses it too). */
export interface LimitWindow {
  /** Fraction used, 0 to 1. */
  utilization: number | null;
  /** Epoch seconds. */
  resetsAt: number | null;
  /** The meter's name, for a window the UI has no name of its own for. */
  label?: string;
}

/** One Codex limit: primary is the short window (5 hours), secondary the week. */
export interface CodexLimit {
  limitId?: string | null;
  limitName?: string | null;
  normalModelSlug?: string | null;
  primary?: RateWindow | null;
  secondary?: RateWindow | null;
}

/** A free reset Codex grants (`title` "Full reset"); epoch seconds. */
export interface CodexResetCredit {
  id?: string;
  status?: string;
  title?: string | null;
  expiresAt?: number | null;
}

export interface Usage {
  claude?: {
    status?: string;
    /** By the SDK's window name (five_hour, seven_day, seven_day_sonnet, ...). */
    windows?: Record<string, LimitWindow>;
    /** Fractions, from a yardd older than `windows`. */
    fiveHour?: number | null;
    sevenDay?: number | null;
  };
  /** Codex's main limit, every limit by id (the main one included) and its free resets. */
  codex?: CodexLimit & { byLimitId?: Record<string, CodexLimit> | null; resetCredits?: { availableCount?: number; credits?: CodexResetCredit[] | null } | null };
  /** Grok's credit allowance for its billing period, one window ("week", "month" or "period"). */
  grok?: { windows?: Record<string, LimitWindow> };
  /** Added accounts' limits by account id, in the same shape as their engine's above. */
  accounts?: Record<string, any>;
}

/** An added account on a host. Default is every engine's own and is not listed. */
export interface AccountEntry {
  id: string;
  engine: Engine;
  name: string;
}

export interface ChangedFile {
  path: string;
  /** A rename's old path; `path` is the new one. */
  from?: string;
  diff?: string;
  kind?: string;
  added?: number | null;
  removed?: number | null;
  untracked?: boolean;
}

export interface Changes {
  files?: ChangedFile[];
  diff?: string;
  truncated?: boolean;
  branch?: string;
  error?: string;
}

export interface PrCheck {
  name: string;
  workflow?: string | null;
  result: "passed" | "failed" | "running" | "skipped" | "neutral";
  url?: string | null;
}

export interface PrComment {
  author: string;
  /** From GitHub, untrusted: render as text. */
  body: string;
  url?: string | null;
}

export interface PrThread {
  path: string;
  line: number | null;
  resolved: boolean;
  outdated: boolean;
  comments: PrComment[];
}

export interface PullRequest {
  number: number;
  title: string;
  url: string;
  state: "OPEN" | "CLOSED" | "MERGED";
  isDraft: boolean;
  base: string;
  head: string;
  reviewDecision?: string | null;
  mergeable?: string | null;
  mergeState?: string | null;
  checks: PrCheck[];
  checksTruncated?: boolean;
  threads: PrThread[];
  threadsError?: string;
}

/** Reply to `git.branches { cwd, host? }`: where a new worktree can be cut from. */
export interface BranchList {
  op: "git.branches";
  cwd: string;
  /** Set when the reply came from a remote host. */
  hostId?: string;
  /** The checked-out branch, or a short commit id when detached. */
  current?: string;
  /** Local and `origin/*` branches, most recently committed first, at most 200. */
  branches?: string[];
  /** The folder does not exist (deleted since it was picked). */
  missing?: boolean;
  error?: string;
}

/** Progress of `host.install { hostId }`: one message per output line, then one with `done`.
 *  Lines starting with `@@` are for the app (`@@engine <name> ok|missing|logged-out`, `@@ok`, `@@fail`). */
export interface HostInstallMsg {
  op: "host.install";
  hostId: string;
  line?: string;
  done?: boolean;
  ok?: boolean;
}

/** One engine on one host, from `engines.status`. `loggedIn` is null when it could not be read. */
export interface EngineState {
  installed: boolean;
  loggedIn: boolean | null;
  account?: string | null;
  /** OpenCode: the providers it can use (it works with its free ones alone). */
  providers?: string[];
  error?: string;
}
export type EngineStates = Partial<Record<Engine, EngineState>> & {
  /** Added accounts' states by account id. */
  accounts?: Record<string, EngineState>;
};

/** Progress of `login.start`. Codex and Grok show a link and a code to enter
 *  there; Claude shows a link whose page gives back a code to paste into the
 *  app. OpenCode first offers its `providers`; picking one (`login.choose
 *  { provider, method }`) asks for an API `key`, or goes on as `url` (paste
 *  the code back) or `code` (a device code; it finishes by itself). */
export interface LoginState {
  /** CLI-managed login that needs an interactive terminal on the host. */
  command?: string;
  state: "starting" | "terminal" | "providers" | "key" | "code" | "url" | "checking" | "done" | "failed" | "cancelled";
  url?: string;
  code?: string | null;
  error?: string;
  providers?: LoginProvider[];
  /** OpenCode: the provider being logged in. */
  provider?: string;
  /** Guidance from the CLI or provider for completing the login. */
  instructions?: string;
  label?: string;
}

/** One of OpenCode's providers and the ways into it that work from the app. */
export interface LoginProvider {
  id: string;
  name: string;
  connected: boolean;
  methods: { index: number; type: "api" | "oauth"; label: string }[];
}

/** Reply to `fs.dirs { path, host? }`: the subfolders the folder picker offers. */
export interface DirList {
  op: "fs.dirs";
  path: string;
  /** Set when the reply came from a remote host. */
  hostId?: string;
  /** Sorted by name, hidden ones included, at most 2000. `git` marks a repository root. */
  dirs?: { name: string; git: boolean }[];
  /** The first folder on `path` that does not exist, when that is why there is no list. */
  missing?: string;
  /** The first entry on `path` that is a file, not a folder, when that is why there is no list. */
  file?: string;
  error?: string;
}

/** Reply to `session.delete { removeWorktree: true }` once the session is gone. */
export interface WorktreeRemoved {
  op: "worktree.removed";
  sessionId: string;
  /** False when `git branch -d` kept a branch with unmerged commits. */
  branchDeleted: boolean;
  message: string;
}

/** One of a session's terminals, from `term.list` (oldest first). Titles are
 *  "1", "2", ... per session and not reused. A terminal whose shell exited
 *  stays listed until `term.close`. */
export interface TermInfo {
  termId: string;
  title: string;
  exited?: boolean;
}

/** Reply to `term.open`, `term.new` and `term.attach`: the scrollback from `offset`. */
export interface TermOpened {
  op: "term.opened";
  sessionId: string;
  termId: string;
  title: string;
  exited?: boolean;
  offset: number;
  /** Base64. */
  data: string;
}

/** A file attached to a prompt, saved by the yardd that runs the session
 *  (a remote host's own data dir for a remote session). */
export interface Attachment {
  id: string;
  /** Sanitized: one path component, no leading dot or dash. */
  name: string;
  /** Inline image types are checked against the bytes. */
  mime: string;
  size: number;
  /** Absolute, on the session's host. */
  path: string;
}

/** Largest file `attach.put` takes. Keep in step with `attachments::MAX_BYTES`. */
export const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024;
/** Most files one message takes. Keep in step with `attachments::MAX_PER_MESSAGE`. */
export const MAX_ATTACHMENTS_PER_MESSAGE = 20;

/** Reply to `session.interrupt` (what was queued, joined) or `session.unqueue`
 *  (one message, with its `id`) when it had not reached the agent yet: the
 *  text goes back to that window's reply box. Sent only to the asking window. */
export interface QueueReturned {
  op: "queue.returned";
  sessionId: string;
  id?: string;
  text: string;
  attachments?: Attachment[];
}

/** Reply to `attach.put { ref, name, mime, data (base64), host? | sessionId? }`.
 *  The file goes to `host`, or to the host of `sessionId`, or stays here. */
export interface AttachStored {
  op: "attach.stored";
  ref: string;
  /** Set when the file went to a remote host. */
  hostId?: string;
  attachment?: Attachment;
  error?: string;
}

/** Reply to `attach.get { ref, sessionId, id }`, for a thumbnail. Only an
 *  attachment that one of the session's prompts carries can be read. */
export interface AttachData {
  op: "attach.data";
  ref?: string;
  sessionId: string;
  id: string;
  mime?: string;
  /** Base64. */
  data?: string;
  error?: string;
}

/** One process of a session, from `session.procs`: the host sampler's view,
 *  up to 3 s old. `cpu` is percent of one core. */
export interface ProcRow {
  pid: number;
  ppid: number;
  name: string;
  /** The command line, joined, at most 400 characters. */
  cmd: string;
  cpu: number;
  memBytes: number;
  ageSecs: number;
  /** `agent`: what the engine runs. `terminal`: the session's shells. */
  origin: "agent" | "terminal";
  /** Start time, seconds since the epoch. `session.kill` sends it back so a
   *  reused pid is refused. Missing from an older yardd. */
  start?: number;
  /** The engine's own process (a Claude sidecar or its CLI): `session.kill`
   *  refuses it, so the tab offers no Stop. Missing from an older yardd. */
  engine?: boolean;
}

/** Reply to `session.kill { sessionId, pid, start?, signal, tree? }`, which
 *  signals one of the session's processes (with `tree`, it and everything
 *  below it, children first). yardd checks the pid against the session's
 *  tree read at that moment. `running`: still alive a few seconds later (the
 *  next step is `signal: "kill"`). `refused` comes with a `reason`. */
export interface KillReply {
  op: "session.killed";
  sessionId: string;
  pid: number;
  signal: "term" | "kill";
  tree: boolean;
  result: "killed" | "running" | "refused";
  reason?: string;
  /** Pids still alive when yardd answered. */
  running: number[];
}

/** Reply to `pr.get`: the branch's PR, no PR, or why it could not be read. */
export type PrResult =
  | Pick<PullRequest, "number" | "title" | "url">
  | { none: true; branch: string; defaultBranch?: string | null }
  /** The folder is not a git repository, so it has no PR. */
  | { repo: false }
  /**
   * `repo`: the folder is a git repository, so a review is still possible,
   * against `defaultBranch` when yardd found one.
   */
  | { error: string; repo?: boolean; defaultBranch?: string | null };

/** A PR link's hover card, from `pr.peek { sessionId, url, ref }`, run with
 *  `gh` on the session's host. Title and names come from GitHub: text only. */
export interface PrPeek {
  number: number;
  title: string;
  /** Canonical: `https://github.com/<owner>/<repo>/pull/<number>`. */
  url: string;
  /** `owner/repo`. */
  repo: string;
  state: "OPEN" | "CLOSED" | "MERGED";
  isDraft: boolean;
  author: string;
  base: string | null;
  head: string | null;
  reviewDecision?: string | null;
  mergeState?: string | null;
  additions?: number | null;
  deletions?: number | null;
  updatedAt?: string | null;
  /** Counts; `failing` names at most 5 of the failed checks. */
  checks: { passed: number; failed: number; running: number; failing: string[] };
}

/** Reply to `pr.peek`, sent only to the asking client. */
export interface PrPeekReply {
  op: "pr.peek";
  sessionId: string;
  ref?: string;
  url: string;
  pr?: PrPeek;
  error?: string;
}

/** One question the agent asks the user (an `approval_request` of kind
 *  `question`): Claude's AskUserQuestion, Codex's request_user_input.
 *  Answers are keyed by `question`. */
export interface Question {
  question: string;
  /** A short chip, "Database". */
  header: string;
  multiSelect: boolean;
  /** None: the answer is typed. */
  options: { label: string; description: string }[];
  /** A password or token: typed hidden, and yardd logs it masked. */
  secret?: boolean;
  /** false: only the options, no typed answer of one's own. */
  other?: boolean;
}

/** Normalized session events. Both engines produce exactly these. */
export type AgentEvent =
  /** `session.create` and `session.send` take `attachments: string[]` (ids
   *  from `attach.put`) and may then have empty text; the event carries
   *  the attachments' records. */
  | { type: "user_message"; text: string; attachments?: Attachment[]; steered?: boolean; at?: number; origin?: MessageOrigin }
  /** `at` is yardd's clock in ms; older logs lack it. */
  | { type: "turn_started"; at?: number }
  | { type: "turn_completed"; status: string; at?: number }
  | { type: "assistant_delta"; itemId: string; text: string }
  | { type: "assistant_message"; itemId: string; text: string }
  /** Thinking: Claude's summarized thinking, Codex's reasoning summary. */
  | { type: "reasoning_delta"; itemId: string; text: string }
  | { type: "reasoning"; itemId: string; text: string }
  /** `agent` identifies a subagent. `parent` is the
   *  itemId of the Agent call a subagent's own step runs under. */
  | { type: "tool_started"; itemId: string; kind: string; title: string; detail: string; agent?: string; parent?: string }
  /** `ms`: how long a subagent ran. An Agent call ends with its task, which
   *  can be after the turn that started it. `app`: the MCP tool's widget
   *  (a chart), fetched with `widget.get`. */
  | { type: "tool_completed"; itemId: string; ok: boolean; output?: string | null; note?: string | null; ms?: number | null; app?: WidgetRef }
  | { type: "file_change"; itemId: string; files: ChangedFile[] }
  /** Kind `question`: the agent asks the user to choose (`questions`), answered with `session.approve`'s `answers`. */
  | { type: "approval_request"; requestId: string; kind: string; title: string; detail: string; /** The call it asks about. */ itemId?: string | null; at?: number; questions?: Question[] }
  | { type: "approval_resolved"; requestId: string; decision: Decision; at?: number; answers?: Record<string, string> }
  | { type: "error"; message: string }
  /** A quiet line from yardd itself, such as a hand-off or a worktree setup. Changes no status. */
  | { type: "note"; text: string; /** A hand-off note's other session. */ session?: string }
  /** The session is waiting for the host to have room. Carries its prompt. */
  | { type: "queued"; reason: string; text?: string; attachments?: Attachment[] };

/** A tool call. An Agent call has `agent` (the subagent's type) and, in
 *  `layout`, `steps`: the calls that ran under it, nested as deep as the
 *  subagents are. */
export interface ToolItem {
  kind: "tool";
  key: string;
  itemId: string;
  toolKind: string;
  title: string;
  detail: string;
  state: "running" | "ok" | "failed";
  /** Set while an approval for this call is open, and after it was denied or
   *  never answered: then the call did not run, whatever `state` says. */
  asked?: "waiting" | "denied" | "unanswered";
  output?: string | null;
  agent?: string;
  /** The itemId of the Agent call this step runs under. */
  parent?: string;
  /** How long the subagent ran. */
  ms?: number;
  steps?: ToolItem[];
  /** An MCP Apps widget the call returned (a chart, a table). */
  app?: WidgetRef;
}

/** A widget yardd stored for a tool call; `widget.get { sessionId, id }` has the rest. */
export interface WidgetRef {
  id: string;
  server: string;
  tool: string;
}

/** One MCP server (or ChatGPT app) of an engine account, from `mcp.status`. */
export interface McpServer {
  name: string;
  status: "connected" | "needs-auth" | "failed" | "pending" | "disabled" | string;
  /** claudeai, plugin, user, project, local, dynamic, managed, config, chatgpt, ... */
  source: string;
  tools: number;
  /** Tools that come with a widget the thread can show. */
  appTools: number;
  error?: string;
  /** Scheme and host of an HTTP server. */
  origin?: string;
  /** A ChatGPT app (Codex): connected on chatgpt.com, at `installUrl`. */
  app?: boolean;
  installUrl?: string;
}

/** `mcp` broadcast: the servers of an engine account on a host. */
export interface McpStatus {
  engine: Engine;
  account: string;
  servers?: McpServer[];
  error?: string;
  at?: number;
}

/** `mcp.auth` broadcast: a sign-in of one server. `open` gives a page to
 *  visit; `paste` asks for the address the browser ends on, for a host the
 *  browser cannot reach. */
export interface McpAuthState {
  state: "starting" | "open" | "sending" | "done" | "failed";
  url?: string;
  paste?: boolean;
  error?: string;
}

/** What the thread renders. Built from events by `applyEvent`. */
export type Item =
  /** `steered`: sent while the agent worked and given to it mid-turn, at `at`. */
  | { kind: "user"; key: string; text: string; attachments?: Attachment[]; steered?: boolean; at?: number; origin?: MessageOrigin }
  | { kind: "assistant"; key: string; itemId: string; text: string; streaming: boolean }
  | { kind: "reasoning"; key: string; itemId: string; text: string; streaming: boolean }
  /** A turn's span, for "Worked for 1m 12s". Not drawn by itself. */
  | { kind: "turn"; key: string; start?: number; end?: number; status: string }
  | ToolItem
  | { kind: "diff"; key: string; files: ChangedFile[] }
  | {
      kind: "approval";
      key: string;
      requestId: string;
      approvalKind: string;
      title: string;
      detail: string;
      decision: Decision | null;
      /** The agent's questions, when it asks the user to choose instead of to allow. */
      questions?: Question[];
      /** What the user answered them. */
      answers?: Record<string, string>;
      /** The tool call it asks about, when the engine says. */
      itemId?: string;
      /** The turn ended (Stop, a failure, a yardd restart) before anyone answered. */
      unanswered?: boolean;
      /** A decision was sent and yardd has not resolved it yet. */
      sending?: boolean;
      /** When it was asked and answered (yardd's clock), so "Worked for"
       *  leaves out the wait. Older logs lack them. */
      askedAt?: number;
      answeredAt?: number;
    }
  | { kind: "error"; key: string; message: string }
  | { kind: "note"; key: string; text: string; session?: string }
  | { kind: "queued"; key: string; reason: string };

/** Where an item with this engine id sits in the current turn, or -1. Ids
 *  are matched only after the last user message: an engine may reuse them
 *  across turns (older Claude sidecars restarted their counter), and a match
 *  in an earlier turn would put new text above the question it answers.
 *  A steered message goes in mid-turn, so it is not a boundary: a command
 *  that started before it still completes after it. */
function inTurn(items: Item[], match: (it: Item) => boolean): number {
  for (let i = items.length - 1; i >= 0; i--) {
    if (startsTurn(items[i])) return -1;
    if (match(items[i])) return i;
  }
  return -1;
}

function startsTurn(it: Item): boolean {
  return it.kind === "user" && !it.steered;
}

/** What an approval makes of the call it asks about. */
function askedOf(a: Extract<Item, { kind: "approval" }>): ToolItem["asked"] {
  if (a.decision === null) return a.unanswered ? "unanswered" : "waiting";
  return a.decision === "deny" ? "denied" : undefined;
}

/** Marks the call an approval names (from `from` on) with what the approval made of it. */
function markAsked(items: Item[], from: number, approval: Extract<Item, { kind: "approval" }>): Item[] {
  if (!approval.itemId) return items;
  const asked = askedOf(approval);
  return items.map((it, i) => {
    if (i < from || it.kind !== "tool" || it.itemId !== approval.itemId) return it;
    const { asked: _, ...rest } = it;
    return asked ? { ...rest, asked } : rest;
  });
}

function turnStart(items: Item[]): number {
  for (let i = items.length - 1; i >= 0; i--) if (startsTurn(items[i])) return i + 1;
  return 0;
}

export function applyEvent(items: Item[], ev: AgentEvent, seq: number): Item[] {
  const key = `e${seq}`;
  switch (ev.type) {
    case "user_message":
      return [
        ...items,
        {
          kind: "user",
          key,
          text: ev.text,
          ...(ev.attachments?.length ? { attachments: ev.attachments } : {}),
          ...(ev.steered ? { steered: true, at: ev.at } : {}),
          ...(ev.origin && ev.origin.kind !== "user" ? { origin: ev.origin } : {}),
        },
      ];
    case "reasoning_delta":
    case "reasoning": {
      const i = inTurn(items, (it) => it.kind === "reasoning" && it.itemId === ev.itemId);
      const streaming = ev.type === "reasoning_delta";
      if (i < 0) return ev.text ? [...items, { kind: "reasoning", key, itemId: ev.itemId, text: ev.text, streaming }] : items;
      const next = items.slice();
      const cur = next[i] as Extract<Item, { kind: "reasoning" }>;
      next[i] = { ...cur, text: streaming ? cur.text + ev.text : ev.text || cur.text, streaming };
      return next;
    }
    case "assistant_delta": {
      const i = inTurn(items, (it) => it.kind === "assistant" && it.itemId === ev.itemId);
      if (i < 0) return [...items, { kind: "assistant", key, itemId: ev.itemId, text: ev.text, streaming: true }];
      const next = items.slice();
      const cur = next[i] as Extract<Item, { kind: "assistant" }>;
      next[i] = { ...cur, text: cur.text + ev.text };
      return next;
    }
    case "assistant_message": {
      const i = inTurn(items, (it) => it.kind === "assistant" && it.itemId === ev.itemId);
      if (i < 0) return [...items, { kind: "assistant", key, itemId: ev.itemId, text: ev.text, streaming: false }];
      const next = items.slice();
      next[i] = { ...(next[i] as Extract<Item, { kind: "assistant" }>), text: ev.text, streaming: false };
      return next;
    }
    case "tool_started": {
      const tool: ToolItem = { kind: "tool", key, itemId: ev.itemId, toolKind: ev.kind, title: ev.title, detail: ev.detail, state: "running" };
      if (ev.agent) tool.agent = ev.agent;
      if (ev.parent) tool.parent = ev.parent;
      // A provider can resume the same subagent in a later turn.
      if (ev.agent) {
        const i = items.findIndex((it) => it.kind === "tool" && it.agent && it.itemId === ev.itemId);
        if (i >= 0) {
          const old = items[i] as ToolItem;
          const next = items.slice();
          next[i] = { ...tool, key: old.key, detail: tool.detail || old.detail };
          return next;
        }
      }
      // Some engines ask before they announce the call.
      const from = turnStart(items);
      for (let i = items.length - 1; i >= from; i--) {
        const it = items[i];
        if (it.kind !== "approval" || it.itemId !== ev.itemId) continue;
        const asked = askedOf(it);
        if (asked) tool.asked = asked;
        break;
      }
      return [...items, tool];
    }
    case "tool_completed": {
      const from = turnStart(items);
      // An Agent call can end after a later prompt: its notification comes when its task does.
      return items.map((it, i) =>
        (i >= from || (it.kind === "tool" && it.agent)) && it.kind === "tool" && it.itemId === ev.itemId
          ? { ...it, state: ev.ok ? "ok" : "failed", output: ev.output ?? null, ...(ev.ms != null ? { ms: ev.ms } : {}), ...(ev.app ? { app: ev.app } : {}) }
          : it,
      );
    }
    case "file_change":
      return ev.files.length ? [...items, { kind: "diff", key, files: ev.files }] : items;
    case "approval_request": {
      const approval: Extract<Item, { kind: "approval" }> = {
        kind: "approval",
        key,
        requestId: ev.requestId,
        approvalKind: ev.kind,
        title: ev.title,
        detail: ev.detail,
        decision: null,
      };
      if (ev.itemId) approval.itemId = ev.itemId;
      if (ev.at != null) approval.askedAt = ev.at;
      if (ev.questions?.length) approval.questions = ev.questions;
      return markAsked([...items, approval], turnStart(items), approval);
    }
    case "approval_resolved": {
      const from = turnStart(items);
      let resolved: Extract<Item, { kind: "approval" }> | undefined;
      const next = items.map((it, i) => {
        if (i >= from && it.kind === "approval" && it.requestId === ev.requestId) {
          return (resolved = { ...it, decision: ev.decision, ...(ev.at != null ? { answeredAt: ev.at } : {}), ...(ev.answers ? { answers: ev.answers } : {}) });
        }
        return it;
      });
      return resolved ? markAsked(next, from, resolved) : next;
    }
    case "turn_started":
      return [...items, { kind: "turn", key, start: ev.at, status: "running" }];
    case "turn_completed": {
      let open = -1;
      for (let i = items.length - 1; i >= 0; i--) {
        if (items[i].kind === "turn") {
          open = i;
          break;
        }
      }
      // A background subagent goes on after a completed turn, with the steps it
      // is taking. Stop or a failure ends it.
      const goesOn = new Set(
        ev.status === "completed" ? items.flatMap((it) => (it.kind === "tool" && it.agent && it.state === "running" ? [it.itemId] : [])) : [],
      );
      // Nothing else may look live once the turn is over.
      return items.map((it, i) => {
        if (i === open && it.kind === "turn") return { ...it, end: ev.at, status: ev.status };
        if ((it.kind === "assistant" || it.kind === "reasoning") && it.streaming) return { ...it, streaming: false };
        if (it.kind === "tool" && it.state === "running" && !goesOn.has(it.itemId) && !(it.parent && goesOn.has(it.parent))) {
          return { ...it, state: "failed" as const, ...(it.asked === "waiting" ? { asked: "unanswered" as const } : {}) };
        }
        // ACP engines fail the asked call themselves on Stop, before the turn
        // ends: it still never ran, so it must not count as run and failed.
        if (it.kind === "tool" && it.asked === "waiting") return { ...it, asked: "unanswered" as const };
        // Nobody chose, so it is not shown as "Denied".
        if (it.kind === "approval" && it.decision === null) return { ...it, unanswered: true };
        return it;
      });
    }
    case "error": {
      // Codex reports a failed turn twice: as an error and as the turn's end.
      const last = items[items.length - 1];
      if (last?.kind === "error" && last.message === ev.message) return items;
      return [...items, { kind: "error", key, message: ev.message }];
    }
    case "note":
      return [...items, ev.session ? { kind: "note", key, text: ev.text, session: ev.session } : { kind: "note", key, text: ev.text }];
    case "queued":
      return [...items, { kind: "queued", key, reason: ev.reason }];
    default:
      return items;
  }
}

/** A Claude Code session on a host, from `claude.sessions` (the Agent SDK's listSessions). */
export interface ClaudeSessionRow {
  sessionId: string;
  title: string;
  cwd?: string | null;
  gitBranch?: string | null;
  lastModified: number;
  createdAt?: number | null;
  /** Our session already holding it, if it was imported. */
  importedAs?: string | null;
}

/** The `claude.sessions` reply. `hostId` is set when it came from a remote host. */
export interface ClaudeSessionList {
  op: "claude.sessions";
  hostId?: string;
  sessions: ClaudeSessionRow[];
  error?: string;
}


/**
 * When an automation runs, in its host's local time zone. `day` is 0 (Sunday) to 6.
 * `manual` never runs on its own: Run now, or a script's `automation.run`.
 */
export type Schedule =
  | { kind: "interval"; every: number; unit: "minutes" | "hours" }
  | { kind: "daily"; time: string }
  | { kind: "weekly"; day: number; time: string }
  | { kind: "manual" };

/** Active hours for an interval; days name the day the window starts (0 = Sunday). */
export interface AutomationWindow {
  start: string;
  end: string;
  days?: number[];
}

export interface LastRun {
  at: number;
  outcome: "started" | "skipped" | "failed";
  sessionId?: string | null;
  /** Why it was skipped or failed. */
  reason?: string | null;
}

/** A scheduled prompt. Each host runs its own; see yardd's automations.rs. */
export interface Automation {
  id: string;
  name: string;
  prompt: string;
  cwd: string;
  engine: Engine;
  model: string;
  effort: string;
  permission: Permission;
  /** The branch runs are cut from; empty is the repository's default branch. */
  base: string;
  schedule: Schedule;
  window?: AutomationWindow | null;
  skipWhileRunning: boolean;
  /** Skip a scheduled run while the engine's usage is above these percentages. */
  maxFiveHour: number | null;
  maxWeek: number | null;
  enabled: boolean;
  createdAt: number;
  /** Null while disabled, and always for a manual schedule. */
  nextRun: number | null;
  lastRun?: LastRun | null;
  /** The connectors (MCP servers) its runs get. Missing: every one but
   *  Metabase (a record saved before the list); new ones start empty. */
  mcp?: string[] | null;
}
/** Hub setup discovery; `mobile.connected` records a successful iOS UI connection. */
export type { MobileStatus } from "./mobile";

/** An avatar colour; styles.css has a token for each. */
export type AgentColor = "blue" | "green" | "amber" | "rose" | "violet" | "teal" | "orange" | "slate";
/** An avatar face; agents.ts draws each. */
export type AgentFace = "smile" | "grin" | "wink" | "calm" | "glasses" | "bot" | "happy" | "curious";

/** What an agent asked for that it may not give itself (yardd's agents.rs). */
export interface AgentRequest {
  cwd?: string;
  permission?: Permission;
  slack?: boolean;
}

/** A named agent, mentioned as `@handle`. An agent lives on the host it runs
 *  on (yardd's agents.rs); this yardd lists its own and each host's, whose
 *  ids are `<hostId>~<id>` like their sessions'. */
export interface Agent {
  id: string;
  /** As people see it: any printable text, spaces and all, up to 40 characters. */
  name: string;
  /** What it is `@mentioned` by, made from the name by yardd ("Custom Domains"
   *  is `Custom-Domains`); unique ignoring case. Older yardds send none: their
   *  names were handles (handleOf). */
  handle?: string;
  color: AgentColor;
  /** Empty: one picked from its id (faceOf). */
  face: AgentFace | "";
  /** One line: what it is for. */
  about: string;
  engine: Engine;
  /** Empty: the host's default model for the engine. */
  model: string;
  effort: string;
  account: string;
  /** Empty: its own folder in yardd's data dir. */
  cwd: string;
  permission: Permission;
  /** Who it is and how it works, given at the start of each chat. */
  instructions: string;
  /** What it keeps in mind across chats. */
  memory: string;
  /** Its one conversation (a session id), made with its first message; every message to the agent goes there. A host's agent's carries the host, like the agent's id. */
  chat?: string | null;
  /** Where its conversation runs: `local`, or a remote host's id (older yardds send none). */
  host?: string;
  /** Conversations it left on a host it moved away from, oldest first: readable, not answered from. */
  previous?: string[];
  /** Made with New agent and still setting itself up in its conversation. */
  setup?: boolean;
  /** What it asked for in its conversation and waits on the owner's click for. */
  request?: AgentRequest | null;
  /** The connectors (MCP servers) its sessions get; the owner's to set.
   *  Missing: every one but Metabase (a record saved before the list). */
  mcp?: string[] | null;
  /** A record this Mac keeps for a host that refused to take it for good
   *  (another Mac's copy is there): why, so its page can say so. */
  handOverError?: string;
  createdAt: number;
  updatedAt: number;
}
