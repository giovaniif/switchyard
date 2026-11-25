# Changelog

Each version is a [GitHub release](https://github.com/giovaniif/better-than-claude-code/releases) with the signed Mac app and the Linux packages. The section for a version becomes its release notes.

## 0.20.0 (2026-11-25)

- **Comment on parts of an answer.** Select text in an agent's answer and choose Comment. Each part gets its own card with its own comment, numbered when there are several, and is highlighted in the answer until you send. The agent receives each part with your comment under it, so comments no longer get mixed up.
- **You see what the agent wrote while it worked.** Messages an agent writes between its steps stay in the conversation after the turn ends; only the tool steps fold into a line.
- **Sending scrolls to your message.**
- **Agents look alive.** Faces are friendly 3D characters that breathe, blink, look around, follow your pointer and hop, and look busy while the agent works.
- **Agent names can have spaces**, like "Custom Domains Triage". You mention it as `@Custom-Domains-Triage`; mentions you already wrote keep working.
- **Set up an agent from its own page.** The chat box has the server, model, account, effort and permission pickers. Click the face to change face and colour, or click the name or description to edit them in place.
- **Simpler Slack setup for an agent's own app.** Paste the configuration token, approve in Slack (it comes back to Switchyard on its own, also from the Mac app), then paste the connection token. Renaming an agent renames its Slack bot, and its face becomes the Slack app's icon.
- **Fixed:**
  - Agents' folders no longer show up as recent folders or the default folder in New session.
  - A turn that ran a subagent says "Ran 1 subagent", not "1 other step".
  - Removing an account in use names the sessions that use it, archived ones too.
  - Subscriptions offers Sign in for an account that needs it, and Sign in opens that account's host.
  - Sidebar: a collapsed group shows when one of its sessions waits for approval, and its session count stays readable.
  - Automation cards keep the next run in view when the schedule has a window.
  - On a phone, error banners and the Add host button no longer push their buttons off screen.

## 0.19.0 (2026-11-24)

- **New agents set themselves up.** New agent opens a conversation instead of a form: tell the agent what it's for, and it picks its own name, writes its description and instructions, and chooses a face, colour and model. It can't give itself your project folder, more access or a place in Slack: it asks, and you answer with a button above the reply box. Edit is still there for changes by hand, or just tell the agent what to change.
- **Each agent can have its own Slack app**, so people @mention it by name. Choose Add to Slack in the agent's header:
  1. Paste a Slack configuration token (once per workspace).
  2. Switchyard creates the agent's app.
  3. Approve the install in Slack.
  4. Paste the app's connection token.

  Whoever installed it can ask right away. The shared Switchyard bot on the Connectors page still works as before.
- **Faces come alive.** Agents' faces blink and glance around, look busy while the agent works, and hop when you point at them. They stay still if your system asks for reduced motion.
- **Scout is gone.** The built-in host agent and its conversation are removed.
- **File links open in a workspace viewer** (Markdown, CSV, images, text) with Download.
- **Sidebar rows are one line**, with the harness's logo and a status badge; archive on hover.
- **Fixed:**
  - The agent form's colours line up under the faces and are round again.
  - A conversation whose history was dropped while it kept streaming reloads instead of staying blank.

## 0.18.0 (2026-11-24)

- **Agents.** The Agents page replaces Switchy. An agent is a name you mention (`@Reviewer`) with a colour, what it's for, how it runs (engine, model, effort, folder, permission) and two notes: instructions and memory. Each agent keeps one ongoing conversation. Type `@Name` in any message box to send it there, or start a new chat from its card. **Scout** is built in and answers quick questions about your hosts, read-only, like Switchy did. Switchy's old history isn't carried over.
- **Agents have their own tools**, on every engine:
  - **Needs you:** an agent can notify you. The notification opens its conversation, and its card shows "Needs you" until you look.
  - **Ask another agent:** an agent can message another agent and gets the reply back in its own conversation. Each message is labelled "From @Name" or "Reply from @Name".
  - **Memory:** an agent can update its own memory, which you see in its form.
  - Limits stop runaway loops: 3 notifications an hour, 3 hops per chain, and no messaging itself.
- **Per-agent accounts and agents on a remote host.** The agent form picks which account an agent uses when its engine has more than one. It can also run an agent on a connected remote host, in its own folder there. The agent's tools, @mentions and Slack all work as they do here. When that host is offline, the card says so and sending is refused. Moving an agent to another host starts its conversation fresh there, carrying over the latest messages. Remote hosts need this version too.
- **Slack for agents.** Connect a Slack app on the Connectors page (new Messaging section; the guide includes a manifest to copy). DM the bot or @mention it with an agent's name, and the message goes into that agent's conversation with a "From <you> on Slack" label. The answer is posted back to your Slack thread. Only people you allow can reach your agents; anyone else is ignored and listed for you to Allow or Dismiss.
- **Sessions are named as they start**, from the task, and again once the work is clearer. Sessions that were never named get a name after their next turn.
- **Subagents pane.** Each subagent is its own aligned card: the task, then its type and state, then its latest step.
- **Automations** shows an amber dot when a run is waiting for your approval.
- **Sessions page** search also matches the account name shown on each row.
- **Fixed:**
  - Messaging a session whose folder was removed (for example, a worktree cleaned up after its PR merged) now says the folder no longer exists, instead of a misleading Claude "failed to launch… musl" error.
  - The first terminal opens at its pane's size, so the prompt no longer shows twice.
  - A denied edit reads "Did not edit", and stopping an OpenCode/Grok/Gemini call that was asking for approval reads "1 command not run".
  - When the agent form refuses a name, it says why under the name field.

## 0.17.0 (2026-11-23)

- **Sessions name themselves.** A new session no longer stays titled with the first line of its task. After its first reply, and again after its third, a small model reads the conversation and gives it a short, clear name such as "Fix login redirect loop". It uses Codex `gpt-5.6-luna` at low effort, else Claude Haiku: a few hundred tokens of your plan per name, in a private thread with no tools. A name you set with Rename is never changed, and automation runs keep their names. `YARD_SESSION_NAMES=off` turns it off.
- **Remote and offline hosts.** Sessions on an offline remote host stay listed after a reload, and New session says when the remembered host is offline.
- **Usage.** The usage popover opens on the account whose percent the trigger shows, and an account that needs sign-in shows no stale percent.
- **Phone improvements.** A long account name no longer pushes Log in or the Session setup rows off screen, and tapping a command in Processes shows all of it.
- **Fixed:**
  - Numbered steps split by code blocks keep their numbers.
  - Opening a session with a diagram lands on the latest reply.
  - Tab on a Recent folder in the folder menu opens it.
  - Switchy's All hosts scope shows the cloud icon, not this machine's.

## 0.16.1 (2026-11-20)

- **Clickable links in bold replies.** Markdown links keep their labels and open the right destination when they appear inside bold text. GitHub PR links keep their hover cards.
- **More reliable sessions.** Voice mode continues from the new-session screen into the conversation. Deleting a session and opening a session in a plain folder no longer raise spurious errors. Handovers from pinned sessions stay in their groups.
- **Phone improvements.** Host selection scrolls to the host's details, login actions and delete choices fit their bars, and models without an effort setting no longer display an old effort in Session setup.
- **Clearer status and layout.** Gemini login checks report when sign-in is still needed. Connectors place account rows directly under their headers, session row dividers align, and disabled outlined buttons show their state.

## 0.16.0 (2026-11-19)

- **Collapse Sessions and Workspace.** Hide either desktop sidebar and reopen it from its panel button. Switchyard remembers each panel’s width and collapsed state. Reopening Workspace keeps its tab and terminal output. The Sessions drawer also has a close button on phones.
- **Scroll the Model picker.** Long catalogs scroll in their own list with a visible scrollbar. The Model heading and Speed control stay in place, and reopening brings the selected model into view.

## 0.15.0 (2026-11-19)

- **Hand over a session.** Choose Handover in the session’s sidebar menu to continue with another agent, account, or model. The new subsession uses the same folder, receives a summary of the conversation, and appears beneath the original in the sidebar.
- **Dropdowns fit the window.** Effort and Codex speed descriptions fit inside their rows, and speed opens within the Model picker. Connectors and Automations use Switchyard’s own menus. Workspace tabs have more space, and the Connectors refresh icon lines up with its label.
- **Open old conversations without a missing-folder banner.** If a session’s folder was removed, the error appears in Terminal with a retry button, and the conversation remains accessible.

## 0.14.0 (2026-11-19)

- **Codex speed.** Choose Standard, Fast, or Ultrafast in the model menu when the selected host reports those options. Speed changes apply to the next turn.
- **Steer queued messages.** Send one queued message into the current Claude or Codex turn while the other messages stay queued.
- **See subagents in Workspace.** Open Subagents beside Terminal to see status, steps, and reports from Claude and Codex when available.
- **Rename sessions.** Use Rename in the session menu to change the title across clients. Conversation history and the worktree stay with the session.
- **Clearer Usage and navigation.** Usage opens above its sidebar row on desktop and as a bottom sheet on phones. It shows reset times and warns on individual limits. Open Hosts from a host status row or page search.
- **Cleaner terminal scrollback.** Reloading a terminal no longer feeds replies to historical terminal queries back into the live shell.
- **Readable composer controls.** Narrow composers group settings before model, account, effort, or access labels get squeezed.

## 0.13.0 (2026-11-18)

- **Simpler Workspace.** Terminal opens first, and Processes sits beside it. The conversation header links to the branch’s pull request on GitHub.
- **One composer toolbar.** Attach, dictate, use voice, and send from one row. In narrow windows, Setup holds the model, account, effort, and permissions. Full access is marked in amber.
- **Clearer account controls.** Usage shows the current account’s limits and reset times first. Connectors are grouped by account, with status, filters, and an Add connector action.
- **Quieter session setup.** A remembered offline host stays selected without an error banner. Web links in agent replies open in the browser.

## 0.12.0 (2026-11-18)

- **Active hours for automations.** Limit an interval to a local time window, including overnight hours and selected start days. Hourly 21:00–09:00 runs from 21:00 through 08:00; daytime hours create no sessions, even after a restart. The form and cards show the hours and real next slot; Run now remains available anytime. Loop patrol now uses this overnight window once the server is updated and loop automations are applied.

## 0.11.0 (2026-11-18)

- **Gemini CLI joins the engines.** Start Gemini sessions alongside Claude Code, Codex, OpenCode and Grok, with streaming replies, tools and diffs, approvals, permission modes, Stop, resume and hand-offs. Install `@google/gemini-cli` on the host; Switchyard reuses its login and discovers its models. Added accounts keep separate logins. For a fresh Google login, Hosts shows the terminal command and checks it when you return. Gemini does not report plan limits or offer an effort control here.
- **Dedicated Hosts, Sessions and Subscriptions pages.** Browse hosts and open one host's details, manage engine accounts separately, find sessions with tabs and search, and inspect subscription limits across hosts. The sidebar's compact Usage row opens a summary with a link to the full Subscriptions page. Empty session searches offer Clear search.
- **Save defaults deliberately.** Settings keeps independent default hosts, models and efforts for new sessions and for Switchy. A one-off selection does not overwrite them. Removing a default host falls back to an available host while keeping your saved model when possible.
- **More control over Switchy.** Host and source scope, model and effort, voice and access controls work together; questions wait when their inference host is busy. Answers show readable model names. A stopped answer is a quiet note, while a refused question keeps its reason visible in red under the message box.
- **iPhone and iPad companion foundation.** A SwiftUI app loads your privately reachable Switchyard host, with native voice and local notifications, simulator tests and a TestFlight build workflow. Hosts always offers phone setup, and the session's Workspace sheet gives phones access to Changes, Terminal, Pull request and Processes. A TestFlight invitation appears when one is configured.
- **Attachments stay with your message.** Editing a queued message or stopping its turn restores its files to the reply box. Selecting more than 20 files explains the limit and disables Send rather than losing the message. On phones, a long attachment list scrolls within its own row so Send stays reachable.
- **Clearer process controls.** Stop with children fits inside the process table, including on phones. Opening Processes for an offline host says it is offline instead of leaving a stale process list on screen.
- **More reliable turn history.** Work durations exclude time spent waiting for your approval. OpenCode and Grok output arriving after a stopped turn is dropped instead of leaking into the next answer. Long first prompts get useful titles, including prompts that begin with a long URL.
- **Layout fixes across desktop and phone.** Connectors' filter tabs stay on one row; folder and effort menus stay on screen; long host, folder and account names preserve their icons and chevrons. Desktop account chips show the whole name, and Import always shows the last-used age on phones. Host and session rows align with their pages, Workspace lines up with Delete, and disabled primary buttons look disabled.
- **Release and verification maintenance.** Releases now prepare their version change in a PR and publish only the tag after it is merged, preserving required CI on `main`. Verification recipes and session routing were repaired, and failed post-merge jobs get one retry before a revert is proposed.

## 0.10.0 (2026-11-18)

- **Connectors, focused on what you use.** The page opens on connected services for one host and engine account. Unused entries stay under Available; search, status filters and eight-row pages keep long catalogs manageable. Sign-in progress stays visible through completion.
- **New sessions remember your setup:** the last server, source project, model and effort, including remote sessions and after a reload. Changing model or effort in an existing chat updates the defaults too. A temporarily offline server stays selected with Send disabled.
- **Mark chats unread.** A chat's menu offers Mark as unread and Mark as read. The blue dot, bold title and Unread label survive reloads, even for the open chat; opening it deliberately clears the mark.
- **Aligned host cards.** Engine login notes no longer push one card's footer below its neighbours. Closed cards share a row height; Details still opens independently.
- **Merged worktrees clean themselves up.** Clean worktrees whose pull request was merged are removed automatically across repositories, together with their branch. Running sessions, changed files and folders still used by a terminal or dev server are kept; conversation history stays.
- **Switchy:** the same message box as chats, a working Cmd-K entry, and a fix for startup failures with some Codex connector configurations.
- **Fixed:**
  - A remembered project folder that was deleted says it no longer exists and disables Send before a session is started.
  - Stopped turns that only wrote text now say "Stopped after …", including steered turns.
  - On mobile, the delete-session confirmation stays on one line beside a long title.

## 0.9.0 (2026-11-17)

- **Every account's limits, clearly.** The sidebar names each account once an engine has more than one ("Claude · Default", "Claude · Work"), each over its own 5-hour and week meters; an account with nothing reported yet says so. Claude limits no longer wait for a session: Switchyard reads each Claude account's limits from Claude Code's own usage report, with no prompt and no usage, when it starts, every 10 minutes, and as soon as an account is added or logs in.
- **The model picker names its account.** It shows models only (the limits moved to the sidebar) and says which account they run on, with the account choice beside "Account"; the closed picker says it too ("Opus 5.5 · Work").
- **Effort is its own dropdown,** next to the model, in the new-session box, in a session and in the automation form.
- **Hosts as a grid.** The Hosts page is a grid of small cards like Automations: each host's state, CPU, memory, disk, engine logins and sessions at a glance; Details opens the sessions table, accounts and host actions inside the card.
- **Switchy:** quick questions about your hosts and sessions, in a conversation of their own that stays between visits (`#/assistant`), answered by your default Claude or Codex account with evidence from This Mac, one remote or all hosts.
- **Go to (Cmd-K):** jump to any session or page from anywhere. On the Mac, Settings and Import move to the app menu.
- **Mermaid diagrams** in an answer render as diagrams, with the source a click away.
- **Fixed:**
  - Cmd-K's recent list shows your sessions, not archived automation runs.
  - Switchy: a removed host's saved scope falls back to All hosts instead of refusing every question.

## 0.8.0 (2026-11-17)

- **Connectors.** A new Connectors page (plug icon in the sidebar) lists every MCP server each engine account has on each host: claude.ai connectors, plugins, your config files and, for Codex, ChatGPT apps. Each one says whether it works: Connected, Needs sign-in, or Not working with the reason. Checking spends no usage.
  - **Connect** signs a server in from the app. A claude.ai connector opens claude.ai and turns Connected by itself. A server that ends its sign-in on a page of the server you can't reach asks you to paste that page's address. Running sessions pick the server up without a restart.
  - **Add connector** adds Linear, Sentry, Datadog, PostHog, Notion, Atlassian or Vercel, or any https address, to Claude Code or Codex for any account, then opens its sign-in. This is how an added work account gets the connectors your Default one has from plugins. Remove takes one out.
- **Charts from connector tools.** When an MCP tool comes with an interactive view (MCP Apps), such as a chart, it shows in the thread under the call and stays in view when the turn folds. It runs sandboxed: it can't reach the page, only the sites its server declares, and only its own server's tools (anything that isn't read-only asks you first on Claude).
- **Hover cards on PR links.** A GitHub pull request link in an answer is a real link, and resting on it shows the PR's state, checks, review status, size, author and age, through the host's own `gh`.
- **Hosts:** each host's Sessions list starts collapsed to one line (how many sessions, their CPU and memory, how many are queued). Click it to see the table.
- **Fixed:**
  - Archiving the session you're looking at no longer opens the Archived list.
  - The PR pane's title opens the pull request on GitHub, like the number beside it.
  - Review with an engine waits while a session works in its folder, instead of being refused.
  - A refused new session names the session in the way, not a "New worktree" box that is gone.

## 0.7.0 (2026-11-17)

- **Stop a session's processes.** The Processes tab has a × on each row: a second click stops the process, Force kill follows if it is still running, and Stop with children takes its whole branch. Only the session's own processes can be stopped, never the agent itself, a process another session shares, or anything else on the machine.
- **Automations as a compact grid.** Small cards show the schedule, the next run, and either the run that is working or how the last one went. A strip of marks lists the recent runs; each opens its run. Runs is the full list, with pinning.
- **Verifier and babysit runs leave the sidebar.** They now run under "Loop verify" and "Loop babysit" on the Automations page, as automations with a new **Manual** schedule that only run when started.
- **Your drafts stay.** A half-written reply stays in its session when you switch away and back, and a half-typed new-session task stays in its box.
- **Fixed:**
  - After Send, the "Working" line is no longer cut off at the bottom of the thread.
  - A command waiting for approval reads "Waiting to run", and a denied one reads "Did not run", not "failed".
  - Changes pane: a renamed file reads old → new. A session in a subfolder lists every new file.
  - OpenCode and Grok: after Stop, a stuck turn's text no longer runs into the next answer. A queued message says it is sent when the turn ends.
  - Adding a host by IP address names it by the whole address, so a second tailnet host is no longer refused.
  - Review with an engine works in a repository without a remote, and stays when the pull request cannot be read.
  - Hand-off notes keep their link when an earlier hand-off's session is deleted.
  - Mobile: the Send button, Add account and the worktree delete confirmation stay in place.

## 0.6.0 (2026-11-16)

- **OpenCode and Grok as engines.** Start sessions on OpenCode (`opencode-ai`) or xAI's Grok CLI (`@xai-official/grok`) next to Claude Code and Codex: streaming, thinking, tool calls, diffs, approvals, permission modes, Stop, resume and added accounts. Install them on a host with `npm i -g opencode-ai @xai-official/grok`; Switchyard lists an engine only where it is installed. Neither reports plan usage, so they have no limit meters. A message sent mid-turn waits for the next turn.
- **Log in to them from Hosts.** Grok: **Log in** shows a link and a code. OpenCode works with its free models as it is; **Add provider** lets you paste a provider's API key or use its sign-in. Keys stay in OpenCode's own files, not in Switchyard.
- **Voice in the chat.**
  - **Dictation:** the mic next to the paperclip writes what you say into the box.
  - **Voice mode:** the waveform button next to it is hands-free. It sends after a short pause, reads the answer aloud (code and tables are skipped), then listens again. Approvals are announced, never answered by voice. Talking over an answer stops it.
  - **Where it works:** the Mac app asks for Microphone and Speech Recognition the first time. Also Chrome, Edge and Safari; in Firefox the mic is greyed out with the reason.
- **Fixed:** a changed binary file in the Changes pane says binary, not +0 −0.

## 0.5.0 (2026-11-16)

- **Fixed: "Open the sign-in page" did nothing in the Mac app**, and neither did pull request or notification links. The window dropped links that open a new tab; they now open in your browser.
- **Check for Updates…** in the Switchyard menu, under About. It always answers: you're on the latest version, a download has started, or the check failed.
- **A host on an older Switchyard says so.** Adding an account there said `unknown op 'account.add'`; it now names the host and says to use Update Switchyard on its card in Hosts.
- **Notifications say what happened, and only the important ones show.** "Needs your approval · Run a command: npm test", "Done in 2m 4s · " followed by the start of the answer, "Failed after 12s · " followed by the error. A turn under a minute, or an automation run's finish, no longer notifies; approvals and failures always do.
- **Session rows show what needs you.** A turning ring while working, an amber "!" while waiting for approval, a blue dot and bold title for an answer you haven't opened yet ("Answered"), red for a failure.
- **Reply to part of an answer with quote cards.** Reply puts the quote in a card above the box instead of `>` lines; several stack and × removes one. Quotes in your sent messages show as quotes.
- **Attach terminal output to a reply.** The Terminal tab's Attach menu adds the selection, the rows on screen, or the last 100 or 500 lines as a card above the reply box. It is sent as a code block, and shows as one in your message.
- **Drag sessions between groups.** Drop a row on a group, on Sessions to take it out, or on Pinned. On a phone the row's … menu still does it.
- **Settings page.** Notifications moved there from the sidebar. Choose which hosts show in the sidebar, so you can hide This Mac.
- **Claude limits from your other hosts.** The sidebar's limits strip reads every host shown there, so Claude limits from a server show on the Mac too. Added accounts like "Work" get their own line.
- **Fixed:**
  - A hand-off keeps the session in its sidebar group, and hand-off notes open the other session.
  - Editing an automation keeps its engine and model; Run now shows a failure reason once.
  - Agent headings and lists render even with no blank line before them.
  - "Show more (1 line)", not "(1 lines)".

## 0.4.3 (2026-11-16)

- **Fixed: Install or Update Switchyard on a server failed at once** with "Cannot execute command-line and remote command" when your ssh config sets a `RemoteCommand` for that host (a tmux attach, say). Switchyard's own ssh connections now leave it and any forced terminal out; your own logins keep them.
- **Tables in answers.** A Markdown table from the agent shows as a table, not as lines of pipes.
- **Fixed:**
  - An approval nobody answered says "Not answered" once its turn ends, not "Denied".
  - The local host shows its own name right after Switchyard starts, not "This Mac".
  - With no memory limit, the Hosts page's queue rule names only the load, not "less than 0 GB free".

## 0.4.2 (2026-11-16)

- **Fixed: an update could leave the app showing the previous version's screens.** The app's window kept an old copy of the page in its cache and didn't check for a new one. It now checks on every load, so an update shows right away.

## 0.4.1 (2026-11-16)

- **Works on a phone.** Below 700 px the sessions sidebar is a drawer: a menu button at the top of each page opens it, and picking something, tapping beside it or Escape closes it. The session, Hosts and Automations pages get the full width instead of a 140 px column. Desktop is unchanged.
- **The Changes pane shows new files.** A file the agent created shows its lines like any other change, a binary one says "No text diff", and with many files the pane scrolls instead of squashing the cards. File names with accents or other non-ASCII letters show as written, not as `\303\247` escapes.
- **A session whose folder was removed says so.** The Changes and Pull request panes and the terminal say the folder no longer exists, instead of "Not a git repository", and no terminal quietly opens in your home folder.
- **Terminal.** Reloading with a shell that already exited no longer shows a "terminal is closed" error. Closing the last terminal leaves an "Open a terminal" button instead of a blank pane.
- **Automations.** A run shows how long it ran, not how long since it last changed (pinning or replying no longer inflates it). Claude automations skip a run when usage is above their limit.
- **Fixed:**
  - Claude usage meters survive a restart.
  - The Notifications switch shows Off, and why, when the browser's permission was revoked.
  - Picking a Codex account in a new session switches it to Codex.
  - A refused "Add host" or "Add account" keeps what you typed.
  - Double clicks on Start anyway, Send to session or Review no longer act twice.
  - A reload or Back keeps you on the Automations page.
  - A window that connects while something changes no longer loses that change.
  - The folder picker says when a typed path matches no folder.

## 0.4.0 (2026-11-16)

- **Automations.** Schedule a prompt on a repository: every N minutes or hours, daily or weekly at a time, with its engine, model and permission mode. Each run gets its own worktree, is archived when it ends, and is listed under its automation on the new Automations page, so runs don't fill the sidebar. Run now, pause, edit; optionally skip a run while the last one is going or when usage is high.
- **Several accounts per engine.** Add a second Claude Code or Codex login (say, your company's) on any host from the Hosts page, log in to it from the app, and pick the account per session. Each account keeps its own login and usage; your existing login stays the Default.
- **Import Claude Code sessions.** "Import" in the sidebar lists the Claude Code sessions on a host; importing one opens it with its history, and your next message continues the same conversation.
- **The message queue works like Claude Code's.** A message you send while the agent works reaches it at its next step, in the same turn, and shows in the thread where it went in. Several queued messages go as one. Stop puts what's queued back in the reply box; Edit (or ↑ in an empty box) takes one back.
- **OS notifications.** Turn on Notifications in the sidebar to get a system notification when a session needs an approval, finishes or fails while you're looking elsewhere; clicking it opens the session.
- **Usage you can see.** Meters for the 5-hour window and the week, with when each resets, in the sidebar and the model picker. The model picker is tidier: the other engine folds into one row.
- **Subagents and processes.** A Claude subagent shows as one entry under the call that started it, with its own steps and report. A Processes tab lists what the session is running right now.
- **Group sessions.** "Move to group…" in a session's menu; groups fold, can be renamed or ungrouped. Archived stays at the bottom of the sidebar.
- **Fixed:** a reload or Back keeps you in the session you had open; a quick approval no longer leaves a session stuck as running; a double click on Allow once no longer shows an error; usage meters read 0% once a window has reset.

## 0.3.0 (2026-11-15)

- **Reply to part of an answer.** Select text in one of the agent's answers and a Reply button appears over it. It puts the selection in the reply box as a quote, ready for your reply underneath.
- **Queue messages while the agent works.** The reply box stays open during a turn: "Add to queue" lines up your next message, shown above the box with a button to remove it, and it goes once the turn finishes. Several queued messages go in order. Stop keeps them waiting instead of sending them.
- **Fixed: the terminal after a reload.** Reloading the page (or reopening the web version) could bring a session's terminal back garbled: a long prompt overwritten by the next command, or a command's output missing. The terminal now waits for its real size before it replays anything.
- **Fixed: terminal keys out of order.** Fast typing could reach the shell with two keys swapped (`TSET` for `TEST`). Keys are now sent in the order they were typed.

## 0.2.3 (2026-11-15)

- **Fixed: the session menu stays on screen.** With a narrow sidebar, or on a row near the bottom of the list, the … menu (Pin, Archive, Delete) was cut off. It now opens over the rest of the window, above the row when there's no room below.
- **The app updates itself.** It checks for a new release at launch and every 6 hours, installs it in the background, and asks before restarting. The restart also restarts the background service, so "an older daemon is running" goes away; sessions are kept, a running turn stops. This is the last version you need to download by hand.
- **A server's Switchyard in the browser, as your own user.** `YARD_BIND` now takes a comma-separated list, so a yardd installed on a server (as an SSH host) can keep loopback for the app and also listen on one address a reverse proxy reaches. Its sessions and terminals then run as you, with your `gh`, SSH keys and tools, instead of as the container's `yard` user. See "Also in a browser" in docs/remote-hosts.md.

## 0.2.2 (2026-11-15)

- **No more memory cap.** Switchyard no longer stops a session for the memory its processes use. The Hosts page still shows each session's CPU and memory, and new sessions still wait while a machine is under pressure. `YARD_SESSION_MEM_CAP_GB` is gone.

## 0.2.1 (2026-11-15)

- **Fixed: Stop always stops.** If Claude Code or Codex doesn't end the turn within 5 seconds, Switchyard ends it. A stuck Claude process is ended and the next message resumes the conversation.
- **Fixed: a session whose Claude Code process died** no longer swallows the next message and stays "Working" forever. The next message starts Claude Code again on the same conversation.
- **Fixed: the memory cap on Linux** counted every thread as a separate process. A Claude session looked many times its real size (an "8.7 GB, over the 8 GB cap" stop at well under 1 GB), and the stop killed Claude Code itself instead of only the commands it ran.
- **Web version and desktop app on the same server.** The compose file now publishes the port on the server's loopback, so adding that server as an SSH host in the desktop app shows the web version's sessions instead of offering to install a second Switchyard. The installer now refuses to install over a Switchyard it didn't set up.

## 0.2.0 (2026-11-15)

- **Log in from the app.** When Claude Code or Codex isn't logged in on a machine, the new-session page and its sessions say so, with **Log in**. Codex shows a link and a code; Claude Code shows a sign-in link and takes the code it gives back. `/login` in a session opens the same dialog, and the Hosts page shows each engine's state.
- **Thinking.** Claude's and Codex's thinking stream into the thread as a one-line preview that opens to the full text.
- **A calmer thread.** Tool calls between messages fold into one line ("Read 3 files, ran 2 commands"). A running turn shows its latest step under "Working for 12s"; a finished turn folds its work into "Worked for 1m 12s" above the answer.
- **Fixed:** a reply could appear above the question it answered after the Claude sidecar restarted (for example after logging in). Threads recorded that way now show in order too.
- **Pin, archive and delete** sessions from the sidebar: the … button on a row, or right-click.
- **Resizable sidebars:** drag the edge of the session list or the workspace pane.
- **Web version:** set `PROJECTS` to a folder on the server and it is mounted at the same path; the folder picker lists it first. Codex retries no longer show as errors.
- A favicon.

## 0.1.0 (2026-11-15)

The first release.

- **Claude Code and Codex in one window.** Streamed answers, commands, edits as diffs and approvals inline, the same for both engines. Switch models mid-session; picking the other engine's model hands the conversation off with a summary.
- **A worktree per session.** Every session in a git repository gets its own branch and worktree, with a folder picker like Claude Code's. Changes, Terminal and Pull request panes sit next to the thread.
- **Permission modes.** Ask first, accept edits, plan only or full access, per session, switchable at any time.
- **Files and images.** Paste, drop or attach them to a prompt; they reach remote agents too.
- **Remote hosts.** Add any machine you can `ssh` into and install Switchyard there from the app. Its sessions show up in the same sidebar and keep running when your Mac sleeps.
- **Guard rails.** CPU, memory and disk for every host on screen; new sessions wait while a machine is under pressure, and a runaway session is stopped.
- **Sessions survive restarts.** yardd keeps every session's events in SQLite and resumes the engines when it starts again.
- **Web version.** One Docker image for Coolify or Docker Compose.
