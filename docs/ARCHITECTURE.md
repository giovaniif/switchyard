# Switchyard: architecture

Working name. Design: [Agent Harness UI](https://claude.ai/artifact/XQdXoraWtiuUwwrrCt3462), sources in [design/](../design).

One UI for Claude Code, Codex, OpenCode, Grok and Gemini sessions on my own subscriptions: fast switching between engines, models and efforts, a terminal and process list next to each session, and the same view for sessions on a remote box.

Rewritten 2026-11-13. Replaces the 2026-11-06 version. Checked against claude 2.1.318, codex-cli 0.159.2, Agent SDK 0.3.323. OpenCode and Grok added 2026-11-16, checked against opencode-ai 1.19.5 and @xai-official/grok 1.0.77.

## Decided

- Tauri shell. Rust core, web UI. Personal first.
- The app drives the installed `claude`, `codex`, `opencode`, `grok` and `gemini` CLIs and turns their event streams into one UI. Which engines exist, their names and what they can do live in one place each side: `crates/yardd/src/engines.rs` and `ui/src/engines.ts`. It never holds a login: credentials stay where each CLI keeps them. It can start a CLI's own login flow on a host (see Engine logins).
- Chat UI in the Claude Code message style, not a terminal. The terminal is a separate pane.
- White, minimal theme. One flat session list of one-line rows: the harness's own mark (Claude, Codex, Gemini, Grok, OpenCode; `ui/src/EngineIcon.tsx`, artwork from LobeHub's MIT icons) with what the session is doing as a badge on its corner (working spinner, approval, unread, failed, queued; none when quiet), then the title. Model, status, account and a remote session's host are in the row's tooltip and read out to screen readers. On hover Archive and the … menu appear at the row's end; touch screens keep … visible. The same marks replace the old "CC"/"CX" badges in the model picker, Connectors and Usage.
- New session is a full page with a centered prompt box, not a dialog.
- Remote sessions run on my own box over SSH and Tailscale, not on claude.ai/code. A claude.ai webview and a hook bridge were considered and dropped.
- Host CPU, memory and disk are always visible. The daemon queues new sessions when a host is under pressure.

## Shape

- **yardd** (`crates/yardd`): Rust daemon. Owns every agent process, the event log and host stats. Serves the UI and a WebSocket on `127.0.0.1:47717` (`YARD_BIND` changes it: one address or a comma-separated list; the container image uses `0.0.0.0`, a server behind a reverse proxy adds the proxy-facing address next to loopback). The port stays out of 4510-4559, which LocalStack publishes on every interface.
- **yard-claude** (`sidecar/yard-claude`): TypeScript sidecar on the Claude Agent SDK, run with Bun. One per Claude session, JSON lines over stdio. The SDK exists only for TypeScript and Python.
- **ACP agents**: `opencode acp`, `grok agent stdio` and `gemini --acp`, one per engine and account, driven from yardd's `acp.rs` (see Engines).
- **ui** (`ui`): React + Vite. Talks only to yardd.
- **app** (`app/src-tauri`): Tauri v2 window. Starts yardd if nothing is listening, then loads yardd's page. It holds no agent state and yardd outlives it, so sessions survive closing the app.
- **App updates** (Sparkle-like): the packaged app checks `releases/latest/download/latest.json` at launch and every 6 hours (Tauri updater plugin) and installs a newer release in place with no dialog and no focus change. Then the app menu's "Check for Updates…" reads "Restart to Update to X.Y.Z…" ("Update to X.Y.Z…" while it is found but not installed, e.g. the download failed), and the page shows a one-line notice "Switchyard X.Y.Z is ready · What's new · Restart · Later". The app passes the state to the page by `eval`: `window.__SWITCHYARD_UPDATE__ = { version, ready }` plus a `switchyard:update` DOM event, again after every page load; a browser on the same yardd has neither, so it never shows the notice. The notice answers with the Tauri event `switchyard:update-action` (`notes` or `restart`; the capability grants `core:event:allow-emit`), and the app only ever answers with its own dialog. The menu item and "What's new" open one native window: the version's notes (latest.json `notes`, the CHANGELOG section, as plain text) with Install and Restart / Later; Install downloads first if needed. A menu check gives exactly one answer: that window, or "X is the latest version" (it waits for a running background check rather than saying "already checking"). Restarting stops the local yardd (SIGTERM, found with `lsof` on the port) so the new app starts its own; sessions come back, a running turn stops. At launch, an older yardd than the bundled one gets the restart question instead of only the title warning. No "Skip This Version": Later dismisses the notice for that version and state. The package is the notarized, stapled `.app`, tarred and signed with the updater key by `.github/scripts/macos-sign.sh`; `tauri.conf.json` holds the public key. A copy run from the repo never updates.

The same yardd binary runs on my Mac and on the remote box. A remote session is a session on another yardd, reached through an SSH port forward over Tailscale. No open port, no auth layer of our own.

The WebSocket is on a localhost TCP port because a webview cannot open a unix socket. yardd rejects any browser origin other than its own page and the Tauri window, so a website cannot drive agents.

## iOS / TestFlight

`mobile/` is a native SwiftUI app (iOS 26, Liquid Glass) for iPhone and iPad.
It is a client of the same `/ws` protocol as the web page, over the privately
reachable HTTPS host, and never starts a daemon. `Core/Thread.swift` ports
`applyEvent` and `Voice/VoiceRules.swift` ports `ui/src/voice.ts`; change them
with their TypeScript twins. Terminals, changes, PRs and processes open the
host's web page in a sheet. `project.yml` generates the Xcode project;
`.github/workflows/ios-app.yml` builds, tests and screenshots the bundled demo
on a simulator, and its explicit TestFlight dispatch archives and uploads with
iOS distribution credentials. See [mobile/README.md](../mobile/README.md).

The Hosts page always offers phone setup: a QR code (the `uqr` package) for
the address a phone can open, which is a host's HTTPS `YARD_ALLOW_ORIGIN`
entries (`mobile.origins`), since the desktop app's page is on loopback.
New-session and session headers show a small phone icon until a phone has
connected to any host. The app sends `mobile.connected { device }` on every
connection; yardd keeps the phone in `mobile.live` until that socket closes,
saves the last five in `mobile.json` with when each was last seen, includes
`mobile` in snapshots and broadcasts `mobile.status`. Remotes relay theirs to
the hub (`host.snapshot.mobile`, `mobile.status` with `hostId`): a phone
connects to the host it can reach, usually not the desktop.
The setup card offers the public TestFlight beta; `YARD_TESTFLIGHT_URL` replaces it (`none` removes it).
The app sends the host's origin as `Origin`, so `YARD_ALLOW_ORIGIN` needs no
new entry. Backgrounding stops speech; foregrounding reconnects the socket.
Notifications are local, from the live socket: there is no APNs service.

## Events


Every engine is normalized to one event list per session. The UI replays it on open.

| Event | Meaning |
|---|---|
| `user_message` | My prompt, with any attachments' details; a message I did not type carries its `origin` (see Agents) |
| `turn_started`, `turn_completed` | Turn boundaries, with yardd's clock in `at` (ms) |
| `assistant_delta`, `assistant_message` | Streamed text, then the final text |
| `reasoning_delta`, `reasoning` | Thinking: Claude's summarized thinking (`--thinking-display summarized`), Codex's reasoning summary (parts become paragraphs) |
| `tool_started`, `tool_completed` | A command, read, search or other tool call. Claude's Agent call carries `agent` (the subagent's type); a subagent's own steps carry `parent` (the Agent call's id); its `tool_completed` carries `ms` |
| `file_change` | Edited files with diffs |
| `approval_request`, `approval_resolved` | A command or edit waiting on me, or (kind `question`, with `questions`/`answers`) Claude's or Codex's questions |
| `error` | Engine error text |
| `queued` | The session is waiting for the host to have room. Carries its prompt |
| `note` | A quiet line from yardd, such as a hand-off. Changes no status |

Session status (`idle`, `running`, `needs_approval`, `queued`, `error`) is derived from these.

**Stop** (`session.interrupt`) asks the engine to stop. If it cannot be asked, or the turn is still open 5 s later, yardd ends the turn itself (`turn_completed` `interrupted`, with a note): a Claude sidecar is killed, and its next message starts a new one that resumes the conversation; a Codex turn is closed on our side. A Claude sidecar that exits on its own is also dropped, so the next message starts a fresh one instead of writing into a dead pipe.

- Item ids come from the engine (Codex item ids, Claude `tool_use` ids; the sidecar's own ids carry a per-process prefix). The UI matches an id only within the current turn, after the last user message, so a reused id can never pull new text up into an earlier turn. Order is the log's order.
- **Subagents** (Claude's Agent tool, formerly Task). The SDK streams a subagent's messages with `parent_tool_use_id` set, plus system `task_started` and `task_notification`. `sidecar/yard-claude/src/subagents.ts` keeps the subagent's tool calls (`tool_started` with `parent`, `tool_completed`) and drops its thinking, text and diffs; the Agent call ends with its `task_notification` (`tool_completed` with the subagent's last words as `output`, its duration as `ms`), and its own `tool_result` is ignored. Checked on a real Haiku turn: an Agent call runs in the background by default, so its `tool_result` is only "launched" and the report arrives in the notification, even after the turn that started it has ended; a foreground one gets its notification before the `tool_result`. So a completed turn leaves a still-running Agent call (and its steps) running, Stop or a failure ends it, and `tool_completed` finds an Agent call in an earlier turn. `layout` nests each step under its call (`steps`, as deep as subagents nest); a still-running Agent call stays out of the turn's fold. The UI shows it as one collapsed line: its description, type, "running" or "done · 2s", its step count; open, the steps and the report. Codex maps collab and subagent activity reports to the same tool events, keyed by child thread. The Workspace Subagents tab groups activity across the whole session, so it can show steps reported after the original turn. An Agent call that names no `subagent_type` runs Claude Code's general-purpose agent and is tagged `general-purpose` (untagged, a background one was folded away as a finished step). While Agent calls are still at work after their turn ended, `Meta.background` lists them (added on `tool_started` with `agent`, removed on its `tool_completed`, cleared by Stop, a failed turn or a yardd restart), and the session's sidebar row keeps its working badge until they report.
- Codex errors with `willRetry` ("Reconnecting... 2/5") are dropped; a turn that fails still ends with its error.
- The thread is laid out like T3 Code's work log (`ui/src/thread.ts`): tool calls and thinking between messages fold into a one-line group ("Read 3 files, ran 2 commands"); while a turn runs, only its latest step shows, under "Working for 12s"; a finished turn folds all of its work into "Worked for 1m 12s" above its answer. A finished turn that wrote messages between its steps keeps them in view in place, each run of steps between them folded to one line, under a "Worked for 1m 12s · …" line that folds nothing (owner: the progress messages were folded away and only the last summary was left to read). A thought shows as one line, its bold heading or first sentence, and opens to the full text.

**Chat diagrams.** Markdown fences labelled `mermaid` automatically show a
diagram once the fence closes, with a Show source / Show diagram toggle.
`ui/src/markdown.ts` recognises backtick and tilde fences, including open
fences during streaming; `MermaidDiagram.tsx` lazy-loads the bundled Mermaid
renderer. Invalid, empty and incomplete diagrams keep their source visible.
Rendering uses strict security, disabled HTML labels and protected security
and CSS settings. The result is a blob-backed SVG image, never inline HTML or
SVG in the chat DOM; object URLs are revoked on changes and unmount. Diagrams
fit the thread width on mobile. No service or engine-specific event is needed.

## Persistence

yardd saves every session in SQLite: `yard.db` in `$YARD_DATA_DIR`, or `~/.yard` when that is unset.

- One writer thread takes writes from a channel and saves them in batches, so emitting an event never waits on the disk.
- Streamed `assistant_delta`s are not saved. The final `assistant_message` has the full text.
- On startup yardd loads every session. One that was mid-turn gets an `error` ("yardd restarted during this turn.") and a failed `turn_completed`.
- A restored session resumes on its next message. Codex: `thread/resume` with the saved thread id. Claude: a new sidecar started with the SDK `resume` option and the saved Claude Code session id.
- `session.delete` stops the session's engine and terminal and removes it from memory and the database. Git worktrees are kept (until merged, see Worktrees).

## Engines

**Codex.** One `codex app-server` per account (started on first use) over stdio, JSON-RPC, driven from Rust. See Accounts.

- `thread/start` per session, `turn/start` per message. `turn/start` takes `model` and `effort`, so switching is per turn.
- Approvals arrive as server requests (`item/commandExecution/requestApproval`, `item/fileChange/requestApproval`) and are answered with `accept`, `acceptForSession` or `decline`.
- **Questions.** Codex's `request_user_input` (experimental) arrives as `item/tool/requestUserInput` with `questions` (`id`, `header`, `question`, `isOther`, `isSecret`, `options` or null). Codex offers the tool only in Plan mode unless `features.default_mode_request_user_input` is on, so yardd starts `codex app-server -c features.default_mode_request_user_input=true` (without it Codex writes the question into its reply). yardd sends the same `approval_request` of kind `question` as Claude's, one choice per question (`other: false` when only the options may be picked, `secret: true` for a hidden field), keeps the question ids, and answers `{answers: {<id>: {answers: [<text>]}}}`; Skip, a restricted session or no session answers `{answers: {}}`, which Codex takes as unanswered. A secret answer is logged as `••••••`. Checked against Codex 0.171 on a real turn: there is no `item/started` for the call, so the card has no tool row.
- `model/list` returns the models and each one's efforts. Today: `gpt-6.1-sol` (default), `gpt-6-astra`, `gpt-6.1-terra`, `gpt-6-sol`, `gpt-6-luna`, `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-5.6-luna`; efforts `low` to `ultra`.
- `account/rateLimits/updated` and `account/rateLimits/read` give `primary` (the 5-hour window) and `secondary` (the week), each with `usedPercent`, `windowDurationMins` and `resetsAt` (epoch seconds). yardd relays them as `usage.codex` (Default) or `usage.accounts[<id>]` (an added account: see Accounts). The read also answers `rateLimitsByLimitId` (every limit by id, the main `codex` one included; another model's limit is its own entry) and `rateLimitResetCredits` (the free resets the account holds, each `title` "Full reset", `status` and `expiresAt`); yardd keeps them beside the windows as `byLimitId` and `resetCredits` (`crates/yardd/src/usage.rs`). The notification carries one limit only, so it is merged in: it keeps the credits, and one for another limit updates only that limit's entry. Codex 0.171 on a Pro plan reported only the main limit, its `primary` a 10080-minute week.
- The protocol is marked experimental. Pin the CLI version.

**Claude.** Agent SDK `query()` in streaming-input mode inside yard-claude.

- `canUseTool` carries approvals. `setModel()` switches model mid-session. `interrupt()` stops a turn.
- **Questions** (`src/questions.ts`). Claude's `AskUserQuestion` comes through `canUseTool` too, in every permission mode, Full access included, and needs answers, not a yes: allowed as it is, Claude hears "The user did not answer the questions." (It used to show as "Allow this?" with its raw JSON, and Allow did exactly that.) The sidecar sends an `approval_request` of kind `question` with `questions` (`question`, `header`, `multiSelect`, `options` of `label` and `description`; at most 4 questions and 8 options, malformed ones left out), `title` the first question (for notifications, the hand-over summary and Slack's approval post) and `itemId` the call. The thread shows a question card (`QuestionCard` in `App.tsx`, `ui/src/questions.ts`): options as radios or checkboxes, a "Something else" field per question (for a one-choice question it replaces the picked option), Send once every question has an answer, and Skip. `session.approve` then carries `answers` (question text to answer: the label, labels joined with ", " in the options' order, or the typed text last; text only, at most four, else refused) to the sidecar's `permission` line, which hands Claude Code the input with `answers` (only for questions it asked); Skip is a deny ("The user chose not to answer these questions."). `approval_resolved` keeps the answers, so the card shows them after a reload. The step reads "Asked <first question>". Codex's questions use the same card (below); ACP engines' requests never carry questions.
- Effort is `low`, `medium`, `high`, `xhigh`, `max`. It is set at session start; changing it mid-session goes through `applyFlagSettings` (not wired yet).
- `rate_limit_event` describes ONE window per event (`rateLimitType`: `five_hour`, `seven_day`, `seven_day_opus`, `seven_day_sonnet`, ...) with `utilization` (a fraction) and `resetsAt` (epoch seconds); some CLI builds send them all as `unifiedWindows`. The sidecar (`src/usage.ts`, `mergeUsage`) folds events into `usage.claude = { status, windows }`, so a five-hour event does not wipe out the weekly one. yardd saves the last usage in `usage.json` in the data dir, so the Claude meters survive a restart; each window carries `resetsAt`, so one that reset meanwhile reads as unknown. (The old mapping read only `unifiedWindows`, which the SDK's types do not have.)
- **Limits without a turn.** Claude reports `rate_limit_event`s only during a turn, so an account that had not run one showed no limits. yardd now reads every Claude account's limits (Default and each added one, with its `CLAUDE_CONFIG_DIR`) at start, every 10 minutes, when an account is added and after it logs in, with `yard-claude usage`: a Claude Code with no prompt, asked for its usage report (the SDK's `usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET`, what `/usage` shows; no model call). `fromUsageReport` (`sidecar/yard-claude/src/usage.ts`) maps its `rate_limits.limits` to the same windows: `session` is `five_hour`, `weekly_all` `seven_day`, a model's `weekly_scoped` `seven_day_<model>`; any other row (a surface's week, a kind the server adds later) is kept under its kind with a `label` from the server's group and scope names, since the SDK says the rows are the server's to render as sent. The report is the whole picture and replaces what was kept; a turn's events name only the windows that turn touched, so yardd lays them over the kept ones (`merge_claude_turn`). They used to replace them, so after any Claude turn the per-model week the report had read (Fable's) was gone until the next report. An SDK without the call reports nothing and the turn's own events still count. `YARD_CLAUDE_USAGE_EVERY_MS` sets the interval (`0` turns it off; the daemon tests do, but one).

**Plan usage in the UI.** `ui/src/usage.ts` normalizes engine limits (5-hour, week, per-model weeks). A single Usage row above the sidebar host rows opens a popover above the row, or a bottom sheet on phones. It shows the current account's meters and reset times and marks windows at 80% or more. Other accounts expand on demand. Manage accounts opens Connectors. Page search opens `#/subscriptions`, showing account identities, source hosts and reset times. The popover reads shown sidebar hosts; the full page reads every host. `stripGroups` deduplicates by engine plus account name across machines, using the newest reset and then utilization. Installed Claude/Codex/Grok accounts without reports say so. Every window says when it resets: the sidebar row under its percent ("5-hour resets in 1 h 35 min", the window whose percent it shows; its tooltip lists every account's windows), the popover as "Resets 2:50 PM · in 1 h 35 min" for the shown account and, unfolded, for each other account's every window (it used to show only the first window's percent), Subscriptions likewise. Codex's full resets read "2 full resets available · First expires Dec 5 · in 9 d". OpenCode and Gemini report nothing; the popover names those installed on a shown host, Subscriptions all of them. Account selection is separate from model and effort selection. New-session model menus show the selected engine's models; existing sessions retain cross-engine hand-off confirmation.
- It runs on the CLI's own login (`apiKeySource: none`).
- Never parse `~/.claude/projects/*.jsonl`. Use the SDK's `listSessions` and `getSessionMessages`.

**ACP engines: OpenCode, Grok and Gemini.** All three speak ACP, the Agent Client Protocol (JSON-RPC lines on stdio), so one driver, `crates/yardd/src/acp.rs`, runs all three. One agent process per engine and account, started on first use like Codex's app-server, holds that account's sessions. `opencode serve` (HTTP + SSE) and a sidecar were the alternatives; ACP gives these engines one client and needs no port.

