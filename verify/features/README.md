# Feature map

What "working" means for Switchyard, area by area, as recipes the CLI runs.
`verify/yv map run <area>` runs one area in its own isolated yardd
(`map-<area>`); `verify/yv map run all` runs every area; `verify/yv map status`
shows the last result of each recipe (pass, fail, needs-var, manual).

Every area file has: a paragraph on what must be true, **Setup** and
**Cleanup**, **Recipes** (a `### label`, then what to expect at both viewports
and what to read back, then fenced `sh` blocks of `yv` commands; a block fails
when any command exits non-zero), and **Gotchas**. `(manual: why)` marks a
flow the CLI cannot drive. `Needs: VAR` marks a recipe that runs only when the
variable is set. `Viewports:` says which viewports a recipe covers when it is
not both. A recipe never contains three backticks in a row, even inside a
JSON string: the first one ends its `sh` block. Write them as `\u0060` in the
JSON (a message with a fenced code block, for example).

Data is synthetic: the seeded repositories, sessions and PR are all named
`TEST-…`, the engines are the scripted fakes in `crates/yardd/tests/fake-codex`,
`gh` and `ssh` are the fakes in `verify/fakes`.

Rules: never weaken a recipe to make it pass. A failing recipe is either map
drift (fix the recipe to match what the app really does) or a product bug
(keep it failing, add a Gotcha, queue a finding). Every fixed bug adds a
recipe with its repro.

A `hasText` target matches case-insensitively, as a substring: a check that
some output is absent (`"count":0`) must use words the prompt itself does not
contain, or the user's own message matches it (`"Late"` matched a `[late] …`
prompt).

| Area | What it covers |
|---|---|
| [mobile](mobile.md) | iOS setup discovery, successful connection status and phone workspace navigation |
| [new-session](new-session.md) | The new-session page: prompt, model, effort, permission, folder, host, base branch, worktree note |
| [session-defaults](session-defaults.md) | New sessions remember the last host, project, model and effort, per host and across a reload |
| [launch-defaults](launch-defaults.md) | Saved model, effort and host defaults for New sessions, Reset, a removed default host |
| [thread](thread.md) | How a session's turns render: replies, thinking, edits, failures, the work-log fold |
| [mermaid](mermaid.md) | Mermaid fences drawn as diagrams, the source kept, invalid and incomplete diagrams, long diagrams on phones |
| [approvals](approvals.md) | Command approvals: allow once, always, deny, and the sidebar status |
| [stop](stop.md) | Stopping a running turn, including one the engine ignores |
| [processes](processes.md) | The Processes tab: the session's process tree, refreshed while visible |
| [terminal](terminal.md) | The Terminal tab: shells per session, tabs, closing a tab, scrollback, attaching output to a reply |
| [approved-design](approved-design.md) | The right pane, single-row composer, account connector cards and usage panel |
| [pr-pane](pr-pane.md) | The conversation header’s external pull request link |
| [sidebar](sidebar.md) | Session rows: order, status, names (generated, renamed), pin, archive, restore, delete |
| [navigation](navigation.md) | Command-K: sessions and pages, Agents (Switchy's old name finds it), repo and branch names, automation runs left out; the macOS app menu |
| [persistence](persistence.md) | Sessions and history across a yardd restart |
| [focused-pages](focused-pages.md) | Approved host, session (search, Active/All/Archived tabs) and subscription pages, account identity and compact usage popover |
| [hosts](hosts.md) | The Hosts page (compact host rows and a tabbed inspector), adding a direct host, an unreachable SSH host, removing a host |
| [queue](queue.md) | The resource guard: sessions queue under pressure, Start anyway |
| [logins](logins.md) | Engine login state and the Codex, Claude, Grok and OpenCode login flows |
| [acp-engines](acp-engines.md) | OpenCode and Grok: listed where installed, sessions, approvals, hand-off rows |
| [attachments](attachments.md) | Files with a prompt: upload, chips, thumbnails, size cap |
| [voice](voice.md) | Dictation in the composer, hands-free voice mode (send after a pause, answers read aloud, approvals announced), the unavailable and blocked states |
| [models](models.md) | Model and effort switches, Codex speed, permission modes, hand-off to another engine |
| [worktrees](worktrees.md) | A worktree per session, in-place folders, removing a worktree |
| [notifications](notifications.md) | OS notifications: the switch on the Settings page, when a session or the `notify` op shows one |
| [settings](settings.md) | The Settings page: notifications, which hosts the sidebar shows, the usage strip from the shown hosts |
| [security](security.md) | Origin checks on the WebSocket, loopback binding |
| [import](import.md) | Importing Claude Code sessions from the host, with history and resume |
| [automations](automations.md) | Scheduled runs: the Automations page, create (daily, interval, weekly), Run now, auto-archive, pin, edit, disable, delete, the working-run dot |
| [agents](agents.md) | Agents: cards with living faces (blink, busy), New agent set up in its conversation with its `update_setup` tool, what it asks for, the edit form with colours under the faces, @mentions that route a message, the one conversation, agents messaging each other (badges), an agent that needs you, an agent's own memory, Add to Slack's first step, remote agents, Switchy's old address |
| [layout](layout.md) | Viewport behaviour: desktop, the narrow pane cut-off, mobile |
| [dropdowns](dropdowns.md) | Menus inside the viewport: composer options, workspace tabs, custom schedules, connector host, handover under the same agent |
| [usage](usage.md) | Usage popover, Subscriptions, reset times and separate account/model controls |
| [accounts](accounts.md) | More than one login per engine: add, log in, pick for a new session, remove |
| [connectors](connectors.md) | MCP servers per engine account: states, sign-in (claude.ai, pasted redirects, ChatGPT apps), chart widgets in the thread |
| [slack](slack.md) | The Slack connector: setup guide and manifest, refused and good tokens, allowing someone who wrote, the default agent, answers posted in the Slack thread, the Slack badge in a conversation, tokens kept out of the page and logs |