- `initialize` (we offer no file system and no terminal: the agent uses its own), `session/new { cwd }` per session, `session/prompt` per message. The prompt's answer (`stopReason`) ends the turn: `end_turn` completed, `cancelled` interrupted, `refusal` failed, `max_tokens` completed with a note.
- `session/update` becomes events: `agent_message_chunk` deltas and `agent_thought_chunk` reasoning deltas, closed into `assistant_message` and `reasoning` when a tool call starts or the turn ends; `tool_call`/`tool_call_update` become `tool_started`/`tool_completed` (kinds `execute`, `read`, `edit`, `search`, `fetch` map to command, read, edit, search, other); a `diff` content becomes one `file_change` (the Claude sidecar's patch: changed lines with two lines of context), before the approval that asks about it.
- Model, effort and mode are the session's `configOptions` (`session/set_config_option`; categories `model`, `thought_level`, `mode`), or `session/set_mode`. yardd sets them before a prompt when they differ from what it last set, so a switch takes effect from the next turn.
- Approvals: `session/request_permission` becomes an `approval_request`; the answer picks the offered option of that kind (`allow_once`, `allow_always`, `reject_once`). The session's mode answers some itself (see Permission modes). Stop sends `session/cancel` and answers open requests `cancelled`; a turn still open after 5 s is ended here, and its late answer is ignored.
- ACP engines deliver a message sent mid-turn with the next turn (`engines::can_steer`). Claude and Codex can deliver it at the next step of the current turn. `session.steer { sessionId, id }` asks the delivery worker to send only that queued message into the active turn. A failed delivery puts it back in the queue.
- Resume: the ACP session id is the engine ref. After a yardd restart, or when the agent exited (its running turn fails with "stopped before answering"), the next message starts a new agent and reopens the session with `session/resume`, else `session/load` (its replayed history is dropped).
- Usage: ACP's `usage_update` is context size and cost, not plan limits. OpenCode and Gemini report none, so no meters and no usage skips for them. Grok's agent answers the extension method `_x.ai/billing` (what its TUI's /usage "Usage limit" tab reads; ACP extension methods start with an underscore, plain `x.ai/billing` is "Method not found"): the credit allowance of its billing period (`creditUsagePercent`, `currentPeriod`, `billingCycle` WEEKLY or MONTHLY in grok 1.0.77's BillingConfig). yardd reads it with no turn from a Grok agent of its own (`grok::refresh_usage`: initialize, ask, kill; a running agent may hold a login from before the last sign-in) at start, every `YARD_CLAUDE_USAGE_EVERY_MS` (10 minutes), after a sign-in, when an account is added and on Refresh, and after each Grok turn on the agent at hand. It is kept as `usage.grok = { windows: { week|month|period: { utilization, resetsAt, label } } }`, Claude's shape. Not verified against a real answer: no signed-in Grok account was at hand (logged out it answers "Authentication required to fetch billing data"), so `usage::grok_from_billing` reads the nesting and time formats loosely and shows nothing without a `creditUsagePercent`. `grok usage --json` prints the same only on Grok's daemon route; on the default shell route it refuses. Automations' usage limits do not read Grok.
- Per-session processes: the agent is registered like a Codex app-server, so its command children are matched by folder.

**OpenCode** (npm `opencode-ai` for V1, `@opencode/cli` for V2). `opencode acp` asks before edits, shell commands and web fetches so yardd can apply the selected permission mode. V1 uses `OPENCODE_PERMISSION`. V2 appends the approval rules to `OPENCODE_CONFIG_CONTENT` for the agent process. The catalog uses Default's providers. V1's `opencode models --verbose` supplies names and variants as efforts. If the verbose command fails, yardd uses the `provider/model` IDs from `opencode models`. Modes are `build` and `plan`. OpenCode works with its own free models and no login.

**Grok** (xAI's Grok Build CLI, npm `@xai-official/grok`, command `grok`). Chosen over superagent-ai/grok-cli, which was renamed `grok-dev` and has had no release since 2026-05; xAI's own CLI ships near-daily, logs in with a Grok account and speaks ACP. `grok agent stdio`. Models and efforts come from `initialize` (`_meta.modelState`, `reasoningEfforts`), effort is the `reasoning_effort` option, and permission modes map to Grok's own modes. `session/new` answers "Authentication required" while logged out; the session shows it.

**Gemini** (Google's Gemini CLI, npm `@google/gemini-cli`). `gemini --acp --approval-mode default`, with `NO_BROWSER=true` so an expired login fails rather than opening a browser inside an agent. The user's CLI login and environment are reused; added accounts use `GEMINI_CLI_HOME` (the CLI creates `.gemini` inside it). Models come from `session/new.models.availableModels`, switched with `session/set_model`; no effort control is advertised. A short-lived, prompt-free ACP session in `<data dir>/gemini-probe` checks login and discovers models at startup and when engine status is refreshed. It has a 30-second timeout and its own process, so it cannot stop an active conversation. An installed but logged-out CLI offers Auto until models can be discovered. Session creation refreshes Default's models too.

- Gemini's modes are `default`, `auto_edit` and `yolo`; Plan uses `plan` when offered, else `default` with yardd refusing edit requests and asking for commands. Switching away from Full access always leaves `yolo`.
- Resume uses `session/load`, suppressing replayed history. Streaming, thinking, tools, diffs, approvals, attachments, Stop, hand-offs and queued next-turn messages use the shared ACP driver.
- Login stays in Gemini's interactive terminal: Hosts / Log in shows `gemini`, or the added account's `GEMINI_CLI_HOME=… gemini`, and **Check login** probes it again. The app does not drive Google's OAuth copy/paste flow through the ACP transport. No Gemini plan-usage meters or Switchy support.
- Integration reference: [Gemini ACP dispatcher](https://github.com/google-gemini/gemini-cli/blob/main/packages/cli/src/acp/acpRpcDispatcher.ts), [session manager](https://github.com/google-gemini/gemini-cli/blob/main/packages/cli/src/acp/acpSessionManager.ts), and [account isolation](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/enterprise.md). Scripted checks emulate the legacy model/mode fields and loadSession; no live Gemini inference is used for verification.

**Permission modes.** A picker next to the model, named like Claude Code's modes. Saved on the session (`permission`), set by `session.create` and changed any time by `session.setPermission`; a hand-off keeps it.

| Mode | Claude (SDK `permissionMode`) | Codex (`turn/start` overrides) | OpenCode, Grok (agent mode; what yardd answers itself) |
|---|---|---|---|
| Ask first (`ask`, default) | `default`: asks before commands and edits | `on-request`, workspace-write sandbox | `build` / `default`; every question goes to the user |
| Accept edits (`acceptEdits`) | `acceptEdits` | same as Ask first: Codex already edits inside its sandbox freely | `build` / `acceptEdits`; edits allowed unasked |
| Plan only (`plan`) | `plan` | `on-request`, read-only sandbox | `plan`; edits refused unasked, commands still asked |
| Full access (`full`) | `bypassPermissions` | `never`, no sandbox | `build` / `bypassPermissions`; everything allowed unasked |

Claude switches at once with `setPermissionMode` (the sidecar always passes `allowDangerouslySkipPermissions` so a later switch to full access works); Codex takes the mode from its next turn. Approving Claude's plan-mode exit leaves the picker showing Plan only until it is changed.

**Engine lifecycle: a finished session holds no connections.** Every loaded Codex thread keeps its own MCP clients, and every Claude sidecar is a Claude Code process with its own. Left loaded, one per session, they piled up: on 2026-11-26 the shared app-server held 46-61 HTTPS connections to one company's Metabase (an automation started a thread every 10 minutes and none was ever unloaded), and the account was disabled. `crates/yardd/src/release.rs`.

- **Release after idle.** A sweep releases a session's engine once it is quiet (status `idle`, no subagent still at work in `background`, no queued message; an approval keeps the status `needs_approval`) and has done nothing for 5 minutes (`YARD_RELEASE_IDLE_MS` overrides it; the tests use a fraction of a second). Activity is `Session::active_at`: any event, or an engine attaching. An automation's run is released as soon as it ends. Private threads are left alone.
- **Work that outlives the turn holds the engine.** Releasing kills what runs inside the engine's process, so two more checks:
  - Tools (`release::keeps_alive`): the sidecar flags `tool_started` with `keepsAlive` (`background` for Bash with `run_in_background`, `monitor`, `wakeup` for `ScheduleWakeup`, `cron` for `CronCreate`) and `endsCron` for `CronDelete`. A flagged call in the latest turn holds the engine until a later turn ends without one. A cron job holds it until a later `CronDelete`, whatever turns come between; the id is not matched, so one delete frees it.
  - Processes (Claude): `guard::agent_work` reads the session's tree from the last sample (taken after the session's last activity). Anything the sidecar runs itself, or a shell given a command under the CLI (`sh -c …`, how Claude Code runs Bash, and all below it), is the agent's and holds the sidecar. Any other child of the CLI is an MCP server and does not. A shell-script MCP server run as `bash -c …` would hold its session. Codex is not process-checked: its shared app-server's children can be told apart per session only while their command item runs, so a background terminal of an unloaded thread is not seen.
- **Codex:** `thread/unsubscribe { threadId }` (codex 0.171: `{ status: notLoaded | notSubscribed | unsubscribed }`). A thread with no subscriber stays loaded, MCP servers running, for the config's `thread_unload_delay_secs` (checked on 0.171: with the default, both unsubscribed threads stayed loaded), so every `thread/start` and `thread/resume` sets it to `0` in its config overrides: the thread unloads at once, `thread/closed` follows and its MCP server processes exit. yardd then reads `thread/loaded/list` and logs the count. The session's backend goes back to `Pending` with its engine ref kept, so its next turn (or a widget's call) first calls `thread/resume` and then `turn/start`, as after a yardd restart. A per-app-server lock keeps a release and the next resume in order. `thread/closed` for a thread yardd did not release marks it released the same way (a turn still open there ends failed). Deleting a session and an agent's fresh context unsubscribe too, and so do yardd's private one-off threads (naming).
- **Claude:** the sidecar is stopped (`claude::release`, the same path as `kill`). The session was idle and the backend was already replaced, so the exit logs nothing; the next message starts a sidecar that resumes the Claude Code session.
- **A dead app-server** (crashed, killed) no longer poisons its account: the reader clears the account's cell (only if it still holds that client), fails every request in flight, and marks every session that had a thread there released. A turn that was running ends failed: "Codex stopped during this turn (its app-server exited). The next message resumes the conversation." The next use starts a new app-server. Before, the `OnceCell` kept the dead client and every request failed "connection closed" until yardd restarted.

**Switching engine** cannot share a thread, so it is a hand-off. Picking a model of another engine in a session asks for confirmation, then starts a new session on that engine in the same folder, with the same branch, title and sidebar group and a `parent` pointing back.

- yardd builds the first message from the source session's event log, with no model call, because the source is usually out of quota.
- The brief holds the original task, each later prompt, each turn's final answer, the uncommitted changed files and any unresolved state. It is capped at 12,000 characters, keeping the task and the most recent turns.
- Agent text is quoted in `<earlier-work>` blocks marked as a record, not instructions.
- Refused for the same engine, and while a turn is running: two agents must not write to one folder at once.

## Resources

- yardd samples CPU, load, memory and disk every 3 s with `sysinfo` and keeps an hour of CPU history in memory.
- **Per session:** the same sample sums CPU and memory over each session's processes: the Claude sidecar tree, the terminal shell tree, and for Codex the app-server's children matched by command folder and start time. Codex runs one app-server per account, so two Codex sessions of one account running commands in the same folder at the same moment can be mixed up.
- **No memory cap.** yardd never stops a running session for its size. A per-session cap (`YARD_SESSION_MEM_CAP_GB`) existed until 0.2.1 and was removed on 2026-11-15: a polling soft cap killed real work, and the queue below already keeps new sessions off a busy host.
- Threads are not processes: on Linux `sysinfo` lists each thread with its process as parent and the process's memory as its own. They are skipped; counting them made a Claude CLI look 8.7 GB (2026-11-14).
- **Queue:** a new session waits (status `queued`) while 1-minute load is above the core count or free memory is under `YARD_MIN_FREE_GB` (default 2). The daemon tests turn the load check off (`YARD_TEST_IGNORE_LOAD=1`) and force pressure where they test the queue. One queued session starts per sample once there is room. "Start anyway" skips the wait. Queued sessions survive a restart.
- **Processes tab:** `session.procs { sessionId }` answers `session.procs { sessionId, procs }`, the session's processes from the same sample (so up to 3 s old): `pid`, `ppid`, `name`, `cmd` (at most 400 characters), `cpu` (percent of one core), `memBytes`, `ageSecs`, `start` (seconds since the epoch), `origin` (`agent` for the sidecar tree or a Codex command, `terminal` for a shell) and `engine` (the engine's own process: a Claude sidecar, or the sidecar's child whose name or command line says `claude`, which is the CLI the SDK starts). It is asked for, not broadcast, and routes through the hub like any session op. The tab (`ProcPane`, next to Changes, Terminal and Pull request, so hidden below 1100 px) asks when opened and every 3 s while visible, and draws the tree with each command line shortened. An older remote yardd answers "unknown op": the tab stays on "Loading…".
- **Stopping a process:** `session.kill { sessionId, pid, start?, signal: "term"|"kill", tree? }` answers `session.killed { sessionId, pid, signal, tree, result, reason?, running }` (and the request's `ref`). yardd does not trust the pid: it reads the process table again at that moment, works out every session's tree from it the same way as the sample, and refuses (`result: "refused"` with a `reason`, not an error) pid 0 or 1, its own pid, any engine process (a Claude sidecar or its CLI, a Codex app-server or ACP agent, which other sessions may share; these are never in the tab's reach, the session's Stop and Delete end them), a pid that is not in this session's tree now, a pid another session lists too, and a `start` that differs from the listed one (the pid was reused). It signals from that same read, then checks every 100 ms whether the processes are gone (a zombie counts as gone) for up to 3 s after SIGTERM or 2 s after SIGKILL: `killed`, or `running` with the pids still alive. Killed pids are dropped from the cached list so the next `session.procs` leaves them out. `tree: true` signals the process and everything below it in the session's list, children before parents in one pass with no pause between, so a parent waiting on a child has no time to start a replacement. Not by process group: a command that did not start its own group shares the one of whatever started it, which can be the engine, and the list is what was checked. Only `term` and `kill` exist; anything else, a missing pid or an unknown session is an error. It routes through the hub like any session op; an older remote answers `unknown op`, which the row shows as that host needing Update Switchyard. The tab offers each row a Stop (×, shown on hover, always on touch) that asks for a second click, with "Stop with children" when the row has children; a row still running after SIGTERM offers "Force kill", also confirmed. The engine's rows offer nothing. The tab refreshes the list after each answer. Known limit: a Codex or ACP child is matched to a session by folder and start time (above), so a helper the shared agent starts in that folder while a command runs there would look like the session's.
- **Pressure episodes** (`pressure.rs`, 2026-11-27, after a load of ~140 on 28 threads with swap full for over an hour that nobody could explain afterwards): every sample also decides whether the host is overloaded: 1-minute load above the cores, free memory under `YARD_MIN_FREE_GB`, or `YARD_FORCE_PRESSURE`: the queue's rule above, so an episode is open exactly while the Hosts page reads "Under pressure". The sampler also reads swap (`host.swapUsed`/`swapTotal`) and keeps its peak, but full swap is never pressure by itself: Linux leaves stale pages there long after the trouble is over (this server sat calm at load 1.3 with 11.5 GB available and swap 4078 of 4095 MB used). Swap at least 90% full is listed as a reason (`swap`) only beside `memory`; PSI or paging rates would say more but are Linux-only and a second source. The first such sample opens an episode `{ id, start, end, lastSeen, reasons, peak, top }`; while it lasts yardd keeps each peak (load, memory used, swap used) and, at its worst sample (load per core plus memory and swap per GB of RAM), the top sessions (the top 3 by CPU and by memory from the guard's attribution, with title, engine and automation name) and the top processes that belong to no session (`pid`, `name`, `cmd`, CPU, RSS; another app or a test daemon left behind is often the culprit); anything under 0.05 cores and 64 MB is left out. A minute without pressure closes it, with `end` its last sample under pressure. Episodes are saved in `pressure.json` in the data dir, a week of them and at most 100; the open one is written when it opens and at most every 30 s while it grows, and one left open by a crash or power cut is closed at its last sample on startup. The `host` message carries `episode` (the open one, or null), `episodes` (the latest 5 closed, newest first) and `stalled` (`{ id, quietSecs }`: running turns with no event for 5 minutes while the host is under pressure, from `Session.event_at`); `host.pressure { host?, before?, limit? }` answers `host.pressure { episodes, more }` with older ones. Each yardd records its own; the hub prefixes the session ids in a remote's episodes and routes `host.pressure` by `host`. Once per episode, after 2 minutes, yardd sends the windows a `notify` ("<host> is overloaded"; the body says what the trouble is, "Load 140 on 28 cores" or "1.0 GB of memory free, swap full", and the worst contributor with its cores and memory, opening its session when that is one). Nothing is stopped. Test-only and inert unless set: `YARD_TEST_READINGS` (a JSON file read every sample whose `load`, `memFree`, `swapUsed`, `swapTotal`, `sessions` and `procs` replace what was read, for the queue as for the episodes; injected load and swap count for episodes even under `YARD_TEST_IGNORE_LOAD`, which otherwise ignores the test machine's own), `YARD_TEST_PRESSURE_CALM_SECS`, `YARD_TEST_STALL_SECS`, `YARD_TEST_PRESSURE_NOTIFY_SECS`. `yv iso up` gives every isolated daemon a calm readings file (var `readings`), so this machine's own load never shows there.
- **In the UI** (`PressurePanel.tsx`, words in `pressure.ts`): while any host has an open episode, a line in the sidebar above the usage strip (over the page on phones and with the sidebar collapsed) reads "<host> is overloaded · load 140 · mostly <worst> (8.0 cores, 9.0 GB)": the worst by CPU when load is the trouble, by memory otherwise, a session or a stray process (`node, pid 9`). A session gets Open and, while its turn runs, its own Stop (`session.interrupt`); no new kill path. Details opens the host's System tab, which lists the episodes (start, length, peaks, top use; sessions link to their conversations; Show older pages with `host.pressure`) and shows swap. A session yardd lists as stalled shows "No progress for 5 min: the host is overloaded (load 140)." under Working, with See Hosts; it goes when the turn moves again. The turn is never failed for it.
- Not built: worktree disk sizes. Merged and spent worktrees are swept (see Worktrees).

## Remote hosts

The local yardd is a hub. The UI keeps its one WebSocket to it.

- Each UI client gets its own connection from the hub to every remote yardd, piped both ways, so replies go back to the client that asked. It opens when the client connects and reopens with backoff, separately from the hub's own connection, which sets the host's status. An op sent while the host shows connected but the client's connection is not open yet (a page asking right after a reload, a phone coming back, a host just back) waits for it, in order, and makes a connection waiting to try again try at once; if it cannot open, the op fails with "this host is not connected", as one sent while the host shows offline does at once (`remote::Pipe`). Every op the client's connection takes gets one answer: the host's reply, or that error with its `ref` when it was not sent (an ssh tunnel that dropped as it went in fails it at once rather than waiting for the tunnel; so does a write that fails, for that op and every one behind it), or "no such host" when the host is removed first (`remote::Pending`). An op the host took whose answer the page waits on stays tracked until that answer comes in; if the connection ends first (a drop, the host removed, a write cut short), it gets one error that claims neither way: "Lost the connection to <host> before it answered. This may have gone through; check once it's back." The page waits on an op that carries its `ref` (the host answers those with it, done or refused), and, with no `ref`, on `session.create`, `session.approve`, `mcp.add` and `mcp.remove`: the pipe sends those with a `ref` of its own, taken off the host's answer, and an approval's or a connector's success is the host's `approval_resolved` or `mcp.change` news. A host older than v0.18.0 answers `session.create` with a `created` that has no `ref`: that settles the oldest create still tracked. Every error the hub makes for an op meant for a host carries that host's `hostId`, as the host's own errors do, so the page files it under that host (a connector's card is released by its own host's error only); the ones a connection's end causes come after its `host.down`, and a connector error for a host that is down shows in the banner, since its cards are hidden (`remote::error_for`). Other ops are fire-and-forget and not tracked (`Pending::awaited`).
- Session and terminal ids that cross the hub are written `<hostId>~<remoteId>`. An op with such an id, or a `session.create` with `host`, goes to that host with the prefix removed.
- A remote `snapshot` arrives as `host.snapshot`; its `host`, `models` and `usage` messages carry a `hostId`. Models, usage, accounts and the home folder are kept per host. `account.add` and `account.remove` carry `host` like `engines.status`, and the remote's `accounts` list comes back with its `hostId`.
- Hosts are saved in `hosts.json` in the data dir and changed with `host.add` and `host.remove`. Status (`connecting`, `connected`, `error` with a message) is broadcast as `hosts`. A host is `connected` once the hub's own connection has read its snapshot, not when the socket opens: before that the hub does not know whether it owns agents, and a move made in that gap was refused as "runs an older Switchyard".
- An `ssh` host gets one `ssh -N -L` tunnel shared by all clients, on a free loopback port, restarted with backoff up to 30 s and killed when the host is removed or yardd stops. The destination is passed after `--` and may not start with `-`.
- A `direct` host is a plain `ws://` URL, for local testing.
- Remote sessions stay listed but offline while their host is down.
- **Agents belong to the host they run on** (see Agents): the hub lists each host's own agents with its own, ids `<hostId>~<id>` like sessions, from what its own connection to the host hears (`remote::HostAgents`; the snapshot's `ownAgents` says the host can own agents, shown on the host as `ownAgents` in `hosts`). An agent op naming such an id (`agent.ask`, `agent.save`, `agent.delete`, `agent.request`, `agent.seen`, and `slack.app.*` / `slack.grant` with `agentId`), or `agent.save` / `agent.new` for a new agent with a remote `host`, goes to that host like a session op; `agent.saved`, `agent.deleted`, `created` and a Slack op's answer come back prefixed. A host's own `agents` and `slack` broadcasts are not piped: the hub merges them into its own (`agents::shown`, `slack::view`).
- **The Hosts page** uses compact full-width host rows, this machine first, then remotes. Selecting a row opens one inspector below the list. Sessions shows running/queued activity with links to conversations; Engine logins manages each engine's accounts, login, providers and removal; System contains resource detail/history, queue policy and remote Update/Remove. Offline errors and install progress stay visible above the tabs. The host list stays the same size regardless of login text or activity. Add host opens its form above the list. `#/sessions` provides searchable active/all/archived conversations. Workspace opens on Terminal and also offers Subagents and Processes. Pull requests open on GitHub from the conversation header.
- **The web version as a host:** its compose file publishes the container's 47717 on the server's loopback, so an `ssh` host for that server reaches the same yardd as the web page. The installer refuses to install over a yardd it did not set up.
- **Install from the app:** when the ssh tunnel is up but no yardd answers behind it, the host reports `noDaemon` and its card offers "Install Switchyard". The hub runs `scripts/remote-install.sh` (compiled into yardd) over the host's ssh as `bash -s -- <port>`, streaming each line to every client as `host.install`. The script downloads `switchyard-linux-{x64,arm64}.tar.gz` from the `latest` release (built by `.github/workflows/linux-host.yml`), swaps it into `~/.local/share/switchyard`, runs yardd as a systemd user service with lingering (or detached where there is no user systemd), checks `/health`, and reports whether `claude` and `codex` are installed and logged in, and `opencode` and `grok` where installed (`@@engine` lines). The same card offers "Update Switchyard" on a connected SSH host. `YARD_SSH` and `YARD_RELEASE_URL` exist for the tests.
- `scripts/install-host.sh` builds and installs from a checkout instead, for development.

## Web version

A container image (`Dockerfile`, `docker-compose.yml`) runs yardd as a web app, for Coolify or plain Docker Compose. Details and setup are in the README.

- One image: release yardd, `ui/dist`, the compiled sidecar, and pinned `codex`, `claude` and `gh` from npm and GitHub's apt repository. It runs as uid 1000 with `HOME` (logins, `~/.yard`) and `/workspace` as volumes.
- No sign-in, by decision: it is meant for a private network such as a tailnet. Anyone who reaches it can run commands in the container.
- yardd binds `YARD_BIND=0.0.0.0` there; loopback stays the default everywhere else. The Origin check stays: `YARD_ALLOW_ORIGIN` (comma separated) names the page's URL. yardd never trusts an Origin because it matches the Host header, since a rebound DNS name would pass that. It warns at startup when bound off loopback with no allowed origin.
- The page uses `wss://` when it is served over https (behind Coolify's Traefik).
- Codex's `workspace-write` sandbox uses bubblewrap, which cannot create namespaces in an unprivileged container (checked: "No permissions to create a new namespace"; with seccomp and AppArmor unconfined it still failed on the loopback interface). The image sets `YARD_CODEX_SANDBOX=danger-full-access`, so the container is the boundary. Approvals stay `on-request`. Elsewhere the default is unchanged.
- The local host's name is `YARD_HOST_NAME`, else "This Mac" on macOS and the hostname elsewhere. A remote host may not take that name. The snapshot carries it as `hostName`, so the UI has it before the first `host` stats sample.
- `.github/workflows/docker.yml` pushes `ghcr.io/giovaniif/switchyard` (`latest` from main, and the short sha).

## Automations

Scheduled work, built into yardd (`crates/yardd/src/automations.rs`, UI in `ui/src/AutomationsPage.tsx`, words in `ui/src/automations.ts`). It replaces timers that started sessions from outside.

- **An automation** is saved in `automations.json` in the data dir (like `hosts.json`; an unreadable file is kept as `.json.bad`): name, prompt, folder, engine, model, effort, permission, base branch, schedule, optional `window: { start: "HH:MM", end: "HH:MM", days?: [0–6] }`, `skipWhileRunning` (on by default), optional `maxFiveHour` and `maxWeek` usage limits (percent), `enabled`, `nextRun` and `lastRun`. A schedule is `interval`, `daily`, `weekly` or `manual`; a manual one never fires on its own (`nextRun` is always null; the card reads "Manual · runs when started") and runs only from Run now or `automation.run`.
- **Schedule**, in the host's local time zone (chrono `Local`): `interval` (every N minutes or hours, counted from the last slot), `daily` at `HH:MM`, `weekly` on a day (0 = Sunday) at `HH:MM`, or `manual` (never on its own). A time that does not exist on a daylight saving day runs an hour later; one that happens twice runs the first time.
- **Active window** (interval schedules only; rejected for daily, weekly and manual): local `start` inclusive, `end` exclusive; `start > end` wraps midnight. Both times must be strict `HH:MM` and different. Optional `days` is a nonempty list of unique weekdays (0 = Sunday); omission means every day. Days name the window's start date: Friday 21:00–Saturday 09:00 belongs to Friday. Slots align to the start, then start + interval, stopping before the end: hourly 21:00–09:00 runs 21:00 through 08:00; 90 minutes runs 21:00, 22:30 through 07:30. Slot and boundary times use the same chrono `Local` gap/fold resolution as `next_at` (gap shifted an hour, ambiguous local time resolved once). Without a window, interval behavior is unchanged.
- **Scheduler:** one task ticks every 15 s (`YARD_AUTOMATION_TICK_MS` for tests). An enabled automation whose `nextRun` has passed fires, and `nextRun` moves to the next slot after now before it runs. A run missed while yardd was down therefore fires once, not once per missed slot. Saving a changed schedule or window, clearing a window, or enabling recomputes `nextRun`; disabling clears it. A windowed interval jumps to the next allowed window when outside; saving at 14:00 yields 21:00, saving at 23:10 yields 00:00 for hourly 21:00–09:00. An overdue windowed run fires once only if now is inside; outside it only advances and broadcasts/persists `nextRun`, without a session or skip record.
- **A run** is `session.create` with a worktree, `permission` and the title `<name> · YYYY-MM-DD HH:MM` (`<name> <title> · …` when `automation.run` carries a `title`, e.g. `Loop verify #161 · 2026-11-17 04:38`); the session's `Meta.automation` is the automation's id (set when the session is inserted, so the first broadcast already has it). Before cutting the worktree yardd runs `git fetch origin` (60 s limit, no credential prompt; a failure only logs). The base is the chosen branch or the repository's default (`origin/HEAD`, else main or master), taken from `origin/<branch>` when that exists. A folder that is not a repository with commits runs in place, one at a time.
- **Skips** (scheduled runs only; "Run now" starts one regardless of the window too): the previous run is still going (a session with this `automation` that is running, waiting for approval or queued), or the engine's usage is above a limit. Claude reports fractions, Codex percentages; a window that already reset is treated as unknown, like the old loop budget check. The outcome is saved as `lastRun` (`started`, `skipped` with a reason, or `failed` with the error).
- **Auto-archive:** when a run's first turn ends, yardd archives it unless it is pinned. Later turns (someone reopened it) do not re-archive. Pinning an archived session unarchives it, so pinning a run brings it back to the sidebar. The sidebar leaves out every unpinned session with `automation` set; a clock row, "Automations", opens the page, with a green dot while a run works and an amber one ("A run needs approval", from `runsAttention()`) while a run waits for approval, which outranks working: the run is not in the sidebar, so this dot is the only sign there that it waits for the user.
- **Ops:** `automation.list`, `automation.save { automation }` (an `id` edits; only the fields sent change, so a switch sends `{ id, enabled }` and keeps its window; `window: null` clears it; answers `automation.saved`), `automation.delete { id }` (its runs stay as ordinary sessions: `automation` is cleared), `automation.run { id, force?, prompt?, title? }` (answers `automation.ran { outcome, sessionId, reason }`; `prompt` replaces the saved prompt for that run only, trimmed, non-empty, at most 200 KB; `title` is one line of at most 80 characters put after the name; the saved automation does not change). Every change broadcasts `automations`, and the snapshot carries the list.
- **Per host:** each host runs its own automations. The four ops route by `host` like `session.create`; a remote's `automations` message gets a `hostId`, and its snapshot reaches the UI as `host.snapshot.automations`. Session ids in a run's reply carry the `<hostId>~` prefix like any session id. The page groups runs by host and the automation id.
- **The page:** a grid of small cards (`auto-fill`, at least 280 px; one column below 700 px), under a heading per host when more than one host has automations. A card shows only what reads at a glance: the name (a cloud when remote), the schedule in words with the next run, the Enabled switch, and one status line from `cardStatus()`: a working run (turning mark, elapsed time; it opens the run), else the last run (`Last: 22m 33s · 1 h ago`, or Failed/Stopped), or a skip or failure to start with yardd's reason cut to one line (amber or red; the whole reason in the tooltip). Below it a strip of the last ten runs as marks, oldest left, each opening its session with time, duration and outcome in its tooltip and label; a run is named by its start time (`runTime()`: `04:38`, `Mon 03:38`), never by its title, which only repeats the automation's name; what a title adds after the name is kept (`runLabel()`: `04:38 #161` for a `Loop verify #161 · …` run). The marks are the sidebar's (`.mark-*`) plus a filled dot for done, a square for failed and an amber ring for a skip, so they differ by shape too. The footer's icon buttons: Runs (every run, newest first, with how long it took and a pin, the way a run goes back to the sidebar), Run now and Edit. Runs shows only once there are runs, so it sits left of Run now: the first run must not move Run now out from under the pointer. The labels are pure functions in `automations.ts`; windowed schedules read "Every hour · 21:00–09:00", with e.g. ", Mon–Fri" for start days. The interval form adds an optional "Only between" pair of time pickers and day chips; no selected chips means every day. Next reads yardd's real next slot. The form reuses the new-session host, folder, branch, model and permission pickers.
- **The agent loops** (since 2026-11-16): patrol, maintain and review are automations named `Loop <kind>` on the live yardd, no longer systemd timers. `verify/yv loop automations --apply` builds them from `verify/loops.json` (schedule, optional window, engine, model, effort; the Claude budget becomes `maxFiveHour` 70 and `maxWeek` 80; permission `full`; base `main`; folder the loops' checkout) with a fixed prompt from `automationPrompt()` in `verify/src/loop.ts`, which tells the agent to read its start time with `date -u` where `prompt()` writes it in. It finds them by name, so rerunning edits them, and turns off the old timers. Patrol has an hourly 21:00–09:00 window. The CLI compares windows when planning updates, passes null to clear a removed window, and fails with `oldYardd` plus an update hint if an older daemon rejects or silently discards the window. Deploy the new yardd before applying the loops. Keeping the `Loop <kind>` title prefix lets `yv loop status`, `log`, `tidy` and the patrol's checks find automation runs like the old ones. Verify and babysit (since 2026-11-17) are manual automations `Loop verify` and `Loop babysit`: `verify/yv loop run verify --pr N` keeps its own skip rules (pause flag; the automation's Enabled switch), saves the automation when its settings differ, then sends `automation.run { force: true, prompt, title: "#N" }`, so a verifier run is on the Automations page and never in the sidebar. Against a yardd from before manual schedules (`automation.save` answers `unknown variant manual`) it starts an ordinary session as before and says so (`oldYardd` in its output). Lost in the move: the pause flag (`yv loop pause` now stops only `loop run`; switch an automation off instead) and the machine-load skip.
- Not built: a machine-load skip, a per-automation retention count, run history for skipped runs beyond the last one.

## Sidebar

- **Go to** (`ui/src/NavigationPalette.tsx`, search in `navigation.ts`): Cmd-K
  opens a session and page palette from anywhere, including the composer and
  terminal (Ctrl-K on non-Mac platforms). The Switchyard heading opens it too.
  Search matches title, engine, host, folder, branch and group, including
  archived sessions and automation runs; an empty search shows the eight most
  recent sessions the sidebar lists (no archived sessions or runs) and the
  pages. Arrow keys select, Enter opens, Escape closes and
  returns focus. Pages include New session, Agents, Automations, Connectors, Settings,
  Hosts and Import; Import opens the existing dialog.
- **Mac app menu:** Settings… (Cmd-,), Import Sessions… and Go to… (Cmd-K)
  live under Switchyard. The shell dispatches fixed `switchyard:navigate`
  DOM events and advertises `__SWITCHYARD_APP_MENU__` during webview setup.
  Only shells advertising that capability hide Settings and Import in the
  sidebar, so browsers and older shells keep working. No extra IPC permission
  is granted to the loopback page.

- Each session row has a menu (the … button or right-click) with Rename, Pin, Move to group, Remove from group, Archive or Restore, and Delete. `session.rename { sessionId, title, ref }` stores a trimmed, single-line title of up to 60 characters and answers with `session.renamed { ref }`. The title appears on the same session and worktree in every window. `session.update { pinned?, archived?, group? }` stores the flags and group name in the session's metadata. These changes survive restarts and route through the hub for remote sessions. Archiving unpins. `group` is a trimmed name of up to 60 characters. Null or blank clears it, and leaving the field out keeps it.
- **Session names** (`crates/yardd/src/naming.rs`). A session starts titled with its task's first line (60 characters). Right after `session.create` sends the task, yardd names it from the task alone, so a long first turn no longer shows a fragment or a bare link in the sidebar. After its first completed turn, and again after its third, it names it again from the conversation; a session never named yet (one from before naming, or whose every try failed) is named after whichever turn it completes next. Each naming reads the task plus the latest messages (each cut to 900 characters, 6,000 bytes in all), with instructions for a 3–7 word title. The model is Codex `gpt-5.6-luna` at effort low, else Claude Haiku (`claude-haiku-5-5`) when Codex is not listed or its turn fails, each in a private restricted thread on the Default account through Switchy's `switchy-` plumbing (Codex: ephemeral, read-only, approvals never; Claude: a fresh restricted sidecar), never saved or shown. This spends the plans a little, a few hundred tokens per name. `YARD_NAMING_CODEX_MODEL` and `YARD_NAMING_CLAUDE_MODEL` pick other models (a model the host does not list is skipped); `YARD_SESSION_NAMES=off` turns naming off (the test harness and `yv iso` with fake engines default to off, since naming adds engine turns; the sidebar map turns it on). One session is named at a time, each try limited to 60 s. The answer is cleaned (first line; quotes, markdown, a `Title:` prefix and a trailing period dropped; at most 12 words, cut at a word to 60 characters); when nothing usable comes back the title stays. OpenCode's free models were tried first and dropped: their anonymous free tier answered "Free usage limit reached" and OpenCode retries that forever. `Meta.titleSource` says where the title came from: absent (first line), `auto`, or `user`. `session.rename` sets `user`, and a `user` title is never replaced, even when the model answers after the rename. Automation runs (their `Loop <kind> · …` titles are looked up by the loops) and hand-offs (`parent`) are never named.
- **Drag and drop** (native HTML5, no library): a row dragged onto a group (its header or rows) moves into it, onto the ungrouped Sessions rows leaves its group, onto Pinned pins it. A group or Sessions drop also unpins, since those sections only show unpinned sessions. `dropUpdate` in `ui/src/sidebarDrop.ts` turns a drop into one `session.update { pinned?, group? }`, or nothing when the session is already there; such a section refuses the drop, so the cursor says no and nothing lights up. The dragged row dims; the section under the pointer is highlighted; a collapsed group opens after 600 ms over it; with no ungrouped rows a "Drop here to remove from group" slot appears while dragging; Escape cancels (the browser's own). The drag carries the session id as `application/x-yard-session`. Archived rows and rows on an unreachable host are not draggable, like the menu. No reordering: order comes from creation and activity and is not stored. Touch has no HTML5 drag, so in the phone drawer (below 700 px) rows are not draggable and the … menu is the way; it is also the keyboard path. The Mac app turns off Tauri's own drop handler (`disable_drag_drop_handler`), which is what lets drags inside the page work.
- Groups have no record of their own: a group exists while a session names it. The sidebar lists Pinned, then ungrouped sessions (creation order), then one collapsible section per group (sorted by name, newest activity first inside; `sidebarLayout` in `store.ts`). Pinned and archived sessions are not shown inside groups. A group header shows name, count and chevron; collapsed, it also carries the Automations row's dot when a session in it waits for approval (amber) or works (green), since its rows are hidden. Its menu renames or ungroups every session that carries the name, archived ones included. Which groups are collapsed is per browser (`yard.groups.collapsed` in localStorage).
- **Mark as unread/read** in a session row’s menu persists per browser in `yard.seen` (`-1` is an explicit unread mark). An unread chat gets a blue dot, bold title and Unread label. Opening it deliberately or choosing Mark as read clears it; reloading an already open chat preserves the explicit mark. Running, approval, queued and offline states keep their priority. Other windows follow through `storage`.
- **Row marks** (`ui/src/rowState.ts`): a turning ring while running, an amber "!" that breathes while it waits for approval, a blue dot with a bold title and "Answered" for a finished turn not opened yet, red for a failure (live, or an unseen failed turn), a hollow ring when idle; shapes differ so they read without color. yardd stamps every session with `turnEndedAt` and `lastTurn` (`completed`, `failed`, `interrupted`) when a turn ends. Which answers were seen is per browser (`yard.seen` in localStorage, the `turnEndedAt` last had open while the window was in front; other windows follow through `storage`). Turns that ended before the browser first kept track (`yard.seen.since`) count as seen, and a stopped turn never stands out.
- Archived is a dock under the scrolling list, just above the host rows, so it never scrolls away. It is shown whenever something is archived, collapsed by default; opened, its rows expand upward in a list with its own scroll (max 40% of the window height, at most 320 px). Archived sessions still open and run, so the collapsed dock carries the same dot as a collapsed group when one of them waits for approval or works.
- The sessions sidebar and the workspace pane are resizable; widths are per browser.
- The top nav: New session, Hosts, Sessions, Subscriptions, Agents, Automations, Connectors, Import and Settings. **Settings** (`#/settings`, `ui/src/SettingsPage.tsx`) holds what the browser keeps for itself: the Notifications switch and, under Sidebar, a "Show in sidebar" switch per host, This Mac included. The bottom block lists a row per shown host (CPU · memory, opens Hosts) under the compact Usage popover trigger, which reads only the shown hosts. Hidden hosts are per browser (`yard.sidebar.hiddenHosts`, host ids; `ui/src/sidebarHosts.ts`); the Hosts page, the pickers and the session list still show every host, and Settings links to Hosts for when every row is hidden.
- Below 700 px (phones on the web version) the sidebar is a drawer over the page, so the page keeps the full width: a menu button first in each page's bar opens it; picking anything in it, any route change, the backdrop or Escape closes it. The workspace pane is already hidden below 1100 px.

## Notifications

- Switchyard shows an OS notification when a session needs the user: it waits for an approval, or it failed, or a turn of at least a minute finished (`MIN_TURN_MS`; an automation's run reports a finish on its page, not in a banner). Only when the window is not focused or that session is not the one open (`shouldNotify` in `ui/src/notify.ts`).
- The banner says what happened, from the session's events (yardd broadcasts every session's events): the command or edit waiting for approval, the error, or how long the turn took and the start of its last answer as one plain line (`noticeText`, `plainLine`). A turn's length is timed on the page's clock, since a remote host's `at` would mix two clocks; a turn already running when the page loaded has no length and is reported. Clicking one focuses the window and opens the session.
- The UI works out the cases from what yardd already sends: a `session` message that changes status (`needs_approval`; running to idle) and the `turn_completed` event before it. A failed turn ends `idle` too, so the event's status tells a failure from a finish, and a Stop (`interrupted`) says nothing. A snapshot (also after a reconnect or a host reload) only sets the baseline, so opening the page never notifies.
- The **Notifications** switch on the Settings page turns them on and off, kept per browser (`localStorage` `yard.notifications`). Turning it on asks the browser for permission; a refusal leaves it off and says why.
- Browser: the Web Notifications API, one banner per session (`tag`). Desktop app: WKWebView has no `Notification`, so the UI calls the Tauri notification plugin through `window.__TAURI__` (`plugin:notification|notify`). The capability gives the loopback page only those calls (is-permission-granted, request-permission, notify, register/remove-listener), next to the folder dialog. A click is reported by the plugin's `actionPerformed` listener, with the session id attached as `extra`; if the listener cannot register, a window focus within 20 s of a banner counts as the click. No change to `tauri.conf.json` or the plist.
- `notify { title, body?, sessionId?, url? }` is the way for an agent or script to reach the owner. yardd checks it (a title; `url` only http or https), clips the text, and broadcasts `{ op: "notify", ... }` to every connected window, which shows it under the same setting. A note that names no session shows even in a focused window. The sender gets `notify.sent { windows }`, the number of connected clients counting itself, so a caller can tell that nobody was there. `verify/yv notify` sends it to the live yardd; an older yardd answers `unknown op`. A session on a remote host is routed to that host like any session op, and its `notify` comes back through the hub with the id prefixed.
- Not covered: the real browser prompt and OS banner (the feature map replaces `window.Notification` with a recorder), and the desktop app, which cannot run in CI.

## Engine logins

- `engines.status` (per host) reports each engine as installed or not, and logged in or not: `claude auth status` (JSON), Codex's `account/read`, the first line of `grok models` ("You are not authenticated.", or who is signed in), and for OpenCode the providers `opencode models` lists (`providers`; it counts as logged in with any, its free ones included). The UI asks once per host snapshot. A logged-out engine shows a notice on the new-session page and in its sessions, with **Log in**; typing `/login` in a session opens the same dialog. The Hosts page lists every engine per host; OpenCode's button is **Add provider**, since it is never logged out.
- Codex: `account/login/start` with `chatgptDeviceCode` on the shared app-server returns a link and a code; `account/login/completed` ends it, then models and limits reload. No terminal involved.
- Claude Code: `claude auth login` runs in a PTY (wide, so the link never wraps). yardd strips the escapes, sends the sign-in link to the UI, and types the code the user pastes back. Exit 0 is success; "Invalid code" ends it as failed. A login that gets no code within 15 minutes is stopped and reported as failed ("Start again"): a closed tab never sends `login.cancel`, and ending it when its window disconnects would break a phone that leaves the app for the sign-in page. Idle Claude sessions then get a fresh sidecar with their next message, since one started logged out keeps failing.
- Grok: `grok login --device-auth` runs in a PTY like Claude's. yardd reads the link and code it prints and shows them like Codex's; exit 0 is success. The account's agent is then restarted once idle (it may have started logged out), and its sessions resume.
- OpenCode: logging in adds a provider. There is no CLI flow to drive without its terminal UI, so a short-lived `opencode serve` on a free loopback port, with a random password and stopped by a shell in front of it once yardd lets go (it never reads stdin, so it would outlive a killed yardd), does it through its own endpoints. `login.start` answers `providers` (from `/provider` and `/provider/auth`: connected first, then popular, then by name; ways in that need a typed setting, such as GitLab's instance URL, are left to the terminal). `login.choose { provider, method }` answers `key` for an API key, `url` for a sign-in that gives back a code, or `code` for a device code (the server finishes it; yardd waits on `oauth/callback` up to 15 minutes). `login.code` carries the key or code: `PUT /auth/<provider>` or `oauth/callback`. The key passes through yardd's memory only. Every request has a time limit: the server accepts connections before it answers them.
- All go through the CLIs' own flows and files; yardd never sees a token, and an OpenCode API key only passes through. See Policy and cost for the Claude side.
- Every login op takes an optional `account` (Default when left out), and `login` progress messages name it. A Claude login runs `claude auth login` with that account's `CLAUDE_CONFIG_DIR`; a Codex login runs on that account's app-server. Logins of different accounts can run at once. After a Claude login, only idle sessions of that account get a fresh sidecar.

## Accounts

More than one login per engine on a host, for example a personal and a company Claude Code subscription.

- Each engine has a **Default** account: the login the CLI already has, run with yardd's own environment, unchanged. Added accounts (`account.add { engine, name, host? }`) each get a folder `<data dir>/accounts/<id>/`, used as `CLAUDE_CONFIG_DIR` for Claude Code (the sidecar, `claude auth status` and `claude auth login`), as `CODEX_HOME` for Codex (that account's app-server), as `GROK_HOME` for Grok (its agent, `grok models`, `grok login`) and as `XDG_DATA_HOME` for OpenCode (its logins and sessions; its config stays the user's). OpenCode has no variable of its own for this, so commands its agent runs see the moved data folder too. These variables are documented by their CLIs (XDG by the spec OpenCode follows); Switchyard never reads what is in the folders. Names are unique per engine (case-insensitive) and at most 40 plain characters.
- They are saved in `accounts.json` next to `hosts.json`, per host, and listed in the `snapshot` and the `accounts` broadcast. `account.remove { id, host? }` cancels a login in progress, stops the account's Codex app-server, and deletes the folder, and with it the login. It is refused while any session (archived ones too) uses the account; the refusal names up to three of them and marks the archived ones, which are not in the sidebar. An agent keeps its own `account` (picked in its form); changing it moves its conversation to a fresh context on that login (see Agents).
- `engines.status` answers `{ engines: { claude, codex, opencode, grok, gemini }, accounts: { <id>: state } }`: Default under `engines`, the added ones by id. Adding or removing an account triggers a fresh answer.
- `session.create` takes `account` (an id of that engine; Default when left out) and checks it. It is saved in the session's metadata (`account`, `default` for older rows), so resume after a restart and every later turn use the same account: the Claude sidecar is started with the account's `CLAUDE_CONFIG_DIR`, and the session's Codex thread lives on that account's app-server (a thread is only known to the `CODEX_HOME` that made it). **The account cannot be changed mid-session**; start a new session instead.
- A hand-off starts a session on another engine, so the source's account does not carry over: it runs on the target engine's Default unless `session.handoff` names an `account` of that engine (the UI does not offer one yet).
- **Usage is per account**: Default's limits stay at `usage.claude` and `usage.codex`; an added account's are at `usage.accounts[<id>]` in the same shape as its engine's. The usage popover shows Default's and every added account's limits from shown hosts; Subscriptions lists all hosts. The separate account chip names the selected engine and account. Codex model lists are loaded from Default's app-server and shared; so are Grok's (Default's agent) and OpenCode's (Default's providers, so an added OpenCode account with other providers still sees Default's list). Claude's list is per account: yardd reads each Claude account's `supportedModels()` with `yard-claude models` (a Claude Code with no prompt, so no turn) at start, after a login or a new account and daily (`YARD_CLAUDE_MODELS_EVERY_MS`), keeps them in `claude-models.json`, and lists their merge; a model only some accounts' plans have carries `accounts`, and the picker offers it only on those. Before the first read, or when reads fail, a built-in list stands (Opus, Fable, Sonnet, Haiku).
- **UI.** The Hosts page lists each engine's added accounts under it with their login state, Log in and Remove (a second click confirms), and **Add account**. The separate account picker lists engine, account name and login identity as full rows, without segmented Default/Work controls. In an existing session it is read-only; starting a new session is required to use another account. The session header and the sidebar row name the account, again only where the engine has a choice (or the session is not on Default).
- Not built: moving a session to another account, per-account models, logging out without removing.

## Worktrees

Every new session in a git repository with at least one commit gets its own branch and worktree at `<data dir>/wt/<repo>/<branch>` (`~/.yard/wt` by default); there is no switch for it. A folder outside a repository, or in one with no commits, runs in place, and the session says so in a note. Both engines share this layout.

- **Host-card alignment:** closed cards stretch to the same height within a row and keep their footers at the bottom. Engine marks have two fixed columns with truncating login notes. Opening Details lets cards grow independently.

- **New-session defaults:** the last used host, source project, model and effort are kept per browser (`ui/src/sessionDefaults.ts`). New sessions and reloads restore them, including remote hosts. Projects and model choices are kept per host; replies and model/effort changes in existing sessions update the defaults too. Model catalogs validate saved models and efforts once loaded. A configured host that is temporarily offline stays selected with Send disabled; a removed host falls back to local.

- **Folder picker:** a button under the prompt box, next to the host and base branch buttons. It lists recent folders (recent picks, then sessions' repositories), browses the chosen host's folders as a path is typed (`fs.dirs`, routed through the hub like `git.branches`), and in the desktop app opens the Finder dialog (Tauri dialog plugin; the capability lets only the loopback page call `dialog:allow-open`).

- The branch is named from the task (`yard/<first words>-<4 hex>`) unless I type one.
- **Base:** the branch button lists local and `origin/*` branches, newest first (`git.branches`). The default is the current branch. The branch name comes from the task.
- The worktree is made before the session. If `git worktree add` fails, nothing else runs and no session exists.
- **Setup:** after a successful add, yardd runs `<data dir>/setup/<repo>.sh` if it exists, else `.yard/worktree-setup.sh` from the repo, with bash in the new worktree (`YARD_REPO`, `YARD_WORKTREE`, 5 minute limit). This is where `node_modules` and `.env` get linked. If it fails, yardd removes that worktree and branch and creates no session. It never runs in the original checkout.
- **Removal:** deleting a worktree session offers "Delete and remove worktree". It runs `git worktree remove`, never forced, and is refused if anything is uncommitted or untracked, or another session still uses the folder. The branch goes with `git branch -d`, which keeps unmerged branches.
- **Merged worktrees go by themselves,** for every repository, with no setting (`sweep.rs`). Every 10 minutes (first pass a minute after start; `YARD_WORKTREE_SWEEP_SECS` changes it, `0` turns it off) yardd looks at each linked worktree under `<data dir>/wt/<repo>/`, whether a session made it or not. One goes when it is clean (no changed or untracked files), no session in it is running, waiting for approval or queued, no process has its current folder inside it (a terminal, a dev server; an idle Claude session's own CLI does not count and is stopped first), and `gh pr list --state merged --search <HEAD>` returns a PR whose `headRefOid` is exactly HEAD, so a branch with commits after the merge stays. Then `git worktree remove` (never forced) from the main checkout and `git branch -D`: a squash merge leaves the branch unmerged to git, and GitHub keeps the PR's head commit. Each session that ran there gets a note ("Worktree and branch … removed: pull request #N was merged."); its conversation stays. A repository without GitHub, or a host without a logged-in `gh`, is never swept for a merge. A **spent** worktree goes the same way without asking GitHub: at least one session ran there and every one is archived, it is clean, and its HEAD is already on another branch (`git for-each-ref --contains HEAD` over `refs/heads` and `refs/remotes`, its own branch excluded), so no commit is lost. That covers automation runs that end without a PR, which no merge ever claims: hourly loops left over a hundred behind in a week and filled the server's disk. Their sessions get "Worktree and branch … removed: its sessions are archived and its commits are on another branch."
- **One writer per folder:** a session without a new worktree is refused while another session in the same folder is running, waiting for approval or queued.
- Uncommitted changes in the original checkout are not carried over.
- Files a setup script creates count as untracked unless gitignored, so they block removal.

## Comments on parts of an answer

Selecting text inside one agent answer (`data-quotable`) shows **Comment** over the selection (`QuoteReply` in App.tsx). It adds a quote card above the reply box with its own comment field, which takes the caret (Enter or Escape goes back to the reply box, Cmd/Ctrl-Enter sends). Several cards are numbered; the number scrolls its part into view, and while cards wait their parts are marked in the answers with the CSS Custom Highlight API (`::highlight(yard-quoted)`, from the selection's range kept in memory; no mark where the browser lacks it or after a reload). Before this, several quotes and one message went out as quotes then text, and the agent could not tell which remark was about which part (owner report).

What is sent (`withQuotes` in `compose.ts`): quotes with no comment go as before, each a Markdown blockquote, then the text. Once any card has a comment, the message starts with `Comments on parts of your answers:`, then one numbered list item per part, the blockquote and its comment indented under it (`(no comment on this part)` for one left empty; a comment starting with `>` is escaped), then the text. Plain Markdown, so every engine reads it. `splitComments` reads it back: Edit and Stop return each part to its card with its comment, the queued row counts comments, and the bubble shows each part with its comment under it.

Sending from a session or agent conversation scrolls the thread to the bottom and keeps it there, so the message just sent is in view.

## Terminal output as context

The Terminal tab's **Attach** menu puts terminal output in a card above the reply box: the selection, the rows on screen, or the last 100 or 500 lines (`ui/src/termRows.ts` reads xterm's buffer: wrapped rows join their line, the blank screen under the prompt is dropped). A card names the terminal and its line count, shows three monospace lines with More, and has ×; several stack next to quote cards. Sending puts each after any quotes and before the typed text as `Terminal 1, 8 lines:` and a fenced `text` block whose fence is longer than any backtick run inside (`snippetBlock` in `compose.ts`), so engines and older yardds read plain Markdown. A sent bubble shows closed fenced blocks as code (`fenceSegments`). Cut to the selection cap (20,000 characters).

## Attachments

Files and images go with a prompt, on the new-session page and in the reply box: paste (clipboard images and files), drop on the composer, or the paperclip. Pending files show as chips above the text (a local preview for images) and can be removed. A prompt can be attachments alone.

- **Upload as soon as attached**, over the same WebSocket: `attach.put { ref, name, mime, data (base64), host? | sessionId? }` answers `attach.stored { ref, attachment | error, hostId? }`. It is routed like the other ops: `host` from the new-session page, `sessionId` (with its `<hostId>~` prefix) from a reply box, so the bytes land on the host that runs the session. Picking another host re-uploads the pending files there.
- yardd saves each file as `<data dir>/attachments/<id>/<name>` with a `.meta.json` beside it. The name is cut to its last path segment, with no control characters and no leading dot or dash. An inline image type (png, jpeg, gif, webp) must match the file's bytes, or the file counts as `application/octet-stream`.
- **Cap: 10 MB per file**, checked in the UI before sending (a red chip, Send disabled) and again by yardd. yardd's server and the hub's client to remotes both allow 32 MiB messages, so a full-size upload (about 14 MB of base64) fits on both hops; tungstenite's default 16 MiB frame limit was too close.
- `session.create` and `session.send` take `attachments: [id]`. The `user_message` event (and `queued`) carries `attachments: [{ id, name, mime, size, path }]`, never the bytes.
- **Codex:** the text gets an "Attached files:" list of absolute paths; each image also goes as a `{ type: "localImage", path }` item on `turn/start` (the `UserInput` shape of codex-cli 0.164, from `codex app-server generate-ts`).
- **Claude:** yardd sends the sidecar `{ type: "user", text, attachments }`. The sidecar (`message.ts`) puts each image first as a base64 image block, then one text block with the same path list. Images over 3.75 MB (5 MB of base64, the API's limit) are named by path only.
- **Thumbnails** in the thread come from `attach.get { ref, sessionId, id }` -> `attach.data { sessionId, id, mime, data }`, routed by session like any session op, so they work for remote sessions. yardd serves only attachments that the session's own prompts carry. Replies with a `ref` skip the reducer.
- **File links** in an answer open the file in the workspace. Agents write them in every dialect: an absolute path, one relative to the session's folder, `~/…`, `file:///…`, ChatGPT's `sandbox:/mnt/data/…` (read from the session's folder by name), often with a position (`a.ts:42`, `a.ts#L42-L50`). `ui/src/fileLinks.ts` decides which link destinations are files (any other scheme stays inert); clicking one sends `file.get { ref, sessionId, path }`, routed by session, and yardd (`crates/yardd/src/files.rs`) answers `file.data { sessionId, path, name, data | error }` with the first existing file the text can mean. It refuses folders and anything over the 10 MB upload cap, which is what fits across a remote host's link. An older host answers "unknown op", shown as an older Switchyard.
- **The file view** (`ui/src/FileView.tsx`, logic in `ui/src/filePreview.ts`) is a fourth workspace tab that exists only while a file is open (its × closes it; the default workspace keeps its three tabs). On a phone the workspace covers the conversation as usual. The header shows the name, what it is ("CSV · 92 rows · 4 columns · 9 KB"), the path with `~` for home, Reload and Download. Markdown renders with the thread's renderer, headings sized by level, and its links open relative to the file. CSV/TSV is a grid (RFC 4180 quotes, `;` sniffed, sticky header and row numbers, number columns right-aligned, first 1,000 rows). PNG/JPEG/GIF/WebP show on a checkerboard (SVG is left out: it can carry script). Other UTF-8 text shows with line numbers, the linked line marked and scrolled to. Binary or over 2 MB: "No preview" and Download. Download saves the bytes already read as `application/octet-stream` through a `download` link, never opens them, so an agent's HTML can't run on yardd's origin. The Mac app's `on_download` lets WebKit save to ~/Downloads and shows the file in Finder; without a handler wry cancels every download. Where a session has no workspace (an agent with no folder), a file link downloads directly.
- Hand-off briefs list each prompt's attachments by name and path.
- The desktop window disables Tauri's own drop handler so the page gets file drops; a drop outside the composer is ignored instead of opening the file.
- Not built: removing attachment files when their session is deleted.

The right pane has a Terminal tab next to Changes.

- yardd owns one PTY per session (`portable-pty`), running my login shell in the session folder. The UI renders it with xterm.js.
- yardd keeps 256 KB of output per terminal. Every chunk carries its byte offset, so a tab switch, page reload or reconnect replays the scrollback without gaps or repeats. xterm applies the replay to its display with input disabled, so historical terminal queries cannot send new replies to the shell.
- The shell starts the first time the tab is opened and stays alive after the app closes.
- A session can have several terminals, shown as tabs ("1", "2", ...) above the terminal. Numbers are not reused while the session lives. `term.list`, `term.new` and `term.attach` sit beside `term.open`, which attaches to the first live terminal or starts one. An exited shell stays listed until closed.
- **Add to message** appends the selected terminal text to the reply box as a fenced block, capped at 20,000 characters. It does not send.
- Not built: splits.

## Voice

Speech in the composer, all in the page: the browser's Web Speech API (`SpeechRecognition`/`webkitSpeechRecognition` to listen, `speechSynthesis` to read). No server-side speech service, no new dependency; yardd sees only the text that gets sent.

- **Dictation**: a microphone button next to the paperclip, in the reply box and on the new-session page. Press to listen (it turns into a pulsing red Stop button, "Stop dictation"); recognized words, interim ones included, go into the box after what is typed; press again or Escape to stop. The text stays to edit. Typing while it listens keeps the typed text and carries on after it. Sending stops it.
- **Voice mode** (session reply box only): the waveform button next to the mic. It says "Voice mode on.", listens, and sends after 1.5 s without new words (`SILENCE_MS`); quote cards and files in the box go with it. The agent's turn runs with the mic off. When the turn ends it reads the turn's last answer (`speakable`: Markdown stripped, code blocks and tables named instead of read, links read as their text, cut at 2,500 characters with "The rest is on screen."), in sentence-sized utterances because Chrome cuts long ones off, then listens again. A failed turn says why; a stopped one hands the mic back silently. An approval is announced ("Needs your approval: <command>") and then the mic stays off: approvals are answered on screen, never by voice. A bar over the text says what it is doing, with Stop (while reading) and Turn off. Escape stops dictation, then the reading, then voice mode. Leaving the session turns it off.
- **Talking over an answer** stops it: the mic stays open while it reads. What it hears is compared with the answer (`isEcho`: most words in common), so the answer coming out of the speakers does not cut itself off; "stop" (once final) only stops the reading; anything else stops it and starts the next message.
- What gets spoken comes from the session's items, not its status (`voiceCue` in `ui/src/voice.ts`): a quick turn can go running and idle in one render. The rules (speakable text, cues, the mode's state machine `voiceStep`, echo and stop words, error texts) are pure and tested in `ui/src/voice.test.ts`; `ui/src/useVoice.ts` drives the browser APIs.
- **Where it works.** Feature-detected, never by user agent. Chrome and Edge: recognition runs on Google's servers (needs the network; audio leaves the machine). Safari on macOS and iOS: Apple's recognizer (Siri/Dictation must be on). Firefox has no recognition: the mic shows greyed out (`aria-disabled`) with the reason as its tooltip and under the box when pressed, and voice mode is hidden. Recognition needs a secure context: `https://switchyard.giovani.sh` and `http://127.0.0.1` both are. A blocked microphone or recognizer says so under the box (`recognitionProblem`).
- **Desktop app** (WKWebView, macOS): WebKit exposes `webkitSpeechRecognition` to embedded web views (the `SpeechRecognitionEnabled` preference is on by default where `SFSpeechRecognizer` exists, and loopback counts as secure). It refuses recognition without a prompt unless the app's Info.plist has both `NSMicrophoneUsageDescription` and `NSSpeechRecognitionUsageDescription` (WebKit's `MediaPermissionUtilities.mm`), so `app/src-tauri/Info.plist` has them; tauri-build merges that file into the bundle and embeds it in dev builds. The hardened runtime also needs `com.apple.security.device.audio-input`, or macOS denies the microphone silently: `.github/scripts/app-entitlements.plist`, passed when `macos-sign.sh` signs the app. The first use asks macOS for Speech Recognition, then Microphone. WebKit then asks the app (`WKUIDelegate` `requestMediaCapturePermission`); `main.rs` answers through `on_permission_request`: microphone allowed (so there is no second, WebKit prompt), camera refused (wry used to grant every capture request when no handler was set). `speechSynthesis` works in WKWebView with the system voices.
- **WebKit's results.** Chrome reports each phrase as its own result; WebKit (Safari, the desktop app) keeps one result whose transcript grows while recognition runs. Words already used, an echo of the answer or text typed during dictation, are marked by result and word count (`Mark`, `markEnd`), never by index alone: an index skipped everything said after the greeting's echo, so voice mode never sent in WebKit (`verify/features/voice.md`, `voice-mode-webkit-results`).
- **iPhone app.** Native: `SFSpeechRecognizer` (on-device when supported) and `AVSpeechSynthesizer` on one `AVAudioEngine` with voice processing, so echo cancellation lets the user talk over an answer. Same rules, ported (`mobile/Switchyard/Voice`). See [mobile/README.md](../mobile/README.md).
- **Limits.** Language: the browser's default (the page is `lang="en"`, so Chrome listens for English; Safari uses the system's). Echo detection is a heuristic: on loud speakers without echo cancellation an answer may still cut itself off; headphones avoid it. iOS Safari may not keep recognition running while it speaks, so talking over an answer may not work on a phone. Safari may end recognition after a pause; it is restarted, and one that keeps ending right after it starts is given up with an error.
- Not covered by tests: real recognition, real voices, the browser and macOS prompts, the desktop app. The feature map (`verify/features/voice.md`) replaces both APIs with recorders.

## Tests

- `CARGO_BUILD_JOBS=2 cargo test -p yardd`: integration tests start the real yardd binary on a free port with a temp data dir and a fake `codex app-server` first on `PATH` (`crates/yardd/tests/fake-codex`). They spend no subscription usage and take a few seconds. The fake records each `turn/start` input in `turn-inputs.jsonl`, so the attachment tests check what reached the engine, locally and on a `direct` remote through a hub. A fake `ssh` (`YARD_SSH`) runs the remote installer locally against a `file://` release. Unit tests cover the hand-off brief, the hub's id rewriting, the `YARD_ALLOW_ORIGIN` list and attachment names.
- `cd ui && bun test`: `applyEvent`, the store reducer, the attachment helpers, the notification rule, the voice rules, and which hosts and usage rows the sidebar shows.
- `cd sidecar/yard-claude && bun test`: the Claude user message with attachments and the subagent mapping, without the SDK.
- A scripted Claude sidecar (`tests/fake-codex/yard-claude`, through `YARD_SIDECAR`) covers a sidecar that dies mid-session and one that ignores Stop, and a `[subagent]` turn (a nested subagent, and a `sleep 30` left running for the process list, which `tests/kill.rs` stops while the sidecar itself is refused); a `[stuck]` fake Codex turn covers the Codex side of Stop. A `[chart]` turn on either fake calls `verify/fakes/mcp-chart`, an MCP server with a widget; the fakes' MCP methods (status, sign-in with a real loopback redirect page, resource read, tool call) go to it too (`tests/connectors.rs`).
- `verify/yv` (docs/VERIFICATION.md) drives the built UI in headless Chromium against an isolated yardd with the same fakes plus a fake `gh` (one canned PR, and a few more for `pr.peek` by URL) and a fake `ssh`; the feature map `verify/features` covers the PR pane and the UI end to end. CI runs its selftest and two map areas.
- OpenCode and Grok (`tests/acp.rs`): the scripted `opencode` and `grok` in `tests/fake-codex` share one ACP agent (`acp-agent.mjs`, the same prompt markers as the fake Codex) and fake the CLIs' other commands: `opencode models`, an `opencode serve` with the login endpoints, `grok models` and `grok login --device-auth` with the real output. They log every request to `acp.log` and every prompt with the session's model, effort and mode to `acp-prompts.jsonl`.
- Not covered: anything that needs the real Claude SDK, real SSH hosts, real OpenCode or Grok turns.

## PR pane

The right pane has a Pull request tab.

- yardd runs `gh pr view --json` and `gh api graphql` (review threads) in the session folder through my own `gh` login. It holds no token and only reads.
- It shows the branch's PR: state, base and head, review decision, checks and review threads, unresolved first. It loads when first opened, refreshes when a turn ends, and polls every 60 s while visible.
- **Send to session** turns a review thread into a normal prompt quoting the comment with its file and line.
- **Review with another engine** starts a new session in the same folder on Codex for Claude work, Claude for anything else (or the first engine with models), asking for a review of `git diff <base>...HEAD` and no file changes. The prompt leaves out the PR title and comments, because they are untrusted text.
- **Hover cards on PR links.** A `https://github.com/<owner>/<repo>/pull/<n>` link in an answer (a Markdown link, or a bare URL shown as `owner/repo#n`) is a real link (new tab), like other web links; a file link opens the file view (above), and any other link shows only its label. A mouse resting on it for 350 ms, or keyboard focus, opens a card: title, Open / Draft / Merged / Closed, checks ("1 of 3 checks failed: lint"), review decision, +/− lines, author, "updated 3 h ago". A touch just follows the link. The card is fixed on `document.body`, flips above the link when there is no room below, stays inside the window, and closes on leaving link and card, Escape, scroll or resize.
  - `pr.peek { sessionId, url, ref }` answers `{ op: "pr.peek", sessionId, ref, url, pr }` or `{ …, error }` to the asking client only. It carries `sessionId` so the hub routes it to the session's host, which runs `gh pr view <url> --json …` with its own login. yardd accepts only that URL shape (a tab such as `/files`, an anchor or a query is stripped) and rebuilds it from checked parts before gh sees it, so agent text never becomes a flag.
  - yardd keeps each answer per URL for 60 s (a failure for 10 s) and runs one gh for concurrent asks of the same URL; the UI keeps the same per host and URL (`ui/src/prPeek.ts`), so hovering down a list of links costs one gh per PR a minute. An older host answers "unknown op", shown in the card as an older Switchyard.
- Not built: creating a PR, pushing, replying to or resolving threads.

## Connectors (MCP servers)

Both CLIs already load every MCP server their login and config give them: claude.ai connectors (Datadog, Sentry, ... as connected on claude.ai), plugins, `~/.claude.json`, project `.mcp.json`; Codex's `config.toml` and ChatGPT apps. Sessions get them with no work from Switchyard. What was missing is seeing which ones work and signing them in, since a server that needs sign-in just fails inside a turn. `crates/yardd/src/mcp.rs`, UI `ui/src/ConnectorsPage.tsx`.

- **The page** (`#/connectors`, plug row in the sidebar) inspects one selected host and engine account on demand. Connected is the default; unused entries stay under Available, and failures under Not working. All shows every status. Search matches service names and sources, with at most eight rows per page. Rows show name, source, state, tool count and widgets. A sign-in stays visible through completion until dismissed, and adding a connector reveals its sign-in even outside the current filter.
- **Claude:** yardd keeps one `yard-claude connectors <home>` per account (the account's `CLAUDE_CONFIG_DIR`), started on demand and stopped after 10 idle minutes. It runs Claude Code with a prompt stream that never yields, so no turn runs and no usage is spent (checked on claude 2.1.325: all servers settle in about 4 s). It answers `mcpServerStatus()`; Connect is the SDK's `mcpAuthenticate` (not in the typed API, but in `core.mjs` and served by the CLI as `mcp_authenticate`): a claude.ai connector gives a claude.ai page and no callback, so the process re-dials it every 4 s until it reads `connected`; any other OAuth server redirects to `http://localhost:<port>/callback` on the host, which the CLI serves. From a browser on another machine that page fails to load, so its address is pasted back and goes to `mcpSubmitOAuthCallbackUrl`. Project `.mcp.json` servers are per folder, so the page (run in the home folder) does not list them.
- **Codex:** the account's app-server: `mcpServerStatus/list` (`authStatus: notLoggedIn` reads as needs sign-in), `app/list` for ChatGPT apps (Connect links to the app's page), `mcpServer/oauth/login`, and `mcpServer/oauthLogin/completed`. Codex redirects to `http://127.0.0.1:<port>/callback` and has no call to hand it an address, so yardd makes that GET itself on the host. Only a loopback `http://` address with a port is accepted.
- **After a sign-in** yardd broadcasts `mcp.auth done`, lists again, and sends `mcp_reconnect` to every running Claude sidecar of that account, so the server works in open sessions without a restart. A Claude login of the account drops its connectors process (it holds the old login's list); removing the account does too.
- **Adding one:** "Add connector" on an account's card offers a catalog of remote OAuth servers it does not have yet (Linear first, then Sentry, Datadog US1, PostHog, Notion, Atlassian, Vercel; each answered 401 with OAuth resource metadata on 2026-11-16) or any `https` address with a name, and opens the new server's sign-in right after. Claude: `claude mcp add --transport http --scope user` with the account's `CLAUDE_CONFIG_DIR`; Codex: the app-server's `config/value/write` on `mcp_servers.<name>` and `config/mcpServer/reload` (`codex mcp add` goes on to wait for a browser sign-in, so it is not used). Remove does the reverse, for servers in the account's own config. yardd checks the name (a config key) and the address (plain `https`) before anything runs. After a Claude change the connectors process restarts and idle sessions get a fresh sidecar with their next message; a running turn keeps its servers. GitHub's MCP endpoint wants a token, not OAuth, so it is not in the catalog (`gh` covers PRs).
- **Accounts:** an added Claude account is its own `CLAUDE_CONFIG_DIR`: it gets its own claude.ai org's connectors with its login, but not Default's plugins or `~/.claude.json` servers. The page says so, and Add connector puts what it needs in its own config. Copying Default's whole setup is not built.
- yardd never sees a token: both CLIs run their own OAuth and keep the tokens where they always do.
- **Scoped to agents and automations.** Agents and automations carry `mcp: Option<Vec<String>>`, set by the owner only (an agent's `update_setup` cannot touch it): `Some(list)` is exactly those servers, by the name the Connectors page lists (`claude.ai Metabase` for a claude.ai connector); `None` (a record saved before the field, or pushed by a yardd that drops it) is every server the account has except `scope::DENIED_BY_DEFAULT` (`metabase`, `claude.ai Metabase`). New agents and automations start with `Some([])`. Firefighter (`g1a212f7695f8`) gets an explicit list with Metabase when agents load or a copy arrives, and without one it keeps every server. A change to an agent's list releases its conversation's engine once idle, so the next turn resumes with the new list. Sessions that are neither an agent's nor an automation's are not scoped. `crates/yardd/src/scope.rs`.
  - **Codex:** `thread/start` and `thread/resume` read `config/read` and pass `mcp_servers.<name>` overrides for every configured server: its transport fields (nulls dropped, as for restricted threads) with `enabled` false when the scope leaves it out. A server the owner disabled in `config.toml` stays disabled. Switchyard's own `mcp_servers.switchyard` is added after. Plugins' servers and ChatGPT apps are not scoped.
  - **Claude:** the sidecar's `start` carries `mcp: { only } | { except }`, and the sidecar passes it as Claude Code policy: the SDK's `managedSettings` (policy settings from the parent process) with `allowedMcpServers` or `deniedMcpServers` by `serverName`. Claude Code applies that to every source it loads servers from (user, project and local config, plugins, `mcpServers`, claude.ai connectors); a server left out never starts. Switchyard's own server is always on the allowlist. With no claude.ai connector on an `only` list, `ENABLE_CLAUDEAI_MCP_SERVERS=false` also keeps Claude Code from fetching the account's connectors at all. Checked against claude 2.1.327 with no prompt (`YARD_TEST_REAL_CLAUDE=1 bun test src/scope.test.ts`): config servers left out never start. A machine whose administrator installed managed settings drops parent policy by default; then the scope does not apply. Alternatives not taken: `strictMcpConfig` with explicit configs needs each allowed server's config, which only a process that connects to every server can read; `toggleMcpServer` writes the choice into the user's settings.
  - **UI:** a Connectors list (one checkbox per server, from `mcp.status`) in the agent form and the automation editor. A record without a list shows the default ticked and says so; a pick saves an explicit list. ACP engines are not scoped and the picker says so.

## Widgets (MCP Apps)

An MCP tool can declare a widget, `_meta.ui.resourceUri` (`ui://...`, MCP Apps, SEP-1865, protocol `2026-01-26`): an HTML page that draws the result, such as a chart. Claude's desktop app shows these; so does the thread now. `crates/yardd/src/widgets.rs`, `sidecar/yard-claude/src/apps.ts`, `ui/src/widget.ts`, `ui/src/WidgetFrame.tsx`.

- **Claude:** the sidecar maps `mcp__<server>__<tool>` names to `mcpServerStatus()` entries (tool `_meta` is there, `mcp_tool_ui_meta_v1`). When such a call returns, it reads the HTML with `readMcpResource` (cached per server and URI) and sends `tool_completed` with `app`: the HTML, the resource's `_meta.ui` (CSP, permissions, prefersBorder), the input, and the server's own result, which Claude Code passes on as the SDK message's `tool_use_result` (`{ content, structuredContent, _meta }`, checked in the CLI source).
- **Codex:** a completed `mcpToolCall` with `mcpAppUi.resourceUri` (or the older `mcpAppResourceUri`): yardd reads it with `mcpServer/resource/read` (with `originCallId`) before emitting `tool_completed`. Its result is the item's `result`.
- **Storage:** `App::emit` moves `app`'s HTML and data to `<data dir>/widgets/<id>.json` (8 MB cap; over it the call shows as a plain call) and logs `app: { id, server, tool }`. `widget.get` serves a record only to a session whose log names it.
- **Layout:** a widget row follows its call while the turn runs, and when the turn folds it stays in view between "Worked for" and the answer.
- **The frame:** `sandbox="allow-scripts"` and no `allow-same-origin`, so the widget has an opaque origin: it cannot read the page, its storage or cookies, and yardd's socket refuses `Origin: null`. Its document is our doctype and a CSP `<meta>` first, then its HTML; the parser keeps the policy first whatever the widget writes. The CSP allows inline code and only the origins the server declared (`connectDomains`, `resourceDomains`, `frameDomains`), each checked to be a bare `https`/`wss` origin. Mounted only when near the screen.
- **The bridge:** JSON-RPC over `postMessage`, only from the frame's own window. `ui/initialize` (host info, light theme from styles.css tokens, container width), then `tool-input` and `tool-result`. `size-changed` sets the height (48 to 720 px). `ui/open-link` opens http(s) only, in a new tab. `tools/call` goes to yardd as `widget.call`, one at a time per widget, only to the widget's own server and only tools offered to widgets (`visibility` with `app`, or none). Claude runs it as the SDK's `mcp_call` control request with `result_not_for_model`, and `check_permissions` unless the tool is read-only, so it asks in the thread like the model's own call; the session's sidecar is started (resuming, no turn) if it had none. Codex has no such check (`mcpServer/tool/call`), so only tools marked read-only run there. `ui/message`, `ui/update-model-context`, `resources/read` and display modes other than inline are refused.
- Checked: the fake engines and `verify/fakes/mcp-chart` end to end (tests, feature map `connectors`, clicks inside the frame with Playwright); against the real CLIs without a turn: Claude's status with tool `_meta`, `readMcpResource`, `mcp_call` with the permission check, and Codex's status, resource read and sign-in link. Not checked: a real turn of either engine calling a widget tool (Codex was logged out on the server; Claude turns are not spent on checks), and the desktop app's WKWebView.

## Policy and cost

- Anthropic's docs say products built on the Agent SDK should use API keys and must not offer claude.ai login. A personal tool that runs my own logged-in CLI is not addressed. Keep it personal; ask Anthropic before any public release.
- The in-app Claude login (Engine logins) runs the CLI's own `claude auth login`, but it is still a claude.ai login offered from Switchyard's UI. Fine for my own hosts; it is the first thing to settle with Anthropic, or to switch to API keys, before others are pointed at it.
- Unconfirmed: whether Agent SDK usage on a subscription is metered separately from interactive use. Check the Usage page.
- The SDK loads my full Claude Code setup, including every MCP tool (see Connectors). A two-turn test cost $0.24 at API rates. Offer a trimmed profile.

## Status

Built and checked in the browser on 2026-11-13:

- yardd with both adapters, the event log, host stats and a git diff for the Changes pane.
- UI: new session, session thread, model and effort picker, approvals, Changes pane, hosts.
- A Codex session fixed a file and ran a check, with command and edit approvals answered in the UI.
- A Claude session (Haiku) did the same through the sidecar.
- Stop interrupts a Codex turn. A follow-up turn on a different Codex model and effort works in the same thread.
- The WebSocket refuses a foreign browser origin (403) and accepts its own (101).

- A session started with "New worktree" ran on its own branch under `~/.yard/wt`, and its edit stayed out of the original checkout. Asking for a branch that exists is refused with no session made.
- The terminal ran commands in the worktree, fit the pane, and showed its earlier output again after a page reload.
- The desktop app starts yardd, loads the UI, and leaves yardd running when it quits.

- Persistence: a session survived a restart with its history, and a follow-up after the restart used the earlier conversation (Codex).
- PR pane: rendered a real open PR from a public repo, with checks and review threads.
- Hand-off: Codex received a hand-off brief and answered from the earlier work. The picker's confirmation was checked in the UI.
- Resource guard: per-session CPU and memory in the Hosts table and the queue with "Start anyway".
- Worktrees: the "From" select, and "Delete and remove worktree" removing a clean worktree and its merged branch.
- Terminal tabs: a second terminal running its own command, and a selection added to the reply box.
- Self-contained app: a copy of the release bundle outside the repo started its own bundled yardd on a test port and ran a Codex turn.
- Remote hosts: a second local yardd added as a `direct` host. A session created on it from the UI ran there, with its terminal and Changes pane working through the hub, and came back after the remote restarted.

Not checked:

- Any real OpenCode or Grok turn: no model was asked anything. Checked against the real CLIs (2026-11-16): `opencode acp`'s `initialize`, `session/new`, `set_config_option` for model, effort and mode, and `session/resume`; `opencode models --verbose`; the whole OpenCode API-key login through a test yardd, with a made-up key in a throwaway HOME; `grok agent stdio`'s `initialize` models and its "Authentication required" answer; `grok models` logged out; the start of `grok login --device-auth` (cancelled). Not seen: Grok's session shapes when logged in (its `configOptions`, modes, tool calls), OpenCode's tool-call and permission updates outside its source, and an OAuth or device sign-in to a real provider.

- A real SSH tunnel. This Mac has no SSH server to tunnel to, so only the failure and retry paths ran.
- Anything that needs a Claude turn in the newer features: Claude resume after a restart, hand-off into Claude, and Claude's per-session usage and cap. Claude weekly usage was nearly exhausted.
- Stop and a mid-session model switch on Claude. Both passed in the script spike.
- Attachments with a real engine. Paste, drop, the paperclip, the size cap, thumbnails after a reload and a remote session were driven in headless Chromium (2026-11-14) against the fake Codex; no real Codex or Claude turn has seen an image yet, and drops were not tried in the desktop window.

Codex sessions run with `approvalPolicy: on-request` in a workspace-write sandbox. `untrusted` asked before every `cat`.

The app is self-contained. `app/build.sh` compiles the sidecar with `bun build --compile`, builds a release yardd, and bundles both with `ui/dist` under `Switchyard.app/Contents/Resources/yard` (78 MB, most of it the sidecar's Bun runtime).

- The shell starts that yardd with `YARD_ROOT` set to that folder and `YARD_SIDECAR` set to the compiled sidecar, so neither Bun nor the repo is needed at run time. The sidecar uses my installed `claude`.
- It asks the login shell for `PATH`, because an app opened from Finder cannot see `codex` or `claude` otherwise.
- `/health` returns `yardd <version> <build id>`. If a yardd with another build id is already running, the shell leaves it alone and says so in the title, because it may hold live sessions.
- With no bundled files (`cargo run`), the shell falls back to the repo builds.
- Unsigned, no auto-update.

Next:

1. Try a real box over Tailscale SSH, and the Claude paths listed above.
2. Creating a PR and replying to review threads from the pane.
3. Claude: effort changes mid-session, and showing Claude's questions as question cards.

## Risks

- `codex app-server` is experimental and can change between CLI versions.
- ACP's `session/resume` and `configOptions` are recent; Grok's ACP has `x.ai/` extensions and changes with its near-daily releases.
- Two agents in one worktree will fight. Enforce one writer.
- If yardd is killed with SIGKILL, an SSH tunnel it started is left running. A normal stop kills it.

## Saved launch defaults

Settings offers defaults for New sessions, using the shared host/model/effort
pickers. `launchDefaults.ts` stores an explicit `{ host, choice }` preference in
`yard.defaults.sessions` in this browser. One-off selections and
`rememberSession` never overwrite it. New-session models are validated against
the selected host, retaining the intended default while catalogs load;
configured offline hosts remain selected and cannot silently run on another
host. Removed hosts/models fall back to available choices without rewriting the
saved preference. Clearing the default restores the prior last-used behavior.
Existing sessions are unaffected. Agents keep their own model and effort in
yardd (see Agents); Switchy's browser default (`yard.defaults.switchy`) is no
longer read.

## Agents

Named agents, after Orca's Bots: each has a display name ("Custom Domains
Triage", spaces and all), a handle it is mentioned by (`@Custom-Domains-Triage`,
made from the name), a colour, what it is for, how it runs, and two notes the user
writes for it. Each agent has a main Switchyard conversation and separate
sessions for its Slack places. Agent conversations stay together under
Agents in the sidebar. There is no built-in agent: Scout, the read-only host agent that
had replaced Switchy, was removed on 2026-11-24 at the owner's request, with
its evidence collector (`host_context.rs`, `context.collect`); a Scout record
and its conversation left by an older yardd are deleted at start
(`agents::retire`). New agents are **set up by talking to them** (below).
`crates/yardd/src/agents.rs`; UI `ui/src/AgentsPage.tsx` (page, a new
agent's conversation, requests, form), `ui/src/agents.ts` (faces, mentions,
card words, attention), `ui/src/SlackSetup.tsx` (an agent's own Slack app),
the composer and session view in `App.tsx`.

- **The record** is saved in `agents.json` in the data dir (an unreadable file
  is kept as `.json.bad`): `name` (any printable text, trimmed with runs of
  spaces made one, 1 to 40 characters, no control, zero-width or direction
  characters; unique ignoring case; `agents::clean_name`), `handle` (see
  Names and handles), `color` (one of eight, a token
  each in `styles.css`), `about` (one line), `engine`, `model` (empty: the
  host's default for the engine), `effort`, `account`, `cwd` (empty: its own
  folder, `<data dir>/agents/<id>/`), `permission` (Ask first unless the user
  picks another; Full access only when picked), `instructions` (who it is:
  Orca's SOUL.md), `memory` (what to keep in mind: MEMORY.md), `chat` (its
  conversation's session id, made with its first message; `<hostId>~<id>`
  as a hub lists a host's agent), `host` (`local` on the yardd that owns it;
  a host's id in a hub's list, and in a record an older hub kept for a host),
  `previous` (conversations it left on this host when it moved away from it
  and back) and `seed` (what its next conversation starts from after a move;
  never sent to the UI), `setup` (made with New agent, not yet named by
  itself) and `request` (what it asked for and the owner has not answered:
  `cwd`, `permission`, `slack`). Fields this build does not know (a field a newer yardd added, say) are kept as they are through every load, save and hand-over (`#[serde(flatten)] extra`); a save from a window cannot add one. **The record lives on the host the agent
  runs on**, with its own Slack app (since 2026-11-25; owner: "Agents should
  be shared by the env. I want to access them anywhere."): see Every host
  owns its agents, below. The UI knows a remote session as an agent's by id
  (`chat` or `previous`, prefixed), not by the remote's `agent` tag; a local
  session tagged with an agent the window does not know is an ordinary one.
- **Names and handles.** The owner asked for "display names with spaces,
  dashes and whatever" (2026-11-24); a mention needs one word. So the name is
  for reading and the handle for typing: yardd makes the handle, never the
  client (`agent.save` ignores one sent): the name's letters and digits as
  typed, one `-` for whatever stands between them, starting with a letter,
  `agent` when there is none ("TEST Custom Domains!" is `TEST-Custom-Domains`;
  `handle_base`). It is unique ignoring case, `-2`, `-3`… on a collision, and
  kept while it still fits a new name: the same words in another case take
  the name's case, a numbered one stays numbered; other words, a new handle
  (`handle_for`; the UI's twin `handleFor` says it in the form before saving).
  Case is kept rather than lowered: `@Reviewer` stays what it always was, and
  a mention matches ignoring case anyway. Agents from before handles get
  their name as handle at start (those names were one word), so every
  mention already written keeps working; a hosted copy from an older hub
  derives its own. The engine hears both: "You are Custom Domains Triage
  (@Custom-Domains-Triage), …", and other agents as `@handle (Name)`.
- **Ops:** `agent.list`, `agent.save { agent }` (an `id` edits; only the fields
  sent change; answers `agent.saved`), `agent.delete { id }` (deletes its
  conversation and its own Slack app too, after a confirm in the UI),
  `agent.ask { agentId, text, attachments? }`, `agent.new { engine, model,
  effort }` (an agent called `New-agent`, `New-agent-2`… with `setup`, on the
  last model used; an earlier one nobody has written to is answered instead
  of a second), `agent.request { id, part, allow }` (the owner's click on
  what it asked for). Every change broadcasts `agents`; the snapshot carries
  the list (`agents::shown`: this yardd's own, then each host's). Between
  yardds: `agent.deliver { agentId, text, origin }` (a hub's mention, agent
  message or Slack message for a host's own agent), `agent.adopt { agent,
  slack }`, `agent.export { id }` and `agent.release { id }` (hand-overs,
  below), and the older hosted-copy ops `agent.hosted.*`, kept so an older
  hub still runs its agents on a newer host.
- **Set up by talking to it.** New agent opens the new agent's conversation,
  no form: its face large, "Hi! What should I do?", ideas to start from, Add
  it to Slack and Discard; nothing runs until the owner writes. While
  `setup`, every turn's input carries `<first_run>` (after Orca's first-run
  rules): ask one question if it cannot tell what it is for; otherwise name
  itself, write its about and instructions, pick a face and colour, ask for a
  folder, more access or Slack when the work needs them, do one small piece
  of the work if it can, and say in one line what it set up. The preamble
  also gives its current setup ("Your setup now: …", with the engine's
  models). It sets itself up with its own tool **`update_setup`** (see
  Agents' own tools): name, about, instructions, memory, face and colour are
  saved at once (through `agent.save`'s checks; a refusal is the tool's error
  with the reason); engine, model and effort wait for the turn to end (the
  conversation cannot change engine mid-turn; `Agents::later`, applied by
  `run_turn_ends`, then a fresh context as for an Edit), with the note
  "<Name> now runs on <model>"; a lower or equal permission and "its own
  folder" at once; a folder of the user's, more access and `connect:
  "slack"` become `request`. Unknown fields are ignored and named. Its own
  changes are no notes change. A note in the thread says what changed
  ("New-agent is set up as @Deployer: name, what it is for, …", or "… as
  Custom Domains (@Custom-Domains): …"); a name of its own ends `setup`,
  from the tool, from Edit or renamed in place. Each saved change broadcasts
  `agents`, so the header follows at once. Requests show as cards
  above the reply box ("Deployer asks for Full access: it would run anything
  without asking", Give Full access / Not now; Allow for a folder; Add to
  Slack opens the Slack dialog); the answer is a note and reaches the agent
  with its next message. Without the tools (no loopback bind) the first-run
  rules ask the owner to use Edit. The Edit form stays for changing things
  by hand.
- **The main conversation.** The agent page, Switchyard mentions and
  `message_agent` send here (`sessions::send_message`). Watched-channel
  batches arrive here too. Slack DMs and thread requests use dedicated place
  sessions. The first message makes it (a `session.create` in the agent's
  folder tagged `Meta.agent`); yardd answers `created { sessionId, ref }` as
  soon as it is known. In the agent's own folder it does not hold the folder;
  in a folder the user chose it runs in place, one writer at a time.
- **Long life.** The engine thread resumes after a restart like any session's.
  When it cannot continue, the next turn starts a fresh engine thread in the
  same conversation: the preamble again, then the latest exchanges as records
  (`<earlier_in_this_conversation>`, newest that fit 8,000 bytes), then the
  message; the thread shows a note, "Started a fresh context: <why>". Why:
  the engine reported a full context (an `error` mentioning the context
  window, prompt length or compaction sets `Meta.fresh_context`), the engine
  could not continue the thread (a send that fails, such as a lost thread,
  is retried once this way), or the agent's engine, model, account ("its
  account changed": a thread lives on the old login's app-server or config
  dir) or folder changed in Edit (`agent.save` moves the conversation's metadata with it and
  refuses while a turn runs). Changed instructions, memory, name or
  description keep the thread and reach the agent with its next message
  (`<notes_changed>`, `Meta.notes_changed`); effort and permission apply from
  the next turn. The user sees one continuous thread.
- **Every host owns its agents.** An agent belongs to the yardd it runs on:
  its record in that yardd's `agents.json`, its own Slack app in its
  `slack.json`, its conversation there, and its tools acting there. So an
  agent on the always-on server answers from Slack while every Mac sleeps,
  and any Mac that adds the server as a host sees, messages, edits and
  deletes it. A record's `host` is `local` on its owner.
  - **Listed by every hub.** The hub's own connection to each host
    (`remote::monitor`) keeps what the host says about its agents: its
    snapshot's `agents` and `slack`, then each broadcast (entries that are
    the host's own remotes', ids with `~`, are left out). The hub's `agents`
    list is its own agents, then each host's in the order hosts were added,
    with ids, `chat` and `previous` prefixed `<hostId>~` and `host` the host's
    id, as for sessions (`agents::shown`, `remote::prefixed_agent`); its
    Slack view adds the hosts' agents' apps under the same ids and, per host,
    its workspaces and addresses (`slack::view`, `hosts`), so the Add to
    Slack dialog makes an agent's app with its own host's configuration
    token. A host that goes offline keeps its agents listed, offline, from
    the last thing it said (in memory, like its last snapshot). The UI needs
    nothing else: such an agent reads as an agent on a remote host always
    did (card, header, sidebar row and Cmd-K name its host).
  - **Routed to it.** Every op on such an agent goes to its host through the
    asking window's own pipe, the prefix removed (`remote::route`,
    `outgoing`); a new agent on a remote host is made there (`agent.save`
    without an `id`, or `agent.new`, with `host`). While the host is
    offline the hub answers instead, as before: "Far runs on box, which is
    offline. Your message was not sent.", "box is offline, so Far can't be
    changed now.", "Delete Far once box is back." (`agents::refusal`); files
    uploaded to the hub are refused for an agent on another host ("These
    files are on This Mac, not on box").
  - **Mentions across hosts.** A mention the hub resolves (a leading
    `@handle` in any window's box becomes `agent.ask` with the prefixed id;
    another agent's `message_agent`; a message to the shared Slack bot)
    reaches the agent where it lives: `agents::deliver` sends a prefixed id
    to its host as `agent.deliver` with the message's origin and the hub's
    id (`origin.hub`). The host keeps only the kinds and fields yardd makes
    (`origins::from_hub`, `hub` included). When its turn ends, origins whose
    `hub` is another yardd are answered on that hub (`agent.hub.owed` over
    the hub's connection, kept up to 200 while it is away;
    `origins::reply`), which runs its own reply hook with the host's agent
    as `<hostId>~<id>`: another agent hears "Reply from @Far", Slack gets
    the post. An agent on a host messages the agents on that host; one on
    the server cannot reach an agent that lives on a Mac (the server cannot
    dial the Mac). **Handles are unique per host.** When two hosts' agents
    share one, the agent listed first (this yardd's, then hosts in order)
    keeps it and the other is listed with its host's name after it
    (`@Reviewer-box`), so a mention, `message_agent` and Slack always name
    one agent; a name two agents share reaches the first in Slack. Names
    are unique per host too (the form checks the agent's own host).
  - **Older hosts.** A host whose snapshot lacks `ownAgents` cannot own
    agents: making one there, or moving one there, is refused with "box
    runs an older Switchyard … Use Update Switchyard on its card in Hosts".
    Records an older hub kept for it stay on the hub and run there as
    hidden copies (the hosted path, below); the agent's page says to update
    the host (`keptNote`).
- **Hand-overs.** Every hand-over of an agent from one yardd to another is
  one named `transfer`, and every step names it: `agent.export` (the old
  side marks the record `leaving` it), `agent.adopt` (the new side keeps it
  as the record's `transfer`), `agent.release` (the old side lets go only of
  the copy that is `leaving` by that transfer), `agent.settled` (the new
  side's Slack app is its own) and `agent.abandon` (the old side stays after
  all). So every step is safe to repeat, and a step left over from an
  earlier hand-over does nothing to a copy that came back since. Steps
  that do not happen at once are kept in `agents-pending.json` and tried
  again every 15 s (`run_pending`, `keep_settling`).
  - **Checked before written.** The new side checks everything before it
    writes anything, so a refusal leaves no trace, not even a backup:
    refused for now while the agent's conversation has a turn going (an
    answer it owes goes back where it was asked), or while it is still
    letting go of an earlier copy; refused for good for a hidden copy
    another hub put there, or an agent of its own from another hand-over.
    Each refusal carries a `code` beside its message (`busy`, `letting_go`,
    `not_yours`, `deleted`; `agents::Refusal`, sent by `ws::error_reply`,
    read back by `remote::request`), and the hub acts on the code only:
    the message names the agent, so its wording could misfire. A refusal
    without a code is a plain failure, tried again later.
    The same transfer sent again changes nothing. Each side backs up
    `agents.json` and `slack.json` (`.bak-<time>`, the newest ten kept;
    `slack.json`'s hold tokens) right before its first change and saves
    with checked writes: a failed backup or save stops the step with an
    error and leaves its lists as they were.
  - **Release waits for a turn.** The old side refuses to let go while the
    agent's conversation has a turn going (a Slack message to its app, which
    still runs there until the release, say) and the release is tried again.
  - **Slack while it settles.** The app is never deleted in Slack when its
    agent is deleted on the side that is `leaving`, nor on the side that took
    it until `agent.settled` (`handOver`): another yardd may run it. Both
    sides may hold its Socket Mode connection between the adopt and the
    release.
  - **Taken whole.** `agent.adopt` takes the record and its app or neither:
    an app it cannot keep puts the record back as it was, so a record found
    on a host by its transfer always has its app.
  - **A record that is leaving holds still.** While a record is `leaving` it
    takes no edit, memory write, delete or other move ("… is moving to
    another host"): the change would be lost with the copy that goes, or
    would leave a copy the user thought gone. A record leaving for a host
    since removed from this yardd is exempt (`moving_now`). A try of
    `migrate` (below) marks the record only while it waits for the host's
    answer, and the owner never sees those tries: an edit, delete or memory
    write that comes meanwhile waits for that answer instead of being
    refused (a refusal leaves the record the hub's), and no try starts while
    an edit is under way (`Agents::edit`, `Agents::try_now`). The try
    holds the record only until the host's answer is in and the record
    says it (`offer` lets go here at once when the host took it, unless a
    turn holds the let-go); the steps owed after it (`agent.settled`, a
    forgotten app's deletion in Slack) run after, so an edit waits at most
    one request. An edit that waited for a try the host accepted, or comes
    from a page that still lists the agent here, says where it went and
    that nothing changed ("On-call moved to box while you were editing it,
    so your change was not saved. Edit it there."; `Agents::moved`, kept
    in memory): the save carries this hub's view of the agent's folder,
    account and host, so it is not sent on to the new host. An
    `agent.export` by another transfer waits until the first is released or
    abandoned.
  - **A deleted agent stays deleted.** A yardd that deletes an agent
    remembers the transfer that brought it (`agents-ended.json`, the newest
    500) and refuses it if a hub sends it again ("… was deleted on box"). The
    hub then drops its own record of that hand-over and the steps it still
    owed, and deletes the app in Slack, which nobody runs any more
    (`agents::forget`).
  - **Listed once meanwhile.** A record a hub keeps for a host that lists
    the agent (not leaving) is that host's copy, shown there. Otherwise the
    hub's record is shown, and a host's copy that still owes its release is
    hidden while the agent is listed elsewhere. An agent mid-move is never
    missing from the list, nor listed twice.
- **Handed over to its host (migration).** A hub from before this kept every
  agent's record itself, even one that ran on the server (a hidden copy
  there, `hosted: true`). When a hub that holds records with `host` set to a
  host sees that host connect and say `ownAgents`, it hands each over
  (`agents::migrate`, one at a time). The transfer is named from the hub,
  the agent and the record's last change (`kept_transfer`), and the record
  is marked `leaving` by it while it is sent (backed up once per hand-over,
  not per try), so every try is the same one and no edit lands on a record
  the host may own already. A refusal from the host (a turn going, still
  letting go, someone else's) clears the mark: the host surely did not take
  it, so the record is the hub's meanwhile, and a turn its hidden copy runs
  saves its memory there. A lost answer, or a step after the adopt that
  failed, keeps it. It sends `agent.adopt` with the whole record (fields this
  build does not know included, `Agent::extra`; `chat` and `previous` the
  host's own ids; others dropped), its own id (`hub`) and its Slack app
  (`slack::export_app`: the whole `AgentApp` record, fields this build does
  not know included; its workspace's configuration token; its icon; not
  its Slack threads, places or approvals, which are the hub's). The host
  turns its hidden copy into its own (`hosted` and `hub` cleared; its
  conversation the copy's or the record's), keeping instructions, memory,
  look, request and setup as sent; a folder or account it lacks falls back
  to the agent's own folder and Default rather than refusing; a name or
  handle another of its agents has gets a number. Its app connects from
  there. Only whoever installed the app joins the host's allowlist, as at an
  install there: the Mac's other allowed people stay on the Mac. Only after
  the host answers `agent.adopted` does the hub let go (its record removed,
  the app's connection closed, its Slack threads and places here dropped,
  never deleting the app in Slack, the agent's old conversations here
  ordinary sessions), then send `agent.settled`.
  It tries on every connection, and every 15 s after a refusal for now,
  doubling up to 5 minutes. A refusal for good is not tried again until the
  next connection, and the agent's page says why (`handOverError`). While
  the host owns the agent and the hub has yet to let go, a message to it
  goes to the host's own agent of that hand-over. Nothing deletes a
  conversation. An app on HTTP events keeps its events address, which
  points at the hub: point it at the host (not built). A configuration
  token copied to the host works once on whichever yardd rotates it first;
  the other then asks for a new one in the dialog. The host rotates it at
  its next start if its wanted manifest (its own redirect addresses)
  differs from the kept one (`sync_app`), which also rewrites the app's
  install redirects to the host's: harmless for an installed app, and the
  Mac's copy of the token then needs a new one.
- **Moving between hosts** (the host picker in Edit or in the box) is a
  hand-over the hub orchestrates (`agents::transfer`): the old host gives
  the agent's record, the latest exchanges of its conversation and its
  Slack app (`agent.export`, refused while a turn runs); the new host takes
  them with the save's other changes and the recap as `seed`, so its next
  conversation starts with the preamble, the latest exchanges and the note
  "Started a fresh context: it moved to <host>." (`agent.adopt`); then the
  old host lets go (`agent.release`: its conversations there ordinary
  sessions, readable) and the new host hears its app is its own. The agent
  keeps its id; the hub names it `<hostId>~<id>` on a remote, so its page
  follows it (`relocated`). Refused while either host is offline or cannot
  own agents. A record leaving this yardd is first saved here pointing at
  its new host, so a lost answer is finished like a migration, by the same
  transfer. A move from another host: an export whose answer is lost gets
  `agent.abandon` (the old host may have marked it); an adopt whose answer
  is lost is looked for on the new host (`took`: listed there by that
  transfer) and finished if it is there, abandoned if not; when the new
  host cannot be asked the abandon is `unsure`, and the old host's app
  stays undeletable in Slack (`slack::protect`), since the new host may run
  it too. The release and `agent.settled` are owed as soon as the new host
  answers, so nothing after that can leave two owners.
  A record an older hub kept is moved the old way to this yardd or to an
  older host (`move_host`), and handed over like this to a host that owns
  agents. The form says what will happen before saving.
- **Only yardds hand over.** `agent.export`, `agent.adopt`,
  `agent.release`, `agent.settled`, `agent.abandon`, `agent.deliver` and
  the hosted-copy ops (`agent.hosted.*`) carry Slack tokens or speak for a
  hub: a window (a client that sends `Origin`) is refused them before
  routing, so no id in the op sends one on to a host either
  (`agents::window_may_send`).
- **The hosted path (older hubs).** A hub from before this hands a host a
  copy of the record (`agent.hosted.save`, with `hosted: true`; kept in the
  host's `agents.json`, never listed, mentioned or edited there), so the
  host's own `agents::prepare` adds the preamble, changed notes and a fresh
  context exactly as for a local agent; a message goes to the host as
  `agent.hosted.ask` over a daemon-owned connection (`remote::request`),
  answered with `created`; moving away asks `agent.hosted.leave` for the
  recap; deleting sends `agent.hosted.delete`. A newer host keeps all of
  these, so an older hub's agents keep running on it, and a newer hub uses
  them for records it still keeps for an older host. Offline, these say so
  as above.
- **The preamble:** the first turn hands the engine "You are <name>, an agent
  the user set up in Switchyard…", what it is for, its instructions and
  memory, then "The user's message:" and the text (`agents::prepare`, called
  from `sessions::start_turn`). The thread and the event log keep only what
  the user typed.
- **@mentions:** the composer (New session, any local session's reply box, an
  agent's conversation) offers the agents when `@` starts a word, filtered by
  what follows (a handle, name or word of the name that starts with it
  first, then any that contain it); each row shows the name, the `@handle`
  when it reads differently, and what it is for; arrows, Enter or Tab pick,
  and the handle is typed. Escape closes only the list. A message that
  starts with an agent's `@handle` goes to that agent's conversation instead
  (the mention dropped), and the
  box says so above the text ("Goes to Reviewer", "… not this session"). In
  the agent's own conversation a leading mention is just text.
- **In the sidebar as direct messages.** The session list's last section, "Agents", lists every agent in the session rows' style: its face where a session shows its harness, the same status badge (an agent asking for something reads as an approval), its name; a row opens its conversation and is highlighted there (owner request, after Slack's DMs). Their conversations themselves stay out of the session sections.
- **Not in the sidebar.** Agent conversations are left out of the sidebar's
  list, its groups, Pinned and Archived, and of Cmd-K's session rows
  (`isAgentConversation`). Their attention shows on the **Agents** nav row
  instead: a dot for the most pressing state of any agent's conversation
  (needs approval, working, an unseen failure, an unseen answer; seen as in
  the sidebar, `yard.seen`), and on each agent's card. Cmd-K lists the agents
  themselves (group Agents, found by name or `@handle`), each opening its
  conversation. An OS
  notification about an agent's turn opens its conversation.
- **UI.** Agents (`#/agents`, an at-sign row in the sidebar; `#/assistant`
  redirects; Cmd-K "switchy" finds it) is a grid of cards like Automations,
  rows sharing their height so status lines and buttons line up: face, name
  and `@handle` ("Setting up" while new), what it is for, how it runs, "In
  Slack" for one with its own app, a status line from the conversation
  ("Working…", "Needs approval", "Asks you for something", "Answered 5 min
  ago · <start of the last answer>", "Failed …", "No messages yet"), then
  Message and Edit; the face and name open the conversation too. New agent
  (and the dashed card when there is none) opens a new agent's conversation;
  `#/agents/new/edit` does the same. An agent's place is `#/agents/<id>`:
  its header (face, name, what it is for, Stop, Add to Slack / Finish Slack /
  In Slack, Edit), its requests and the thread, the reply box saying "Reply
  to <Name>".
- **Changed where it is shown** (owner: "easy to edit without leaving the
  initial chat page"). The face, in the header and large on the start page,
  opens a small popover (`AgentLook`): its face large in the picked colour,
  the eight faces drawn in that colour and the eight colours under them;
  each pick saves at once and shows there before yardd answers (a refusal
  puts it back and says why). The name and what it is for (the header's on a
  wide window, the start page's under its heading on any) become a field
  when clicked (`InPlace`): Enter saves, Escape cancels, leaving the field
  saves a change; a problem (the UI's `nameProblem` first, then yardd's
  refusal) is said under the field, which stays open. Edit keeps the whole
  form.
- **How it runs, from its box** (owner: "allow changing the model, server,
  and etc on the chat box"). The start page's box and the conversation's
  reply box carry New session's pickers for the agent (`AgentRunPickers`):
  model with its engine, the account where the engine has more than one
  there, effort and permission, plus the host (under the start page's box,
  as New session has it; in the reply box's row). On a phone they fold into
  "Agent setup", as New session's do. Each pick is an `agent.save` of that
  field alone, with Edit's rules: an engine, model or account change goes on
  in a fresh context and is refused while a turn runs (said above the box);
  a model of another engine runs on that engine's Default account; Full
  access is only ever the owner's pick. A host picked before the first
  message just moves the agent (its own folder there, the host's default
  model or account where the new host lacks its own: `hostChange`); after
  it, the box first says what a move does (`moveNote`) with Keep it here /
  Move to <host>. The conversation's speed (Codex service tier) stays the
  session's. An earlier conversation it left on another host keeps no box. An agent working in its own folder has no Terminal, Subagents
  or Processes pane; one given a project folder keeps them. Each answer
  carries the agent's face and name.
- **Faces move.** Each agent is a small 3D character (`faces.ts`): a
  shaded ball in its colour (radial gradient lit from the top left, a soft
  specular highlight, a rim light, a contact shadow), eyes with whites,
  pupils and catchlights, eyelids, cheeks and a mouth. The eight face names
  are expressions (`FACES`: smile, grin, wink, calm with heavy lids,
  glasses, bot with an antenna and screen eyes, happy with smiling lower
  lids, curious with a raised brow and a tilt); yardd keeps the name and
  drops one it does not know, and an agent with none shows one picked from
  its id. The colours are `AGENT_RGB`, equal to the `--agent-*` tokens (a
  test checks). Detail follows size: at 14-20 px the head fills the box and
  drops shadow, rim and cheeks; card sizes grow 12 %; 88 px adds sparkles.
  One description (`faceTree`) renders as React elements in `AgentAvatar`
  and as a standalone SVG string (`avatarSvg(agent, px)`, the still pose,
  for an agent's Slack icon): every colour, gradient and clip is inside the
  SVG with ids unique per instance, so serializing the rendered face and
  rasterizing it gives the same picture. CSS moves named groups: it
  breathes, blinks every 5.2 s (the lids slide down), glances aside with
  its features turning a little (a 3D turn) and tilts its head now and then,
  each agent at its own moment (`blinkDelay` from its id); while its
  conversation runs (header, card, the answer being written) it bounces with
  a squash, sways, looks around and talks; reaching for one (card, face
  picker, start page) makes it hop with a squash and stretch while its
  shadow shrinks. The large ones (lg, xl) look at the pointer within 260 px:
  one shared `pointermove` listener, read once per frame, sets `--look-x/y`.
  Everything is off under `prefers-reduced-motion`, and the form's unpicked
  faces stay still. The owner asked for faces "3D and move around like
  grokbot, blink and etc., friendly": xAI's Grok Bot (agent product, 2026)
  gives its Bots simple shapes with big expressive eyes whose motion shows
  their state (calm and slightly curious at rest, "kicks into gear" when
  working); Grok's CLI carries no mascot, so the drawing and motion are ours.
- **The form** (`#/agents/<id>/edit`, existing agents) is one centred column:
  the face large with the name under it, then cards for Look (faces and
  colours on one 8-column grid, 4 on a phone, so each colour sits under a
  face and every swatch is round), Role (what it is for, instructions,
  memory) and Where and how, and a footer that stays in reach with Delete,
  Cancel and Save (both back to the conversation). It reuses New session's
  host, folder, model, account, effort and permission pickers: the folder,
  models and accounts are the picked host's (a new host clears the folder),
  and the account picker shows only when the engine has more than one
  account there. The explanation line and the card's how-it-runs line name
  the host when it is not this one (a cloud before it on the card), and the
  account where the engine has a choice. Deleting asks once more and says the
  conversation (and its Slack app) goes too.
- **Origins** (`origins.rs`). A message into a conversation that the owner
  did not type carries an `origin`, kept on its `user_message` (and on the
  queued message before that) and shown in the thread as a small badge above
  the bubble, the sender's face and words (`originLabel` in agents.ts; React
  text only): `{ kind: "agent", agentId, name, handle, hops }` is "From
  @handle", `{ kind: "agentReply", agentId, name, handle, hops }` is "Reply
  from @handle" (`name` from a yardd before handles),
  `{ kind: "slack", team, channel, channelName, threadTs, ts, user, userName }`
  is "From Gio on Slack · #general" with an aubergine dot. None,
  or `{ kind: "user" }`, is the owner (no badge); a kind the UI does not know
  shows none. Origins are set by yardd only, never read from a client's op.
  The engine reads such a message framed with where it came from
  (`origins::engine_text`: `<message from="@Builder">…</message>`, by handle, and that the
  answer goes back; `<reply from="@Reviewer">…</reply>`; `<slack from="Gio"
  in="#general">…</slack>` with names flattened to one line without quotes or
  angle brackets, so a Slack display name cannot break the frame); the thread shows only
  the text. Queued messages from several origins become one turn but stay
  separate `user_message`s, each with its badge. A message is queued only if
  the turn is still running when it is pushed (`App::queue_while`, one lock
  for both): looked at and pushed apart, an answer that arrived as the turn
  ended waited in an idle conversation until something else came. `Meta.answering` keeps the
  origins of the messages the running turn answers: set when it starts, grown
  by steered messages (before the engine reads them, so a turn that ends at
  once still owes them). When the turn ends, `App::emit` hands them with its
  final answer, or its error, to the **reply hook** (`origins::reply`, run off
  a channel by a task in main): one match on `kind`, where `agent` delivers
  the answer into the sender's conversation as an `agentReply` (which starts
  its turn) and a reply is not answered back. A failed turn sends "<Name>'s
  turn failed before it answered: …", a stopped one "<Name> was stopped before
  it answered"; Stop also takes such messages out of the queue (they never
  land in the owner's reply box) and their senders hear "… stopped before it
  read your message". A yardd restart mid-turn fails the turn, so the sender
  hears that too. The `slack` arm hands all of a turn's Slack origins at once
  to `slack::answer` (one post per Slack thread). Slack origins also hear,
  once per turn, that the turn waits on an approval: `App::emit` owes them an
  `approval` status at the turn's first `approval_request`
  (`origins::wants_approval_note`); an `agent` origin ignores it and waits
  for the answer.
- **Agents' own tools** (`agent_tools.rs`). Every user-made agent gets seven
  tools in its conversation on every engine: `notify_owner { message }`,
  `message_agent { to, text }`, `list_agents`, `remember { text }` (adds a
  line), `set_memory { text }` (replaces it) and `update_setup { name?,
  about?, instructions?, memory?, face?, color?, engine?, model?, effort?,
  cwd?, permission?, connect? }` (see Set up by talking to it; "Updated its
  setup"; `name` is a display name too) and `slack_reply { text, channel?,
  thread_ts? }` (see Post in Slack on its own). `message_agent`'s `to` is the
  other's handle, or its name as it reads; `list_agents` and the tool's
  description list `@handle (Name): what it is for`. Private threads and the
  owner's own sessions never get them.
  - **How they reach the engines.** yardd is also a stdio MCP server:
    `yardd agent-tools`. An agent's conversation hands its engine that server
    with two variables, `YARD_TOOLS_URL` (`http://127.0.0.1:<port>/agent-tools`)
    and `YARD_TOOLS_KEY` (32 random bytes, one per conversation for this
    yardd's life, dropped with the session): Claude through the sidecar's
    `start.tools` (the SDK's `mcpServers.switchyard`, with `allowedTools:
    ["mcp__switchyard"]` so they run unasked); Codex as a thread-local
    `config` override `mcp_servers.switchyard` on `thread/start` and
    `thread/resume` (never written to the user's config); OpenCode, Grok and
    Gemini in `mcpServers` on ACP `session/new`, `session/resume` and
    `session/load`. The server answers `initialize` itself and passes
    `tools/list` and `tools/call` to yardd: `POST /agent-tools`, which answers
    only loopback peers, never a request with an `Origin` header, and only a
    key it made, and acts for the one conversation the key names. One
    implementation for four engines, and the key, not the model, decides who
    is calling. Chosen over Codex's dynamic tools and an SDK in-process server
    (one engine each). Without a loopback bind (`YARD_BIND` lists only other
    addresses) no engine is offered the tools. Our tools never ask for
    approval: Codex's MCP elicitation from `switchyard` is accepted, and an
    ACP permission request for a `switchyard_*` call is allowed; they act only
    on the agent's own conversation and are capped. Each engine shows a call
    as a normal step, labelled by `agent_tools::label` (and its twin in the
    sidecar's `agentTools.ts`): "Asked for you", "Messaged @Reviewer",
    "Remembered", "Updated memory", "Listed agents", "Posted in Slack".
  - **Needs you.** `notify_owner` broadcasts the `notify` op (as `yv notify`
    does) titled "<Name> needs you", with the message and the conversation's
    id, so a click opens the conversation; and sets `Meta.needsYou { message,
    at }`. The card reads "Needs you · <message>" with the amber "!" mark and
    the Agents row gets a breathing amber dot (after a pending approval, before
    working), until a focused window shows the conversation and sends
    `agent.seen { agentId }`, which clears it for every window. At most 3 per
    agent per hour; the 4th says when it may try again.
  - **Message another agent.** `to` is a name (case and a leading `@`
    ignored); the description of `message_agent` lists the agents there are.
    The message goes into the target's one conversation with origin `agent`
    (made with it if it had none; queued while it works) and the call
    answers at once; delivery that fails comes back as a reply. Hops: one
    more than the longest chain among the messages the sender's turn answers;
    a 4th hop, a message to oneself or an unknown name is refused in the tool
    result. At most 30 messages per agent per hour.
  - **Post in Slack on its own.** An answer reaches Slack only as the final
    answer to a Slack message, so an agent told "tell us here when it's
    deployed" had no way to post later. `slack_reply` posts `text` (Markdown,
    turned into mrkdwn and split like an answer, at most 20,000 bytes) in a
    Slack thread with the bot token of the agent's own app
    (`slack::thread_for`, `slack::post_in`), at any time: from a turn the
    owner started in Switchyard, or long after the Slack message. Safety:
    only threads routed to that agent (the `team/channel/thread_ts` links
    Slack routing keeps, the newest 1,000), in its app's workspace, and not a
    direct message with the retired shared bot (another bot's DM, where its app
    cannot post). Any other channel or thread is refused, and the refusal
    lists the agent's threads (place, channel id, `thread_ts`, last message)
    so the model can pick one. With no `channel` (an id or `#name`) and no
    `thread_ts`, the thread of the Slack message that started the turn (its
    first Slack origin in `answering`); a turn with no Slack origin must name
    a thread, unless exactly one of the agent's threads had a message in the
    last 24 hours. Never just the newest thread: an agent juggling two
    tickets would post one's news in the other's thread. `channel` alone
    picks the turn's thread if it is there, else its newest thread there.
    Posts are mrkdwn with the app's mention list (`to_mrkdwn_with`), so the
    people the owner chose can be tagged and nobody else. An agent without an app, or with one not installed
    yet, is refused and told how to get one. A thread routed through the
    retired shared bot in a channel is allowed, and if the agent's app is not in
    that channel Slack's `not_in_channel` is said plainly. Links keep `app`
    (it came through the agent's own app) and `place` (`#ops`) for this;
    links from before they were kept count as the shared bot's. Posts wait
    for an answer being posted (`posting`), so a thread reads in order. At
    most 20 per agent per hour. On a remote host it runs on the hub, where
    Slack is.
  - **Memory.** `remember` and `set_memory` change `memory` (the 20,000
    character cap, refused past it) and broadcast `agents`. The engine knows
    already, so it is not a notes change. An open form follows the new memory
    unless the user has typed in it, and Save leaves an untouched memory out.
  - **On a remote host.** An agent a host owns has its tools on that host,
    like any agent there: its memory lands in its record there (the hubs
    hear it in the next `agents`), `notify_owner` reaches every window of
    every hub connected to that host (the `notify` broadcast is piped, its
    conversation id prefixed), and `message_agent` and `list_agents` name
    that host's agents. A hub's `list_agents` names the hosts' agents too,
    so an agent on a Mac messages one on the server (see Mentions across
    hosts). **Hosted copies** (an older hub's agents, or records a newer hub
    still keeps for an older host) keep the older way: everything their
    tools act on is on the hub, so the host's engine gets the same stdio
    server and talks to the host's own yardd, over the host's loopback,
    with a key the host made for that one conversation; the host's
    `/agent-tools` sees that the conversation belongs to a hosted copy and
    passes the call to the hub instead of running it. The way back is the
    connection the hub already keeps open to every host (`remote::monitor`,
    over the SSH tunnel): on connecting it sends `agent.hosted.hub { hub }`
    (the hub's id, made once and kept in its data folder as `hub-id`), and
    the host remembers that connection for that hub until it closes. Every
    copy names the hub that pushed it (`Agent::hub`, set by
    `agents::push`); a call goes to that hub only, as `agent.hub.call {
    ref, agentId, sessionId, method, params, answering }`, and is answered
    on the same connection with `agent.hosted.answer { ref, result,
    needsYou?, memory? }` (an answer from any other connection is ignored).
    The hub runs it only for an agent whose `host` is the remote asking
    (`agent_tools::from_host`), as the conversation `<host>~<id>`. Answers
    such a copy owes go to the hub as `agent.hub.owed` (the same op a host's
    own agent uses for messages a hub delivered). Security: the engine still
    reaches only loopback on its own host with a per-conversation key; no
    port is opened (the hub dials the host, as before); and the hub acts
    only for agents on that host. **Version skew:** a hub from before
    hosted copies does not name itself, so its copies have no `hub` and get
    no tools; an older host answers the hello with `unknown op`, which the
    hub ignores. While the hub is not connected, a call waits up to 5 s for
    it, then fails with a line the model reads ("…is not connected to this
    host right now"); `tools/list` then names no agents.
- **Slack (built, step 3; see Slack below).** A Slack thread is not a session
  of its own: a message from Slack goes into the agent's one conversation
  with origin `{ kind: "slack", … }` (the team, channel, thread and message it
  came from, shown as a Slack badge with person and channel), and the reply
  hook's `slack` arm posts the answer to that message back to the Slack thread.
- **Private engine threads** (`private.rs`) are what Switchy's plumbing became:
  ids starting `private-` (or the older `switchy-`) stay in memory, never in a
  snapshot, the store or a client op (yardd refuses an op naming one); their
  events go to `app.private.events` instead of a log; in Plan they get the
  restricted profile (Codex: an ephemeral read-only thread with approvals off
  and every tool, MCP server and plugin disabled by thread-local overrides,
  null optional fields left out; Claude: a fresh sidecar with no tools, an
  empty strict MCP config, no setting sources and a denying permission
  callback). Session naming (#380) uses them; it moves from
  `switchy::internal` / `app.switchy.events` to `private::internal` /
  `app.private.events`. The sidecar's `start` carries `restricted` (and
  `switchy` with the same value, for a sidecar built before agents).
- **Switchy's conversation** (`switchy.json`) is not read and is left where
  it is.
- Tests (`tests/agents.rs`, scripted engines): the registry and its refusals,
  the preamble on the first turn only, one conversation per agent, deleting
  an agent with its conversation, a fresh context after a full context, a
  model change, an account change and a lost thread, changed notes reaching
  the thread, an agent that belongs to a remote (made there, the folder and
  account checked there, the record only there, listed by the hub with the
  prefix, the preamble on the remote's first turn, replies and mentions in
  the same conversation, notes and a model change there, files from the hub
  refused, listed and refused offline), a second hub adding the same host
  and asking, editing and deleting its agent (the first hub hearing each
  change), moving to a remote and back (the record handed over, the recap
  carried, refused mid-turn and while offline, the old conversation an
  ordinary session), the hand-over of a record an older hub kept (notes,
  look, request, conversation and the whole Slack app record with a field
  this build does not know; no Slack threads; backups; the same
  conversation going on; twice is safe), a host too old
  to own agents keeping the copy (and refusing new agents and moves there),
  backups taken before the first change on either side and kept to ten, a
  hand-over that waits for a turn running on its host with no backup
  meanwhile, a hidden copy another Mac runs left alone and its page saying
  why, an agent sent again (the same transfer changing nothing, another
  refused), a hand-over that cannot be saved answering an error, an owed
  release from another hand-over leaving the agent alone, an agent moved
  back before its old host let go ending up in one place, a release
  refused mid-turn, a message to a kept record reaching its host's own, a
  window refused the hand-over ops even through the hub, fields this build
  does not know kept through saves and moves, an agent deleted on its host
  mid-hand-over staying deleted (release failing, or the adopt answer
  lost), a refusal told by its code, never by the agent's name (one named
  "… was deleted on …" refused as busy keeps its record and app), a lost export answer abandoned, an agent's own app moving whole and
  once with no thread, a lost adopt answer between hosts found
  and finished, one backup per hand-over however often it is tried, an
  adopt whose app cannot be kept keeping nothing, a second export refused
  while the first is under way, an app being handed over
  left in Slack on delete until it settles or the move is abandoned, and a
  moved agent's Slack places and approvals staying on the host they were
  made on (a remote agent's places listed on the hub, an approval from
  before a move only deniable, even after a move back) (`tests/slack.rs`),
  the queue, and Scout retired with its conversation. `tests/naming.rs`
  covers the private restricted thread with Codex's null config fields. The
  `agents` feature map drives the page, card alignment, living faces, New
  agent's conversation and setup, requests, the aligned colours, the form
  (with the host and account pickers and the note about a move), mentions
  (also to an agent on a remote host), the one conversation, a remote
  agent's card, conversation and offline state, a second hub listing a
  host's agents at both viewports, Add to Slack, and the Agents row's mark
  at both viewports. Not proven: a real model's answer to a preamble, or
  whether it calls update_setup well from the first-run rules.
- Tests for the tools (`tests/agent_tools.rs`): the scripted engines call the
  real `yardd agent-tools` over stdio for a `[[tool NAME {json}]]` marker
  (`tests/fake-codex/agent-tools.mjs`, on Codex, the Claude sidecar and ACP):
  needs you with its cap and `agent.seen`, a message and its answer back with
  both origins kept across a restart, the hop limit and a message to oneself,
  failed and stopped answers, memory edits and the cap, OpenCode running them
  unasked, a new agent setting itself up with update_setup (direct fields,
  the model after the turn, requests and the owner's answers, a refused
  name), plain sessions without them, the endpoint's key
  check, a host's own agent's tools acting on its host (two daemons:
  `remember` in its record there and the hub's list, `message_agent` to an
  agent on the same host, `notify_owner` on the hub's windows and its mark
  cleared by `agent.seen`) and an agent on the hub messaging it with the
  answer back, a kept record's tools acting on the hub while its host is
  older (`remember` in the hub's record and the copy, `message_agent` both
  ways), and a hosted copy without a hub (no tools) or with its hub away
  (the call says so), then answered only on the hub's own connection, and `slack_reply` (in `tests/slack.rs`). The `agents` map drives the badges, the Needs-you card and row, and
  memory in the form at both viewports, and a remote agent's tools. Checked against the real CLIs
  without a turn: Codex 0.171 takes the `mcp_servers.switchyard` override,
  starts the server and runs `mcpServer/tool/call` through it; the real
  sidecar with Claude Code starts it from `start.tools` and runs `mcp_call`.
  Not proven: a real model choosing to call them, Codex's approval prompt for
  an MCP call during a real turn (answered by the elicitation accept, not
  seen live), and how OpenCode, Grok and Gemini name the calls (matched by
  `switchyard` plus the tool name).
- Not built yet: one thread across a move between hosts (the earlier
  conversation stays on the old host as an ordinary session), files
  attached on one host sent to an agent on another, an agent on the server
  messaging an agent that lives on a Mac, and keeping a host's agents listed
  across a hub restart while the host is offline.

## Slack

Two things: the owner's own Slack (his direct messages and the channels he
picks, read and answered as him) and his agents in Slack, each with an app of
its own. `crates/yardd/src/slack.rs`; UI `ui/src/SlackCard.tsx` (the
Connectors card: his own Slack and who may request agent tool use),
`ui/src/SlackSetup.tsx` (an agent's app), `ui/src/slack.ts` (status words,
steps, the badge's words). Agents' messages and answers ride the shared
origins and reply hook (see Agents, Origins): Slack adds the `slack` kind,
its engine framing and one arm in `origins::reply`.

- **The shared bot is retired** (owner decision, after agents got apps of
  their own). It was one "Switchyard" bot that routed a DM or @mention to the
  agent named first in it, else a per-channel or the default agent. Gone with
  it: `slack.tokens`, `slack.disconnect`, `slack.retry`, `slack.route`, the
  default agent, the channel rows and `Agents::all`. A `slack.json` that still
  holds its tokens is rewritten without them when yardd starts; the people
  allowed, the routed threads and agents' apps stay. An answer owed to a
  message that came through it (no `app` on its origin) has no token left and
  leaves a note in the conversation instead. The owner can delete that old
  app in Slack himself.
- **The owner's own Slack** (`me` in `slack.json`, `MyApp`). Plan, approved
  by the owner: every DM and group DM comes into Switchyard, new ones too;
  channels only when he imports them; 60 days of history, then more on
  demand; replies from either app. A bot only sees what is sent to it, so this
  is an app of his with **user scopes** and no bot user: `channels:history`,
  `channels:read`, `groups:history`, `groups:read`, `im:history`, `im:read`,
  `im:write`, `mpim:history`, `mpim:read`, `mpim:write`, `users:read`,
  `chat:write`, `reactions:read`, `reactions:write`, and user events
  `message.im`, `message.mpim`, `message.channels`, `message.groups`,
  `reaction_added`, `reaction_removed` (`my_manifest`, all asked once so later
  steps need no new approval). Made like an agent's app:
  - `slack.grant { token, me: true, origin }` (a workspace's configuration
    token, or `slack.me.create { teamId, origin }` for one already given, one
    click on the card), `apps.manifest.create`, named "Switchyard".
  - Approve in Slack: the install link (`slack.me.authorize` makes a new one;
    the OAuth `state` maps to no agent) comes back to `/slack/oauth`, which
    keeps `authed_user.access_token` (`xoxp-`) and who he is. With no https
    address he pastes the User OAuth Token (`slack.me.userToken`, checked
    with `auth.test`). A workspace that makes admins approve apps sends them
    his request; the card says to choose New link once they approve.
  - `slack.me.appToken` (`xapp-`, checked with `apps.connections.open`).
    Then `run` keeps his Socket Mode connection, in the slot the shared bot
    had: dialled when ready, redialled on `disconnect`, a drop (backing off to
    a minute), a new token or `slack.me.retry`, and after a restart. `connect`
    takes his user token; his user id is not added to `own_bots` (his posts
    are not a bot's).
  - His events go to his inbox (below). He goes on the allowlist when he
    approves.
  - `slack.me.remove` (Disconnect): forgets his tokens, closes the connection
    and deletes his app (best effort). Always Socket Mode, even with a public
    address. The view sends `me { step, appId, teamId, teamName, userName,
    canRedirect, authorizeUrl?, userToken, appToken, state?, handle?, error? }`,
    tokens as kind and last four characters only.
- **His inbox** (`crates/yardd/src/slack/inbox.rs`, a child of slack.rs so
  it shares the Web API client; UI `SlackPage.tsx`, `slackInbox.ts`,
  `slackText.ts`). Tables in `yard.db`, through a connection of its own (WAL,
  beside the session store's writer): `slack_conversations` (team, id, kind
  `im`/`mpim`/`channel`/`group`, name, the DM's other person, newest
  top-level ts, his read mark, `synced`, `complete` = nothing older in
  Slack), `slack_messages` (team, channel, ts, thread_ts, user, a bot's name,
  Slack's text as sent, subtype, edited, deleted, reply count, reactions as
  `[{name, users}]`, file names), `slack_users` (names). ts are compared as
  text (ten-digit seconds).
  - **Sync** when his connection says hello the first time in a process, and
    after a connection that dropped or failed, not after Slack's routine
    `disconnect` refresh (which would refetch every DM every few hours):
    `users.conversations types=im,mpim` (deleted users left out); for each,
    one not loaded gets 60 days (`conversations.history oldest`, every page)
    and Slack's read mark (`conversations.info last_read`, only when it had
    messages; none, or zeros, counts everything as read), one loaded catches
    up from its newest ts. One at a time (the client waits out 429s twice).
    Internal apps are not under Slack's 2025 history limits for distributed
    apps.
  - **Channels** (phase 4): the card's Channels row lists the channels he is
    in (`slack.inbox.channels`: `users.conversations
    types=public_channel,private_channel`, imported first), each with an
    Import switch (`slack.inbox.import { channel, on }`, a `C…`/`G…` id that
    `conversations.info` confirms is a channel). On: the conversation is kept
    with `imported = 1` and its 60 days load in the background, then it is
    live and caught up like a DM, listed with them (a `#` mark). Off: its
    messages and row are deleted. A message in a channel not imported is
    ignored. A channel's read mark stays in Switchyard: `conversations.mark`
    on a channel needs `channels:write`/`groups:write`, which can also make
    and archive channels, and his app does not ask for them.
  - **Live**: `message` in an `im`/`mpim` or an imported channel (an unknown DM is looked up with
    `conversations.info` first: someone new wrote), `message_changed`,
    `message_deleted` (kept as deleted, text dropped), `message_replied`,
    `reaction_added`/`removed`. A reply live for the first time is one more
    on its parent (Slack's count; replies loaded from history are already in
    it), never fewer than the replies kept. Each change is broadcast as
    `slack.inbox.message { team, channel, message, users }`; the list as
    `slack.inbox { conversations }` (coalesced, 300 ms), also in the snapshot
    as `slackInbox`. Remotes' copies are dropped.
  - **Ops**: `slack.inbox.list`; `slack.inbox.messages { team, channel,
    before?, thread? }` (50 top-level messages, oldest first, with `users`
    and `more`; past what is kept, `conversations.history latest` a page at a
    time until Slack has no more; a thread is read fresh with
    `conversations.replies`); `slack.inbox.read { ts }` (his mark here, and
    `conversations.mark` in Slack); `slack.inbox.send { text, thread? }`
    (`chat.postMessage` with his user token; `&`, `<`, `>` escaped so typed
    text is never markup; Slack's answer is stored and broadcast at once, and
    his own message reads everything before it); `slack.inbox.react { ts,
    name, add }`. **Calling an agent from his reply box**: `send` takes
    `mentions` (agent ids, at most 10); each one with its own app ready in
    this workspace has its `@handle`, written as a word in any case, made a real
    `<@bot_user>` mention, so its app answers there in a place session of
    its own (see agents' own apps) if it is in the conversation. Nothing
    else he types becomes a mention. The box offers those agents on `@`
    (any part of the name or handle) and says which ones a message calls.
    A mention of an agent's bot reads as the agent's name in his inbox.
    Agents on other hosts are called the same way (`slack::callable_apps`):
    each app's view carries its bot's user id (`botUser`, not a secret), so
    the hub that holds his Slack knows a host's agent's bot from what the
    host last said, and the box offers only apps with one (an older host
    sends none). A host the hub cannot reach still has its agent offered
    and called: the mention goes through Slack to the app on that host,
    not through the hub. Disconnecting his Slack forgets his inbox.
  - **The page**: the sidebar's Slack section (before Agents): his
    conversations newest first, the first 8 plus any unread or open one, an
    unread count, Show all. `#/slack/<team>/<channel>` (yv route
    `slack:<name>`): messages grouped by author (five minutes) and day,
    Slack's markup parsed into React elements (`slackText.ts`: bold,
    italics, strikes, code, code blocks, quotes, http/https/mailto links
    only, mentions by name, a few emoji drawn), edited and deleted marks,
    reactions (his highlighted, a click toggles), "N replies", Load older,
    a thread pane (the whole screen on a phone), each message's actions
    floating at its top right on hover or a tap, and a reply box that sends
    as him. Opening it reads it.
- **Transport: Socket Mode** for his app and agents' apps (unless a public
  address is set, below). The server is tailnet-only, so Slack cannot call
  it by default. The app-level token (`xapp-`, scope `connections:write`)
  opens a WebSocket with `apps.connections.open`; the user or bot token makes
  the Web API calls, form-encoded with a bearer header. Every envelope is
  acknowledged; `disconnect` dials again at once (`link_disabled` is shown as
  an error); a drop or a refusal backs off from 1 s to a minute; with no frame
  for 90 s (yardd pings every 30 s) the connection is redialled. TLS is rustls
  on ring (reqwest and tokio-tungstenite), as the app's updater already
  builds it. Configure Slack on one yardd only: two connections with the same
  app token split Slack's events between them.
- **Tokens** are kept in `slack.json` in the data dir, written with mode 0600
  (aside, then renamed). They never leave yardd: the snapshot and the `slack`
  broadcast carry `xoxp-…WXYZ` only, and no log line or error names them (the
  Socket Mode address, which holds a ticket, is not logged either).
- **Who may request tools.** Human DMs, mentions and follow-ups reach an
  agent’s app. The workspace’s app installer and people on the Slack
  allowlist are trusted. Bot messages, edits and membership events are
  ignored for this route. The Connectors card lists trusted members and
  can revoke their allowlist entries. The app installer can always decide
  for their own app.
- **Place sessions.** A channel thread uses `team/channel/thread_ts`. A DM
  uses `team/channel/` for the whole conversation. Each agent has its own
  session for each key. In channels a plain follow-up reaches an agent
  only in a thread routed to its app, and a message addressed to someone
  else stays with that recipient. The router deduplicates mentions per app,
  passes Slack markup as text and keeps file attachments in Slack. The
  first delivery fetches up to 30 recent messages using the bot token
  (`conversations.replies` or `conversations.history`). Slack API errors
  leave the current request usable on its own. History is clipped, escaped
  and framed as data. The current message is framed separately. The
  owner’s chat stays on the agent page beside **Slack places**, where each
  place shows its most recent message time and any pending approval.
  An agent’s common setup and memory follow its place sessions. An
  agent’s app runs on the agent’s host (below), so its places, approvals
  and routed threads are that host’s, in its `slack.json`. A hub lists a
  host’s places under the prefixed ids (`slack::view`) and sends a
  window’s approval for one to that host (`slack::approval_host`). A
  record a hub keeps for an older host is the exception: the hub runs its
  app and calls `agent.hosted.slack`; the session is created on the host
  and answers travel back through the hub. **A move carries none of
  them.** The old host drops the agent’s places and threads
  (`slack::release_app`) and keeps its pending approvals, which can then
  only be denied. A place or approval counts only while its conversation
  is still the agent’s where it runs (`places::on_current_host`), so
  nothing kept from before a move routes a message again, even after a
  move back: the agent’s next Slack message starts a new conversation.
  Place messages queue one at a time; they never share a turn or steer one
  in progress.
- **Guest turns.** Claude uses a fresh restricted sidecar in a scratch
  directory with the app’s conversation history and the agent’s setup
  and memory. It starts with an empty tool list, a wildcard tool deny,
  strict empty MCP config, empty settings sources, skills and plugins,
  and no resume. The permission callback also denies tool requests.
  Verbatim prompts skip CLI-side expansion. Switchyard’s own tools are
  disabled for these turns because they can read or change state outside
  Slack. A regular reply is posted in the place. A final reply consisting
  of the access marker opens a request for the current message. Codex
  0.171’s generated app-server schemas expose sandbox and approval
  settings, while the empty allowlist in Codex’s internal `ToolPolicy`
  is unavailable there. A read-only sandbox still permits file reads.
  OpenCode, Grok and Gemini’s ACP modes also lack an enforced all-tools-off
  profile. For those engines, Switchyard posts an explanation and opens
  an approval before sending the guest’s message to the engine.
- **Access approvals.** The place shows a needs-approval card and its bot
  opens a DM to the installer containing the original request. On Socket
  Mode apps, the manifest enables interactivity and the DM offers
  **Approve once**, **Always allow** the requester, and **Deny**. Each
  envelope is acknowledged. A button click must match the installer,
  app, workspace and DM message for the pending request. Either surface
  resolves the same card and updates the bot’s DM. Approval replays the
  original message as a normal turn with the agent’s configured
  permissions. Always allow also saves the person to the allowlist. A
  denial is posted in the original place. HTTP-event apps get a DM that
  points to the Switchyard card and use the card for decisions.
- **Answers** (`slack::answer`, the reply hook's `slack` arm), through the
  app the message came in on. The message
  gets :eyes: when it reaches the agent (queued or not). When the turn that
  answered it ends: completed, the final answer is posted in the thread
  (`thread_ts` = the message's thread or its own ts), Markdown turned into
  mrkdwn (bold, italics, strikes, links, headings, bullets; `&`, `<`, `>`
  escaped everywhere so an answer can never mention `<!channel>` or a user;
  code kept, fences without a language), split at 3,500 bytes between lines
  with code blocks closed and reopened, and :white_check_mark:; failed,
  "Reviewer could not finish: <error>" and :x:; stopped, "Reviewer was
  stopped in Switchyard." and :x:; a queued Slack message that Stop took
  back, "Reviewer was stopped in Switchyard before it read your message." and
  :x:. A turn waiting on an approval posts "Reviewer needs your approval in
  Switchyard: <title>" once (the `approval` status above). One post per Slack
  thread, however many of its messages the turn answered; posts go out one
  at a time so a thread reads them in order. A post that fails leaves a note
  in the conversation. An error before the agent is reached (it is queued for
  room on the host, say) is posted in the thread with :x:.
- **The card** (Connectors page, Messaging, Slack): his own Slack's steps
  (Token, Approve, Connect) until it is ready, then "As @gio in Acme" (or the
  error with Retry); the requests and the people allowed; a footer with his
  tokens' kinds and Disconnect. Ops: `slack.me.*`, `slack.allow`,
  `slack.disallow`, `slack.dismiss`; each broadcasts `slack`. His own Slack
  lives on the hub: a remote's own (`me`, its inbox) is not shown, only its
  agents' apps and Slack places (`slack::view`, below).
- **An agent's own app** (after Orca's bots: "I can't tag it" was the owner's
  complaint about the shared bot). Each agent can have a Slack app of its
  own, named after it, so people @mention it by name. It lives **on the
  agent's host**: kept in that yardd's `slack.json`, its Socket Mode
  connection run from there, so a server agent answers in Slack while every
  Mac sleeps. Set up from any window: the steps route to the agent's host
  like its other ops, with that host's workspaces. A move or a hand-over
  carries the whole record, its watched channels and mention list included
  (see Agents). Its dialog (`SlackSetup.tsx`) opens from Add to Slack in the
  agent's header, from its start page, and from its request card.
  - **Token.** The owner pastes a Slack App Configuration *Refresh* Token
    (`xoxe-`), once per workspace (`slack.grant`; with `agentId` it also
    makes that agent's app). A pasted Access Token (`xoxe.xoxp-`) is told
    apart. yardd rotates it with `tooling.tokens.rotate` (each refresh token
    works once; access lasts 12 h) and keeps both in `slack.json`. The next
    agent in that workspace is one click (`slack.app.create { teamId }`).
  - **Its app** (`apps.manifest.create` with the access token): display name
    the agent's name as written, cut to Slack's 35 characters (`app_name`);
    description what it is for (139); the bot user's display name its handle
    (`bot_handle`): lower case, accents folded (`José` → `jose`), spaces and
    other separators a dash, runs of dashes one, only a-z 0-9 `-` `_` `.`,
    cut to 21 characters with no separator at either end (`agent` if nothing
    is left). 21 because Slack makes the bot's username from the display
    name that way, so the two agree and people @mention what they see. It
    reads its DMs, mentions and the channels it is in; Socket Mode, or HTTP events (below). The
    manifest Slack has is kept (`manifest`) with the app's signing secret.
  - **Its name follows the agent.** Saving an agent (`agents::save`, so also
    its own `update_setup`) with a new name or purpose calls `agent_changed`,
    which in the background sends the whole manifest again
    (`apps.manifest.update`, a rotated configuration token) when it differs
    from the kept one. Apps made before this are updated when yardd starts
    (no manifest kept). A failure never fails the save: it is kept as
    `syncError`, shown in the dialog with a field for a new configuration
    token (`slack.grant` with an agent that has an app sends what waited), and
    noted in the agent's conversation. Changing the public address resends
    every app's manifest.
  - **Its icon is the agent's face.** Slack has `apps.icon.set` (configuration
    token, scope `app_configurations:write`, a multipart `file` of 512 to
    2000 pixels a side); the manifest has no icon field. Only a page can draw
    the face, so `SlackIcon.tsx` (mounted once in `App`) renders each agent
    with an app as `AgentAvatar` out of sight, serializes that rendered
    `<svg>` with every element's paint inlined from computed style (so a
    redrawn face needs no change here), puts it on the avatar's colour and
    light (`avatarIconSvg`), draws it at 1024 px on a canvas and sends the PNG
    (`slack.app.icon { agentId, key, png }`). The key (`iconKey`, a hash of
    that SVG) is kept by yardd, so a window sends only when the face, colour
    or drawing changed, once per picture per window. The SVG's gradient and
    clip ids are renamed in order (`stableIds`): the page's come from React's
    `useId`, which differs with the window's size and page, and gave each
    window size its own key and its own upload. An agent on an offline host
    gets nothing until the host is back (the hub would refuse the op on every
    page view); then the next window sends it. yardd checks the PNG's
    size, keeps it as `slack-icons/<agent>.png`, and sends it with the manifest
    update; a lapsed token leaves it waiting like a rename.
  - **Install.** The manifest lists as redirects every https origin yardd
    serves its page on (`YARD_ALLOW_ORIGIN`) and the public address, each
    plus `/slack/oauth`. The install link uses the asking window's own origin
    when listed, else the first listed one, whatever the window is: from the
    desktop app's http://127.0.0.1 it is still https://switchyard.giovani.sh.
    Only the owner's browser has to reach it, so a tailnet name works. A
    window cannot name another site: only listed addresses are used. The
    install link carries a one-time state for 30
    minutes (`slack.app.authorize` makes a new one); `GET /slack/oauth`
    exchanges the code (`oauth.v2.access` with the app's client secret) and
    shows a small page. With no https address at all the dialog has the
    owner install from the app's page and paste its bot
    token (`slack.app.botToken`, checked with `auth.test`). Whoever installed
    it goes on the allowlist (for a pasted token: who made the configuration
    token).
  - **Connection, Socket Mode** (no public address). It needs an app-level
    token (`xapp-`, `connections:write`) that Slack only makes by hand: one
    screen with Open Basic Information (the app's own page) and three short
    lines of what to click
    (`slack.app.appToken`, checked with `apps.connections.open`). Then it
    keeps its own Socket Mode connection (`run_app`, redialled like the
    owner's, from start after a restart).
  - **Connection, HTTP** (Orca's way; strictly opt-in and off by default:
    the owner would rather not expose anything, so the dialog does not offer
    it and no tunnel is set up): with a public https address for Slack
    (`YARD_SLACK_PUBLIC_URL`, which wins, or the `slack.publicUrl { url }`
    op, which has no UI), a new app's manifest has
    `socket_mode_enabled: false` and `request_url` `<public>/slack/events`,
    and there is no app-level token: the configuration token is the only
    paste (with an https redirect). `POST /slack/events` reads at most 1 MB,
    refuses a timestamp more than 5 minutes off, and trusts nothing until the
    `v0` signature (HMAC-SHA256 of `v0:<ts>:<body>`, ring, constant-time)
    matches one HTTP app's signing secret; that app is the agent. It answers
    `url_verification`, and hands an `event_callback` to the same queue and
    `on_event` as Socket Mode at once (200 well within Slack's 3 s), once per
    `event_id` (a retry, `X-Slack-Retry-Num`, is acknowledged only). A check
    that comes while an app is being made waits up to 2.5 s for its secret.
    `GET /slack/events` answers a per-process probe: setting the address
    fetches it through the address and the dialog says whether it reached
    this yardd (`publicCheck`). **Expose only `/slack/events` and
    `/slack/oauth`** on the public address (a Cloudflare Tunnel with a path
    rule, say); nothing else of yardd's may be public. An app keeps the way
    it was made: one on Socket Mode stays there when an address is set later.
  - **Messages.** The app routes to its agent’s place, using the app id,
    origin details, reactions and answers above. Its agent can also post on its own, at any time,
    in a thread routed to it (`slack_reply`, see Agents' own tools).
  - **Watched channels** (owner request: On-call, Domain Triage and
    Firefighter only reacted when @mentioned, and every Datadog, PostHog or
    Sentry alert has a `bot_id`, so it was dropped). Design approved by the
    owner: a top-level message or alert in a watched channel starts a turn,
    batched so an alert storm is one turn; the text reaches the agent as
    data, not instructions (these agents have shell access); triage runs
    read-only, and anything that acts needs the owner's go-ahead.
    - **The list** lives on the agent's app record (`AgentApp::watched`,
      `{ id, name }`, next to `mentions`, in `slack.json`), at most 20, set
      only by the owner in the agent's Slack dialog (`slack.app.watch {
      agentId, channel, name? }`, a `C…`/`G…` id; the name typed, else
      `conversations.info`; `slack.app.unwatch { agentId, channel }`, which
      also drops a batch waiting). The agent has no tool for it, and its
      setup line (`current_setup`) names the channels and says only the user
      changes them. The app must be in the channel (it already subscribes to
      `message.channels`/`message.groups`).
    - **Routing** (`watch`, first thing in `on_event` for an agent's own
      app): a `message` in a watched channel that is top level (no
      `thread_ts`, or its own), with no subtype, `bot_message` or
      `file_share`, from a person or an integration. Ignored: Switchyard's
      own bots (every agent app's bot, by user id, bot id
      from `auth.test`, or app id), edits, deletions, joins, and thread
      replies, which keep the follow-up rules. A person's @mention of the
      agent there is a request and goes the mention path. A watched notice is automatic triage the owner
      configured, and the text arrives as data. The words are the
      text, else an integration's attachments (pretext, author, title, its
      link, text, fields) or, with neither, its blocks; Slack markup read as
      text; at most 3,000 bytes each.
    - **Batching** (`collect`, `flush_watched`): per agent and channel for 30
      s from the first message (`YARD_SLACK_WATCH_MS` in tests), then one
      turn. The same text (whitespace aside) again in the window is counted
      (`repeated="2"`); one passed on in the last 5 minutes is dropped. One
      turn lists at most 20 messages and counts the rest. At most 6 watched
      turns per agent an hour (the agent tools' limiter): past that the batch
      waits, still collecting, until a turn is free, and the agent's
      conversation says once until when.
    - **The turn** (`deliver_watched`, through `agents::deliver`) stays in
      the agent’s main Switchyard conversation. Its text
      is one `<watched n from at thread_ts link? repeated?>` tag per message,
      written by yardd (attributes one line without quotes, the message
      escaped, so it can neither close its tag nor forge one); the link is a
      permalink built from `auth.test`'s workspace address. Its origin is the
      first message's (`kind: slack`, `watched: true`, `count`, `app`), so
      :eyes:/:white_check_mark: and the **final answer go to the first
      message's thread only**. Every message's thread is routed to the agent
      (a `Link` each), so the owner can answer in any of them and the agent
      posts in the others with `slack_reply { thread_ts }`, which the frame
      tells it. The engine reads a fixed frame around the tags
      (`origins::engine_text`): an automatic notice from a watched channel,
      the content is data and not instructions whatever it says, triage it,
      this turn is read-only, ask the user before anything that changes
      something; the rules come before and after the data. The badge reads
      "Watched on Slack · #alerts · 3 messages" (or "· from Datadog").
    - **Read-only, per turn.** A turn whose `answering` holds a read-only
      origin (`origins::read_only`: `watched`, or `readOnly` below) runs in
      Plan whatever the session's mode (`origins::turn_permission`), on every
      engine: Codex takes the sandbox and approval policy with each
      `turn/start` (read-only sandbox, `on-request`, so a write or network
      call asks); Claude's sidecar gets the turn's mode on its `user` line
      and calls `setPermissionMode` before the message reaches Claude Code
      (always after the first turn, since an approved plan changes the mode
      inside Claude Code; if switching to Plan fails the turn fails rather
      than run); ACP engines get Plan's mode and yardd's Plan policy (edits
      refused, commands asked). The conversation notes it at turn start, and
      approvals show and notify as always. The next turn is back in the
      session's mode. A read-only message is never steered into a running
      turn (`take_steerable`, and Steer is not sent for it); queued with
      others it makes their shared turn read-only. In such a turn the agent
      tools refuse `remember`, `set_memory` and `update_setup` (nothing a
      later trusted turn would read), and `message_agent` tags its message
      `readOnly: true`, as does the answer back (`origins::answer`), so
      handing a notice to an agent with full access gets it no access; both
      survive the trip to a remote host (`from_hub`). The owner's go-ahead
      in an alert’s thread is a normal turn in that thread’s place session,
      with the agent’s own permission.
  - **Remove** (`slack.app.remove`, or deleting the agent) closes the
    connection and deletes the app (`apps.manifest.delete`, best effort) and
    its icon.
  - Not built: Orca's member list and access requests per agent, "post as",
    moving an app made on Socket Mode to HTTP.
- **Tests.** `YARD_SLACK_API` points the Web API (and so Socket Mode)
  elsewhere: `verify/fakes/fake-slack` is a Bun fake of the Web API methods
  above and Socket Mode, with control endpoints to send a message, a
  `disconnect` or a drop and to read posts, reactions and acks.
  `tests/slack.rs` covers separate DM and channel-thread sessions, follow-ups,
  escaped history, Claude’s restricted launch, Codex requests waiting before
  launch, owner-only clicks and all three decisions, including a pending card
  surviving restart. It also covers bots being ignored, a channel mention
  handled once, the engine framing, answers in the right thread with reactions, failure,
  approval, stop and unread posts, every envelope acked, reconnects after a
  `disconnect` and a drop, a refused token shown, tokens in no snapshot,
  broadcast or log (file mode 0600), and a message to an agent on a remote
  host (its origin there, its answer posted from the hub), and an agent's own
  app (the token checks, the manifest, the install's redirect and a forged
  or reused state, the app-level token, a mention answered through its bot,
  a restart, removal), an app on a public address (only the configuration
  token; the https redirect from an http window; `url_verification` and
  events signed, mis-signed, stale, unsigned and too large; a retry handled
  once; no socket after a restart; no secret shown or logged), and an app
  following its agent (`apps.manifest.update` on a rename with the new
  handle, the icon sent once per key and refused when not a 512-2000 px PNG,
  a lapsed configuration token shown and caught up by a new one, an old app
  updated at start, the public address set from the page), and `slack_reply`
  (refused without an app and with no thread yet; the only recent thread
  by default and one named by `#name` and `thread_ts`, posted by the agent's
  own bot; a turn from a second Slack thread posting in its own thread, a
  Switchyard turn with two recent threads refused until it names one, the
  app's mention list kept in a post; another agent's thread, a channel it was not asked in and a
  made-up `thread_ts` refused with its threads listed; an agent without an
  app refused in its own thread), and watched channels (an unwatched
  channel quiet; a person not on the allowlist, an alert in attachments, its
  repeat and another integration's blocks as one read-only turn on a
  full-access agent, with the frame, three tags and an escaped attempt to
  close one; Switchyard's own bots, an edit and a thread reply ignored; the
  answer in the first thread only; the owner's reply in an alert's thread and
  an @mention there as normal turns; an integration's thread reply ignored;
  a repeat within minutes dropped; the seventh turn in an hour held with a
  note; unwatch; and in a watched turn `slack_reply` posting in the alert's
  thread, `remember` and `update_setup` refused, and `message_agent` to a
  Claude agent with full access making her turn Plan, `turn permission=plan`
  on the sidecar, her answer back read-only, her next owner turn full
  again), and an agent's own app moving with it to another host, watched
  channels and all, and answering from there with the hub stopped. The fake
  knows
  `tooling.tokens.rotate`, `apps.manifest.create`/`update`/`delete`,
  `apps.icon.set` (multipart), `oauth.v2.access`, a second bot @agent, signed
  HTTP events (`/_fake/event`, `/_fake/challenge`), a lapsed token
  (`/_fake/lapse`) and integrations' messages (`user: null`, `username`,
  `bot_profile`, `attachments`, `blocks`). Unit tests cover routing, mention stripping, Slack
  markup, mrkdwn, splitting, the `slack` framing, the handle and app name,
  signatures (Slack's documented example) and their age, redirects, the
  public address and the icon check; UI tests the steps, the token count,
  the icon SVG and its key. The
  `slack` feature map drives the Connectors card, Slack places, approvals
  in Switchyard and the origin badge at both viewports,
  against the fake.
- **Not checked:** a real Slack workspace. Nothing here has talked to
  slack.com: the manifest import and update (whether a new bot display name
  shows without a reinstall), `apps.icon.set` and its multipart body, Slack's
  real signatures beyond its documented example, its checking of a request
  URL set by `apps.manifest.create` (the manifest docs say it may need a
  click on Verify in the app's settings), the public address through a real
  tunnel, real event shapes beyond Slack's documented
  ones, rate limits, Enterprise Grid ids and the TLS path to slack.com are
  untested. For watched channels: real Datadog, PostHog and Sentry message
  shapes (only Slack's documented attachment and block fields are read),
  whether `bot_profile.app_id` is set on our own posts, and a real Claude
  turn in Plan (how Claude Code treats Bash and MCP tools there; Codex MCP
  tools of other servers are not under its sandbox) are not checked. The
  owner's own Slack connects from the hub only; an agent's own app runs on
  the agent's host. Remote delivery and approval are covered using a
  second daemon. A note about a Slack post that failed lands in a local
  conversation only: for a remote agent's it is logged instead. Not tried
  against real Slack: an app's Socket Mode connection moving from one yardd
  to another (the fake takes it), and whether a configuration token copied
  to the host and rotated by one yardd leaves the other's needing a new one
  (by Slack's docs it does).
