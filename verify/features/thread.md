# Thread

A session's page shows its turns in order: the prompt as a bubble, streamed
text that settles into the final answer, thinking and tool calls folded into a
work log ("Worked for 3s"), edits as diffs, and an engine error as a red line.
Follow-up messages append a new turn. Both engines render the same way.

## Setup

```sh
yv iso up
yv iso seed
```

## Cleanup

```sh
yv iso down
```

## Recipes

### reply-renders
Expect at both viewports: the prompt and the fake's reply "Echo: TEST-say hello".

```sh
yv drive 'session:{{sid_echo}}' --steps '[{"expect":"css:.thread","text":"TEST-say hello"},{"expect":"css:.thread","text":"Echo: TEST-say hello"},{"shot":"reply"}]'
```

### markdown-table-renders
Viewports: both
An agent's Markdown table renders as a table (header cells, one row per line), not as raw `| a | b |` text, and a wide one scrolls inside the thread instead of widening the page at 390 px.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"TEST-table\n\n| File | Result | Note |\n|---|:---:|---|\n| src/a.ts | pass | |\n| src/b.ts | `fail` | expected 3 tokens, got 2 in the long-running integration suite for the parser |"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_echo}}' --steps '[{"expect":{"css":".prose table th","hasText":"Result"}},{"expect":{"css":".prose table td","hasText":"src/b.ts"}},{"expect":{"css":".prose table td code","hasText":"fail"}},{"expect":{"css":".prose table tr"},"count":3},{"expect":{"css":".prose","hasText":"|---"},"visible":false},{"eval":"document.documentElement.scrollWidth <= window.innerWidth","equals":true},{"shot":"table"}]'
```

### markdown-blocks-without-blank-lines
Viewports: both
A heading or a list right under a line of text, with no blank line between them, still renders as a heading and a list, not as raw `## ` and `- ` text.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"TEST-blocks\n\n## Summary\nFixed the parser.\n\nChanges:\n- src/a.ts\n- src/b.ts"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_echo}}' --steps '[{"expect":{"css":".prose h3","hasText":"Summary"}},{"expect":{"css":".prose li","hasText":"src/b.ts"}},{"expect":{"css":".prose p","hasText":"Changes:"}},{"expect":{"css":".prose","hasText":"## Summary"},"visible":false},{"expect":{"css":".prose","hasText":"- src/a.ts"},"visible":false},{"shot":"blocks"}]'
```

### web-links-open-outside-the-thread
Viewports: both
A labeled web link and a bare URL open in a new tab. A local file link is a
download button (see `file-links-download`); inline code and other schemes stay
inert.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"TEST-links\n\n[Review and comment on the Claude Design prototype](https://claude.ai/artifact/example).\n\n(https://example.com/path?mode=review).\n\n[Local file](/tmp/example.txt) [Script](javascript:alert%281%29) `https://example.com/code`"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_echo}}' --steps '[{"expect":"role:link:Review and comment on the Claude Design prototype"},{"expect":"role:link:https://example.com/path?mode=review"},{"eval":"[...document.querySelectorAll(\".thread .prose a\")].map(a => [a.getAttribute(\"href\"), a.target, a.rel])","equals":[["https://claude.ai/artifact/example","_blank","noopener noreferrer"],["https://example.com/path?mode=review","_blank","noopener noreferrer"]]},{"expect":{"css":".thread .prose button.file-link","hasText":"Local file"}},{"expect":{"css":".thread .prose code","hasText":"Script"}},{"expect":{"css":".thread .prose code","hasText":"https://example.com/code"}},{"shot":"web-links"}]'
```

### thinking-folds
A finished turn with thinking folds into "Worked for …"; opening it shows the
thought's first line.

```sh
yv drive 'session:{{sid_think}}' --steps '[{"expect":"css:.thread","text":"Thought it through."},{"click":"role:button:Worked for"},{"expect":"css:.thread","text":"Reading the tests"},{"shot":"thinking-open"}]'
```

### edit-shows-diff
An edit shows the file and its diff lines inline.

```sh
yv drive 'session:{{sid_edit}}' --steps '[{"expect":"css:.thread","text":"src/lib.rs"},{"expect":"css:.thread","text":"fn added() {}"},{"expect":"css:.thread","text":"Edited src/lib.rs."}]'
```

### failed-turn-shows-error
The engine's error shows in the thread.

```sh
yv drive 'session:{{sid_fail}}' --steps '[{"expect":"css:.error-line","text":"fake failure: the model is unavailable"}]'
```

### follow-up-turn
A reply typed in the session runs a second turn in the same thread.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"fill":"label:Message","value":"TEST-second turn"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-second turn"}]'
yv state --expect '{"sessions":[{"id":"{{sid_echo}}","status":"idle"}]}'
```

### claude-session-renders
A Claude session (scripted sidecar) renders like a Codex one.

```sh
yv drive 'session:{{sid_claude}}' --steps '[{"expect":"css:.thread","text":"Echo: TEST-hello from Claude"}]'
```

### reply-to-part-of-answer
Selecting part of an agent's answer shows a Comment button over the selection;
pressing it puts the selection in a quote card above the reply box (not into
the text) with its own comment field, which takes the caret. A second Comment
stacks a second card, × removes one. Sent without a comment, each quote goes
before the text as a Markdown blockquote (the wire format engines get: the fake
echoes it back), and the sent bubble shows the quote as a quote, not as `>`
characters. Selecting your own message shows no button.

```sh
yv drive 'session:{{sid_echo}}' --steps '[{"expect":"css:.thread","text":"Echo: TEST-say hello"},{"eval":"(() => { const p = [...document.querySelectorAll(\".thread .prose p\")].find((e) => e.textContent === \"Echo: TEST-say hello\"); p.scrollIntoView({ block: \"center\" }); const r = document.createRange(); r.selectNodeContents(p); const s = getSelection(); s.removeAllRanges(); s.addRange(r); return s.toString(); })()","equals":"Echo: TEST-say hello"},{"expect":{"role":"button","name":"Comment","exact":true},"inViewport":true},{"shot":"reply-button"},{"click":{"role":"button","name":"Comment","exact":true}},{"expect":{"css":".composer .quote-card"},"text":"Echo: TEST-say hello"},{"expect":"label:Message","value":""},{"eval":"document.activeElement === document.querySelector(\".quote-card textarea[aria-label=\\\"Comment on this part\\\"]\")","equals":true},{"expect":{"role":"button","name":"Comment","exact":true},"visible":false},{"eval":"(() => { const p = [...document.querySelectorAll(\".thread .prose p\")].find((e) => e.textContent === \"Echo: TEST-say hello\"); p.scrollIntoView({ block: \"center\" }); const t = document.createTreeWalker(p, NodeFilter.SHOW_TEXT).nextNode(); const r = document.createRange(); r.setStart(t, 11); r.setEnd(t, 20); const s = getSelection(); s.removeAllRanges(); s.addRange(r); return s.toString(); })()","equals":"say hello"},{"click":{"role":"button","name":"Comment","exact":true}},{"expect":{"css":".composer .quote-card"},"count":2},{"shot":"two-quotes"},{"click":{"role":"button","name":"Remove quote","within":{"css":".quote-card","hasText":"Echo: TEST-say hello"}}},{"expect":{"css":".composer .quote-card"},"count":1},{"expect":{"css":".composer .quote-card"},"text":"say hello"},{"expect":{"css":".composer .quote-card","hasText":"Echo: TEST"},"visible":false},{"fill":"label:Message","value":"TEST-about that"},{"shot":"quoted"},{"click":{"role":"button","name":"Send","exact":true}},{"expect":{"css":".prose p","hasText":"Echo: > say hello"}},{"expect":{"css":".bubble .bubble-quote","hasText":"say hello"}},{"expect":{"css":".bubble","hasText":"TEST-about that"}},{"expect":{"css":".bubble","hasText":"> say"},"visible":false},{"expect":{"css":".composer .quote-card"},"visible":false},{"expect":"label:Message","value":""},{"shot":"sent"}]'
```

### comments-go-with-their-parts
Comments on two parts of an answer stay apart (owner: with several parts
quoted, the agent could not tell which remark was about which). Each card is
numbered and has its own comment; the quoted parts are marked in the answer
while they wait. Sent, the message is one numbered item per part with its
comment under it, after the header "Comments on parts of your answers:" (the
fake echoes it), the bubble shows each part with its comment, and the thread
scrolls to the bottom to show it.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"expect":"css:.thread","text":"Echo: TEST-say hello"},{"eval":"(() => { const p = [...document.querySelectorAll(\".thread .prose p\")].find((e) => e.textContent === \"Echo: TEST-say hello\"); p.scrollIntoView({ block: \"center\" }); const t = document.createTreeWalker(p, NodeFilter.SHOW_TEXT).nextNode(); const r = document.createRange(); r.setStart(t, 0); r.setEnd(t, 4); const s = getSelection(); s.removeAllRanges(); s.addRange(r); return s.toString(); })()","equals":"Echo"},{"click":{"role":"button","name":"Comment","exact":true}},{"fill":"label:Comment on this part","value":"TEST-why echo?"},{"press":"Enter"},{"eval":"document.activeElement === document.querySelector(\"textarea[aria-label=Message]\")","equals":true},{"eval":"(() => { const p = [...document.querySelectorAll(\".thread .prose p\")].find((e) => e.textContent === \"Echo: TEST-say hello\"); p.scrollIntoView({ block: \"center\" }); const t = document.createTreeWalker(p, NodeFilter.SHOW_TEXT).nextNode(); const r = document.createRange(); r.setStart(t, 11); r.setEnd(t, 20); const s = getSelection(); s.removeAllRanges(); s.addRange(r); return s.toString(); })()","equals":"say hello"},{"click":{"role":"button","name":"Comment","exact":true}},{"expect":{"css":".composer .quote-card-number"},"count":2},{"fill":"label:Comment on part 2","value":"TEST-say it twice"},{"eval":"(() => { const h = globalThis.CSS?.highlights?.get(\"yard-quoted\"); return !h || h.size === 2; })()","equals":true},{"fill":"label:Message","value":"TEST-and the rest"},{"shot":"two-comments"},{"click":{"role":"button","name":"Send","exact":true}},{"expect":{"css":".prose","hasText":"Comments on parts of your answers"}},{"expect":{"css":".bubble .bubble-comment"},"count":2},{"expect":{"css":".bubble .bubble-comment","hasText":"TEST-why echo?"}},{"expect":{"css":".bubble .bubble-comment","hasText":"TEST-say it twice"}},{"expect":{"css":".bubble","hasText":"TEST-and the rest"}},{"expect":{"css":".bubble","hasText":"Comments on parts"},"visible":false},{"expect":"css:.composer .quote-card","visible":false},{"eval":"(() => { const h = globalThis.CSS?.highlights?.get(\"yard-quoted\"); return !h || h.size === 0; })()","equals":true},{"wait":500},{"eval":"(() => { const el = document.querySelector(\".thread .scroll, .thread [class*=scroll]\") || document.querySelector(\".bubble\").closest(\"[class]\"); const b = [...document.querySelectorAll(\".bubble\")].pop().getBoundingClientRect(); return b.top < innerHeight && b.bottom > 0; })()","equals":true},{"shot":"sent"}]'
```

### no-reply-on-own-message
Viewports: desktop
Selecting text in your own prompt bubble offers no Comment button.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"eval":"(() => { const b = document.querySelector(\".thread .bubble\"); const r = document.createRange(); r.selectNodeContents(b); getSelection().removeAllRanges(); getSelection().addRange(r); return getSelection().toString(); })()","equals":"TEST-say hello"},{"wait":500},{"expect":{"role":"button","name":"Comment","exact":true},"visible":false}]'
```

### queue-a-message-while-working
Like Claude Code: while a turn runs the reply box stays open, "Add to queue"
puts the message under "Queued" above the box and empties the box, and the
agent gets it at its next step, in the same turn: here the approved command
is that step, and the fake's answer names what was steered in. Each viewport
gets its own busy session.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[cmd] TEST-busy turn mobile"}' --wait '{"op":"created"}' --save busy_mobile=sessionId
yv drive 'session:{{busy_mobile}}' --viewport mobile --steps '[{"expect":"text:Allow this command?"},{"fill":"label:Message","value":"TEST-queued one"},{"click":{"role":"button","name":"Add to queue","exact":true}},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"text":"TEST-queued one"},{"expect":"label:Message","value":""},{"shot":"queued"},{"click":"role:button:Allow once"},{"expect":"css:.thread","text":"Ran it. | steered: TEST-queued one"},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"visible":false},{"shot":"sent"}]'
yv state --expect '{"sessions":[{"id":"{{busy_mobile}}","status":"idle"}]}'
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[cmd] TEST-busy turn desktop"}' --wait '{"op":"created"}' --save busy_desktop=sessionId
yv drive 'session:{{busy_desktop}}' --viewport desktop --steps '[{"expect":"text:Allow this command?"},{"fill":"label:Message","value":"TEST-queued one"},{"click":{"role":"button","name":"Add to queue","exact":true}},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"text":"TEST-queued one"},{"expect":"label:Message","value":""},{"shot":"queued"},{"click":"role:button:Allow once"},{"expect":"css:.thread","text":"Ran it. | steered: TEST-queued one"},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"visible":false},{"shot":"sent"}]'
yv state --expect '{"sessions":[{"id":"{{busy_desktop}}","status":"idle"}]}'
```

### remove-a-queued-message
Viewports: desktop
× drops a queued message: it leaves the list, does not come back to the reply
box, and never reaches the agent.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"[slow] TEST-drop turn"}' --wait '{"op":"created"}' --save dropq_sid=sessionId
yv drive 'session:{{dropq_sid}}' --viewport desktop --steps '[{"fill":"label:Message","value":"TEST-dropped"},{"click":{"role":"button","name":"Add to queue","exact":true}},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"text":"TEST-dropped"},{"click":{"role":"button","name":"Remove from queue","exact":true}},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"visible":false},{"wait":500},{"expect":"label:Message","value":""},{"click":{"role":"button","name":"Stop","exact":true}},{"expect":"label:Message","value":""},{"expect":{"css":".thread","hasText":"TEST-dropped"},"visible":false}]'
```

### steer-a-running-turn
A message sent while the agent runs commands reaches it before its answer:
the answer accounts for it, in one turn, and the message shows where it went in.
The commands it interrupted still finish as succeeded, not failed.
(Sent over the API: a drive waits for the page to go quiet, which a running
turn never does.)

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"[steps] TEST-three steps"}' --wait '{"op":"event","event":{"type":"tool_started"}}' --save steps_sid=sessionId
yv api '{"op":"session.send","sessionId":"{{steps_sid}}","text":"TEST-steer"}'
yv drive 'session:{{steps_sid}}' --steps '[{"expect":"css:.thread","text":"steered: TEST-steer","timeout":15000},{"expect":"css:.thread","text":"TEST-steer"},{"expect":"css:.thread","text":"Worked for"},{"expect":{"css":".thread","hasText":"failed"},"visible":false},{"shot":"steered"}]'
yv state --expect '{"sessions":[{"id":"{{steps_sid}}","status":"idle"}]}'
```

### steer-a-selected-queued-message
Viewports: both
Steer sends one queued message into the active turn, ahead of the other queued message.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"[slow] TEST-steer desktop"}' --wait '{"op":"created"}' --save manual_steer_desktop=sessionId
yv drive 'session:{{manual_steer_desktop}}' --viewport desktop --steps '[{"fill":"label:Message","value":"TEST-keep queued"},{"click":"role:button:Add to queue"},{"expect":"css:.pending","text":"TEST-keep queued"},{"fill":"label:Message","value":"TEST-send now"},{"click":"role:button:Add to queue"},{"expect":"css:.pending","text":"TEST-send now"},{"shot":"steer-ready"},{"click":{"role":"button","name":"Steer","within":{"css":".pending-row","hasText":"TEST-send now"}}},{"expect":"css:.thread","text":"TEST-send now"},{"expect":"css:.pending","text":"TEST-keep queued"},{"expect":{"css":".pending-row","hasText":"TEST-send now"},"count":0},{"shot":"steered-one"}]'
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"[slow] TEST-steer mobile"}' --wait '{"op":"created"}' --save manual_steer_mobile=sessionId
yv drive 'session:{{manual_steer_mobile}}' --viewport mobile --steps '[{"fill":"label:Message","value":"TEST-keep queued"},{"click":"role:button:Add to queue"},{"expect":"css:.pending","text":"TEST-keep queued"},{"fill":"label:Message","value":"TEST-send now"},{"click":"role:button:Add to queue"},{"expect":"css:.pending","text":"TEST-send now"},{"shot":"steer-ready"},{"click":{"role":"button","name":"Steer","within":{"css":".pending-row","hasText":"TEST-send now"}}},{"expect":"css:.thread","text":"TEST-send now"},{"expect":"css:.pending","text":"TEST-keep queued"},{"expect":{"css":".pending-row","hasText":"TEST-send now"},"count":0},{"shot":"steered-one"}]'
```

### stop-returns-the-queue
Viewports: desktop
Stop ends the turn and puts what was queued back in the reply box, unsent.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"[slow] TEST-long turn"}' --wait '{"op":"created"}' --save slowq_sid=sessionId
yv drive 'session:{{slowq_sid}}' --viewport desktop --steps '[{"expect":"css:.thread","text":"Working"},{"fill":"label:Message","value":"TEST-held"},{"click":{"role":"button","name":"Add to queue","exact":true}},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"text":"TEST-held"},{"click":{"role":"button","name":"Stop","exact":true}},{"expect":"label:Message","value":"TEST-held"},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"visible":false},{"expect":{"css":".thread","hasText":"Echo: TEST-held"},"visible":false}]'
```

### edit-a-queued-message
Viewports: desktop
Edit takes a queued message back into the reply box; so does the up arrow in
an empty box.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"[slow] TEST-another long turn"}' --wait '{"op":"created"}' --save editq_sid=sessionId
yv drive 'session:{{editq_sid}}' --viewport desktop --steps '[{"fill":"label:Message","value":"TEST-first draft"},{"click":{"role":"button","name":"Add to queue","exact":true}},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"text":"TEST-first draft"},{"click":{"role":"button","name":"Edit","exact":true}},{"expect":"label:Message","value":"TEST-first draft"},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"visible":false},{"click":{"role":"button","name":"Add to queue","exact":true}},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"text":"TEST-first draft"},{"press":"ArrowUp","target":"label:Message"},{"expect":"label:Message","value":"TEST-first draft"},{"click":{"role":"button","name":"Stop","exact":true}}]'
```

### edit-a-queued-reply-keeps-its-quotes
Viewports: desktop
A queued message that starts with quotes (a Reply, or `> ` lines typed by
hand) shows its text in the queue with a quote count, and Edit brings the
quotes back as cards and the rest back as text, so nothing is lost.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"[slow] TEST-quote turn"}' --wait '{"op":"created"}' --save quoteq_sid=sessionId
yv drive 'session:{{quoteq_sid}}' --viewport desktop --steps '[{"fill":"label:Message","value":"> TEST-quoted line\n\nTEST-my reply"},{"click":{"role":"button","name":"Add to queue","exact":true}},{"expect":{"css":"[aria-label=\"Waiting to send\"] .pending-text"},"text":"TEST-my reply"},{"expect":{"css":"[aria-label=\"Waiting to send\"]"},"text":"1 quote"},{"click":{"role":"button","name":"Edit","exact":true}},{"expect":{"css":".composer .quote-card"},"text":"TEST-quoted line"},{"expect":"label:Message","value":"TEST-my reply"},{"shot":"edited"},{"click":{"role":"button","name":"Stop","exact":true}}]'
```

### draft-survives-switching-sessions
Text typed in a session's reply box and files attached to it stay there while
you look at another session and come back. Each session keeps its own draft,
and a sent draft does not come back.

```sh
printf 'TEST note\n' > "$(v work)/TEST-draft-note.txt"
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"fill":"label:Message","value":"TEST-half a reply"},{"upload":"input","files":["{{work}}/TEST-draft-note.txt"]},{"expect":"css:.composer .attachments","text":"TEST-draft-note.txt"},{"click":{"css":".session-row","hasText":"TEST-fix the greeting"}},{"expect":"label:Message","value":""},{"expect":{"css":".composer .attachments"},"visible":false},{"click":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":"label:Message","value":"TEST-half a reply"},{"expect":"css:.composer .attachments","text":"TEST-draft-note.txt"},{"shot":"draft-kept"},{"click":{"role":"button","name":"Send","exact":true}},{"expect":"css:.thread","text":"Echo: TEST-half a reply"},{"click":{"css":".session-row","hasText":"TEST-fix the greeting"}},{"click":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":"label:Message","value":""},{"expect":{"css":".composer .attachments"},"visible":false}]'
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"fill":"label:Message","value":"TEST-typed on the phone"},{"click":"role:button:Open sessions"},{"click":{"css":".session-row","hasText":"TEST-fix the greeting"}},{"expect":"label:Message","value":""},{"click":"role:button:Open sessions"},{"click":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":"label:Message","value":"TEST-typed on the phone"},{"shot":"draft-kept-mobile"}]'
```

### workspace-subagents
Viewports: both
The workspace opens on Terminal. Subagents shows the session's agents and their expandable steps and reports.

```sh
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"medium","cwd":"{{repo}}","worktree":true,"text":"[subagent] TEST-workspace desktop"}' --wait '{"op":"created"}' --save workspace_agents_desktop=sessionId
yv drive 'session:{{workspace_agents_desktop}}' --viewport desktop --steps '[{"expect":"css:.thread","text":"Echo: [subagent] TEST-workspace"},{"expect":{"css":".workspace-tabs [aria-pressed=true]","hasText":"Terminal"}},{"click":"role:button:Subagents"},{"expect":"role:region:Session subagents","text":"TEST-survey the repo"},{"click":{"role":"button","name":"TEST-survey the repo","within":"role:region:Session subagents"}},{"expect":"role:region:Session subagents","text":"TEST-subagent report"},{"expect":"role:region:Session subagents","text":"README.md"},{"shot":"subagents"}]'
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"medium","cwd":"{{repo}}","worktree":true,"text":"[subagent] TEST-workspace mobile"}' --wait '{"op":"created"}' --save workspace_agents_mobile=sessionId
yv drive 'session:{{workspace_agents_mobile}}' --viewport mobile --steps '[{"expect":"css:.thread","text":"Echo: [subagent] TEST-workspace"},{"click":"role:button:Workspace"},{"expect":{"css":".workspace-tabs [aria-pressed=true]","hasText":"Terminal"}},{"click":"role:button:Subagents"},{"expect":"role:region:Session subagents","text":"TEST-survey the repo"},{"click":{"role":"button","name":"TEST-survey the repo","within":"role:region:Session subagents"}},{"expect":"role:region:Session subagents","text":"TEST-subagent report"},{"expect":"role:region:Session subagents","text":"README.md"},{"shot":"subagents"}]'
```

### workspace-subagents-line-up
Viewports: both
Several subagents at once, one with a two-line task and two still running:
each is its own card with space between cards (none overlapping), and every
card's task, type and state and latest step start on the same left edge.

```sh
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"medium","cwd":"{{repo}}","worktree":true,"text":"[subagents] TEST-fan out desktop"}' --wait '{"op":"created"}' --save fan_desktop=sessionId
yv drive 'session:{{fan_desktop}}' --viewport desktop --steps '[{"click":{"css":".workspace-tabs .tab","hasText":"Subagents"}},{"expect":"css:.subagents-pane > .agent","count":3},{"expect":{"css":".subagents-pane > .agent","hasText":"running"}},{"eval":"(() => { const cards=[...document.querySelectorAll(\".subagents-pane > .agent\")].map(c=>c.getBoundingClientRect()); const lefts=[...document.querySelectorAll(\".subagents-pane .subagent-title, .subagents-pane .agent-what, .subagents-pane .subagent-meta, .subagents-pane .subagent-latest\")].map(e=>Math.round(e.getBoundingClientRect().left)); return cards.every((c,i)=>!i || c.top>=cards[i-1].bottom+4) && new Set(lefts).size===1; })()","equals":true},{"shot":"subagents-line-up"},{"click":{"role":"button","name":"TEST-Slack connector","within":"role:region:Session subagents"}},{"expect":"role:region:Session subagents","text":"TEST-subagents report: Slack done"},{"shot":"subagent-open"}]'
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"medium","cwd":"{{repo}}","worktree":true,"text":"[subagents] TEST-fan out mobile"}' --wait '{"op":"created"}' --save fan_mobile=sessionId
yv drive 'session:{{fan_mobile}}' --viewport mobile --steps '[{"click":"role:button:Workspace"},{"click":{"css":".workspace-tabs .tab","hasText":"Subagents"}},{"expect":"css:.subagents-pane > .agent","count":3},{"eval":"(() => { const cards=[...document.querySelectorAll(\".subagents-pane > .agent\")].map(c=>c.getBoundingClientRect()); const lefts=[...document.querySelectorAll(\".subagents-pane .subagent-title, .subagents-pane .agent-what, .subagents-pane .subagent-meta, .subagents-pane .subagent-latest\")].map(e=>Math.round(e.getBoundingClientRect().left)); return cards.every((c,i)=>!i || c.top>=cards[i-1].bottom+4) && new Set(lefts).size===1 && document.documentElement.scrollWidth<=innerWidth; })()","equals":true},{"shot":"subagents-line-up"}]'
```

### subagent-nests-and-expands
A Claude subagent (the Agent tool) shows as one entry in the work log: its
description, its type and how long it took; collapsed, its steps stay out of
the list. Opening it shows the steps it took, nested, and its report. The
scripted sidecar's `[subagent]` turn runs one subagent that reads a file and
lists a folder.

```sh
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"medium","cwd":"{{repo}}","worktree":true,"text":"[subagent] TEST-survey"}' --wait '{"op":"created"}' --save sub_sid=sessionId
yv drive 'session:{{sub_sid}}' --steps '[{"expect":"css:.thread","text":"Echo: [subagent] TEST-survey","timeout":20000},{"click":"role:button:Worked for"},{"expect":"css:.thread","text":"TEST-survey the repo"},{"expect":"css:.thread","text":"general-purpose"},{"expect":"css:.thread","text":"2 steps"},{"expect":{"css":".thread","hasText":"README.md"},"visible":false},{"shot":"subagent-collapsed"},{"click":"role:button:TEST-survey the repo"},{"expect":"css:.thread","text":"README.md"},{"expect":"css:.thread","text":"ls src"},{"expect":"css:.thread","text":"TEST-subagent report: two files read"},{"shot":"subagent-open"},{"click":"role:button:TEST-survey the repo"},{"expect":{"css":".thread","hasText":"README.md"},"visible":false}]'
```

### fold-names-the-subagent
A finished turn that ran a subagent says so on its folded line: "Ran 1
subagent", not the catch-all "1 other step" (#410). At both viewports.

```sh
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"medium","cwd":"{{repo}}","worktree":true,"text":"[subagent] TEST-fold names it"}' --wait '{"op":"created"}' --save fold_sub_sid=sessionId
yv drive 'session:{{fold_sub_sid}}' --steps '[{"expect":"css:.thread","text":"Echo: [subagent] TEST-fold names it","timeout":20000},{"expect":"role:button:Worked for","text":"Ran 1 subagent"},{"expect":{"css":".thread","hasText":"other step"},"visible":false},{"shot":"fold-subagent"}]'
```

### working-line-stays-in-view
A long thread scrolled to the bottom: after Send, the "Working" line is inside
the visible thread from its first frame. It appears with the session's status,
just after the event that scrolled, so it used to sit below the fold until the
next step arrived (seconds, while an engine thinks). Whether the two land in
one frame is timing, so the recipe first grows the thread by 240 px without a
new item and expects it to stay at the bottom (the same path as the Working
line, a loading thumbnail or a growing reply box), then probes every frame
for 4 s after a real Send.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"TEST-filler 0"}' --wait '{"op":"created"}' --save wl=sessionId
for i in 1 2 3 4 5 6 7 8 9 10; do yv api "{\"op\":\"session.send\",\"sessionId\":\"{{wl}}\",\"text\":\"TEST-filler $i: a line long enough to take some room in the thread so that it scrolls\"}" >/dev/null; sleep 0.6; done
yv drive 'session:{{wl}}' --viewport desktop --steps '[{"expect":"css:.thread","text":"TEST-filler 10"},{"wait":500},{"eval":"(()=>{const d=document.createElement('\''div'\'');d.id='\''grow'\'';d.style.height='\''240px'\'';document.querySelector('\''.column'\'').appendChild(d);return 1})()"},{"wait":300},{"eval":"(()=>{const s=document.querySelector('\''.scroll'\'');const gap=Math.round(s.scrollHeight-s.scrollTop-s.clientHeight);return gap<2||('\''the thread grew and was left '\''+gap+'\'' px above the bottom'\'')})()","equals":true},{"eval":"(()=>{document.getElementById('\''grow'\'').remove();return 1})()"},{"wait":300},{"eval":"(()=>{window.__cut=[];const t0=performance.now();const tick=()=>{const w=document.querySelector('\''.working'\'');const s=document.querySelector('\''.scroll'\'');if(w){const d=w.getBoundingClientRect().bottom-s.getBoundingClientRect().bottom;if(d>1)window.__cut.push(Math.round(d))};if(performance.now()-t0<4000)requestAnimationFrame(tick)};requestAnimationFrame(tick);return 1})()"},{"fill":"label:Message","value":"[steps] TEST-working in view"},{"click":"role:button:Send"},{"wait":3500},{"eval":"window.__cut.length===0||('\''Working was cut by '\''+window.__cut.slice(0,5).join('\'','\'')+'\'' px'\'')","equals":true},{"shot":"working-in-view"}]'
yv drive 'session:{{wl}}' --viewport mobile --steps '[{"expect":"css:.thread","text":"TEST-filler 10"},{"wait":500},{"eval":"(()=>{const d=document.createElement('\''div'\'');d.id='\''grow'\'';d.style.height='\''240px'\'';document.querySelector('\''.column'\'').appendChild(d);return 1})()"},{"wait":300},{"eval":"(()=>{const s=document.querySelector('\''.scroll'\'');const gap=Math.round(s.scrollHeight-s.scrollTop-s.clientHeight);return gap<2||('\''the thread grew and was left '\''+gap+'\'' px above the bottom'\'')})()","equals":true},{"eval":"(()=>{document.getElementById('\''grow'\'').remove();return 1})()"},{"wait":300},{"eval":"(()=>{window.__cut=[];const t0=performance.now();const tick=()=>{const w=document.querySelector('\''.working'\'');const s=document.querySelector('\''.scroll'\'');if(w){const d=w.getBoundingClientRect().bottom-s.getBoundingClientRect().bottom;if(d>1)window.__cut.push(Math.round(d))};if(performance.now()-t0<4000)requestAnimationFrame(tick)};requestAnimationFrame(tick);return 1})()"},{"fill":"label:Message","value":"[steps] TEST-working in view, mobile"},{"click":"role:button:Send"},{"wait":3500},{"eval":"window.__cut.length===0||('\''Working was cut by '\''+window.__cut.slice(0,5).join('\'','\'')+'\'' px'\'')","equals":true},{"shot":"working-in-view"}]'
```

### pr-link-hover-card
Viewports: both
A GitHub PR link in an answer is a real link (a bare URL shows as
`owner/repo#number`, as on GitHub; a local file link is a download button, not a PR link). A mouse
resting on it opens a card after a moment with the PR's title, an Open pill,
"1 of 3 checks failed: lint", the review decision, +12 −3, the author and
"updated 3 h ago", inside the window at 390 px too. Moving away closes it.
The PR is #4242 from the fake gh, asked through `pr.peek`.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"TEST-pr links\n\nOpened https://github.com/test-owner/test-demo-app/pull/4242. It builds on [the greeting module](https://github.com/test-owner/test-demo-app/pull/4100); see also [#4243](https://github.com/test-owner/test-demo-app/pull/4243/files) and https://github.com/test-owner/test-demo-app/pull/9999 (gone). Local [`src/greet.ts`](src/greet.ts) stays code."}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_echo}}' --steps '[{"expect":{"css":".prose a.pr-link","hasText":"test-owner/test-demo-app#4242"}},{"expect":{"css":".prose button.file-link","hasText":"src/greet.ts"}},{"eval":"[...document.querySelectorAll(\".prose a.pr-link\")].slice(-4).map((a) => a.getAttribute(\"href\") + \" \" + a.target).join(\"|\")","equals":"https://github.com/test-owner/test-demo-app/pull/4242 _blank|https://github.com/test-owner/test-demo-app/pull/4100 _blank|https://github.com/test-owner/test-demo-app/pull/4243/files _blank|https://github.com/test-owner/test-demo-app/pull/9999 _blank"},{"hover":{"css":".prose a.pr-link","hasText":"test-owner/test-demo-app#4242"}},{"expect":"role:tooltip","text":"TEST-fix the greeting"},{"expect":"role:tooltip","text":"1 of 3 checks failed: lint"},{"expect":"role:tooltip","text":"Review required"},{"expect":"role:tooltip","text":"+12"},{"expect":"role:tooltip","text":"@test-author · updated 3 h ago"},{"expect":{"css":".pr-pill","hasText":"Open"}},{"expect":"role:tooltip","inViewport":true},{"eval":"document.documentElement.scrollWidth <= window.innerWidth","equals":true},{"shot":"pr-card"},{"hover":"css:.thread .bar h1"},{"expect":"role:tooltip","visible":false}]'
```

### pr-link-card-states
Viewports: desktop
A merged PR gets a Merged pill, a draft a Draft pill and "No checks", and a
long title wraps inside the card. Hovering one link after another swaps the card.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"TEST-pr links\n\nOpened https://github.com/test-owner/test-demo-app/pull/4242. It builds on [the greeting module](https://github.com/test-owner/test-demo-app/pull/4100); see also [#4243](https://github.com/test-owner/test-demo-app/pull/4243/files) and https://github.com/test-owner/test-demo-app/pull/9999 (gone). Local [`src/greet.ts`](src/greet.ts) stays code."}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"hover":{"css":".prose a.pr-link","hasText":"the greeting module"}},{"expect":"role:tooltip","text":"TEST-add the greeting module"},{"expect":{"css":".pr-pill","hasText":"Merged"}},{"expect":"role:tooltip","text":"1 check passed"},{"shot":"pr-card-merged"},{"hover":{"css":".prose a.pr-link","hasText":"#4243"}},{"expect":"role:tooltip","text":"much longer pull request title"},{"expect":{"css":".pr-pill","hasText":"Draft"}},{"expect":"role:tooltip","text":"No checks"},{"expect":"role:tooltip","inViewport":true},{"expect":"role:tooltip","count":1},{"shot":"pr-card-draft"}]'
```

### pr-link-card-error
Viewports: both
A PR gh cannot find shows the card with why, not a page banner. The failed
`pr.peek` reply is the point here, so errors are allowed.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"TEST-pr links\n\nOpened https://github.com/test-owner/test-demo-app/pull/4242. It builds on [the greeting module](https://github.com/test-owner/test-demo-app/pull/4100); see also [#4243](https://github.com/test-owner/test-demo-app/pull/4243/files) and https://github.com/test-owner/test-demo-app/pull/9999 (gone). Local [`src/greet.ts`](src/greet.ts) stays code."}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_echo}}' --allow-errors --steps '[{"hover":{"css":".prose a.pr-link","hasText":"#9999"}},{"expect":"role:tooltip","text":"no such pull request, or no access to it"},{"expect":"role:tooltip","inViewport":true},{"expect":"css:.banner-error","visible":false},{"shot":"pr-card-error"}]'
```

### pr-link-keyboard-and-touch
Viewports: both
Keyboard focus on a PR link opens its card and Escape closes it. A tap is
not a hover: it follows the link (here caught before it leaves the page)
and opens no card.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"TEST-pr links\n\nOpened https://github.com/test-owner/test-demo-app/pull/4242. It builds on [the greeting module](https://github.com/test-owner/test-demo-app/pull/4100); see also [#4243](https://github.com/test-owner/test-demo-app/pull/4243/files) and https://github.com/test-owner/test-demo-app/pull/9999 (gone). Local [`src/greet.ts`](src/greet.ts) stays code."}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"press":"Tab","target":{"css":".prose a.pr-link","hasText":"the greeting module"}},{"expect":"role:tooltip","text":"much longer pull request title"},{"expect":"role:tooltip","count":1},{"press":"Escape"},{"expect":"role:tooltip","visible":false}]'
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"eval":"document.addEventListener(\"click\", (e) => { const a = e.target.closest(\"a\"); if (a) { e.preventDefault(); window.__followed = a.href; } }, true); true","equals":true},{"tap":{"css":".prose a.pr-link","hasText":"test-owner/test-demo-app#4242"}},{"wait":800},{"expect":"role:tooltip","visible":false},{"eval":"window.__followed","equals":"https://github.com/test-owner/test-demo-app/pull/4242"}]'
```

### links-inside-bold-text
Viewports: both
Bold recommendations keep their link labels, open the correct destinations, turn
a local file into a download button, and keep literal code and other schemes inert.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"TEST-bold links\n\n**Merge [TEST change #4242](https://github.com/test-owner/test-demo-app/pull/4242) now.**\n\n**Read [the review guide](https://example.com/guide), or run `check --links`.**\n\n**Keep [Local notes](/tmp/notes.txt), [Script](javascript:alert%281%29), and `[literal](https://example.com/literal)` inert.**"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_echo}}' --steps '[
  {"expect":{"css":".prose strong a.pr-link","hasText":"TEST change #4242"}},
  {"expect":{"css":".prose strong a","hasText":"the review guide"}},
  {"eval":"[...document.querySelectorAll(\".prose strong a\")].map(a => [a.textContent, a.getAttribute(\"href\"), a.target])","equals":[["TEST change #4242","https://github.com/test-owner/test-demo-app/pull/4242","_blank"],["the review guide","https://example.com/guide","_blank"]]},
  {"expect":{"css":".prose strong code","hasText":"check --links"}},
  {"expect":{"css":".prose strong button.file-link","hasText":"Local notes"}},
  {"expect":{"css":".prose strong code","hasText":"Script"}},
  {"expect":{"css":".prose strong code","hasText":"[literal](https://example.com/literal)"}},
  {"shot":"bold-links"},
  {"eval":"document.addEventListener(\"click\", (event) => { const link = event.target.closest(\"a\"); if (link) { event.preventDefault(); window.followedLink = link.href; } }, true)"},
  {"click":"role:link:the review guide"},
  {"eval":"window.followedLink","equals":"https://example.com/guide"}
]'
```

### file-links-download
Viewports: both
A file the agent links opens in the workspace from the session's host, whatever
dialect the link is in: a path relative to the session's folder with a line
number, ChatGPT's `sandbox:/mnt/data/…`, an absolute path. Its Download button
saves the bytes as a file, never as a page (an agent's HTML shows as source
and does not run). A missing file says so in the view, and the page stays put.

```sh
printf 'org,domain\n1,example.com\n' > "$(v plain)/TEST-report.csv"
printf '<script>window.top.__pwned=1</script>' > "$(v plain)/TEST-page.html"
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"TEST-files\n\n- [TEST report](TEST-report.csv:2)\n- [TEST sandbox copy](sandbox:/mnt/data/TEST-report.csv)\n- [TEST absolute]({{plain}}/TEST-report.csv)\n- [TEST page](TEST-page.html#L1)\n- [TEST missing](TEST-gone.csv)"}' --wait '{"op":"created"}' --save files_sid=sessionId
yv drive 'session:{{files_sid}}' --steps '[
  {"eval":"(() => { window.__dl = []; const click = HTMLAnchorElement.prototype.click; HTMLAnchorElement.prototype.click = function () { if (this.download) window.__dl.push(fetch(this.href).then((r) => r.blob()).then(async (b) => [this.download, b.type, await b.text()])); return click.call(this); }; return true; })()","equals":true},
  {"click":{"css":".prose button.file-link","hasText":"TEST report"}},
  {"expect":{"css":".file-view .file-name","hasText":"TEST-report.csv"}},
  {"expect":{"css":".file-view .file-meta","hasText":"CSV · 1 row · 2 columns"}},
  {"expect":{"css":".file-table th","hasText":"domain"}},
  {"click":{"css":".file-view button.file-download"}},
  {"wait":1000},
  {"eval":"Promise.all(window.__dl)","equals":[["TEST-report.csv","application/octet-stream","org,domain\n1,example.com\n"]]},
  {"shot":"file-links"}
]'
yv drive 'session:{{files_sid}}' --steps '[{"click":{"css":".prose button.file-link","hasText":"TEST sandbox copy"}},{"expect":{"css":".file-view .file-meta","hasText":"1 row"}},{"expect":{"css":".file-view .file-path","hasText":"TEST-plain-folder/TEST-report.csv"}}]'
yv drive 'session:{{files_sid}}' --steps '[{"click":{"css":".prose button.file-link","hasText":"TEST absolute"}},{"expect":{"css":".file-view .file-meta","hasText":"1 row"}}]'
yv drive 'session:{{files_sid}}' --steps '[
  {"eval":"(() => { window.__dl = []; const click = HTMLAnchorElement.prototype.click; HTMLAnchorElement.prototype.click = function () { if (this.download) window.__dl.push(fetch(this.href).then((r) => r.blob()).then(async (b) => [this.download, b.type, await b.text()])); return click.call(this); }; return true; })()","equals":true},
  {"click":{"css":".prose button.file-link","hasText":"TEST page"}},
  {"expect":{"css":".file-line-hit","hasText":"<script>window.top.__pwned=1</script>"}},
  {"click":{"css":".file-view button.file-download"}},
  {"wait":1000},
  {"eval":"Promise.all(window.__dl)","equals":[["TEST-page.html","application/octet-stream","<script>window.top.__pwned=1</script>"]]},
  {"eval":"window.__pwned === undefined","equals":true}
]'
# The refusal is the thing proven here, so its error reply is allowed.
yv drive 'session:{{files_sid}}' --allow-errors --steps '[
  {"click":{"css":".prose button.file-link","hasText":"TEST missing"}},
  {"expect":{"css":".file-view .file-empty[role=alert]","hasText":"TEST-gone.csv was not found"}},
  {"expect":{"role":"button","name":"Try again","exact":true}},
  {"eval":"location.hash.startsWith(\"#/s/\")","equals":true},
  {"shot":"file-link-missing"}
]'
```

### file-viewer-kinds
Viewports: both
The workspace's file view fits the file: Markdown as a document (heading sizes,
quotes, links that open files beside it), a CSV as a grid with its row and
column count and numbers aligned right, an image on a checkerboard, code with
line numbers and the linked line marked, and anything else as "No preview"
with Download. The default workspace keeps its three tabs; the file's tab
closes with its ×.

```sh
mkdir -p "$(v plain)/notes"
printf '# TEST doc\n\nIntro.\n\n## TEST part\n\n> TEST quoted note\n\nSee [TEST nested](notes/TEST-nested.md).\n' > "$(v plain)/TEST-doc.md"
printf '# TEST nested doc\n' > "$(v plain)/notes/TEST-nested.md"
printf 'name,"users, total",growth\nAtlas,"1,618",4.5%%\n"Ember, Co.",386,-2%%\n' > "$(v plain)/TEST-grid.csv"
printf 'one\ntwo\nthree\nfour\n' > "$(v plain)/TEST-code.ts"
printf 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==' | base64 -d > "$(v plain)/TEST-dot.png"
printf 'PK\003\004\000\001\002' > "$(v plain)/TEST-bundle.zip"
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"TEST-kinds\n\n- [TEST doc](TEST-doc.md)\n- [TEST grid](TEST-grid.csv)\n- [TEST code](TEST-code.ts:3)\n- [TEST dot](TEST-dot.png)\n- [TEST bundle](TEST-bundle.zip)"}' --wait '{"op":"created"}' --save kinds_sid=sessionId
yv drive 'session:{{kinds_sid}}' --steps '[
  {"expect":"css:.workspace-tabs .file-tab","visible":false},
  {"click":{"css":".prose button.file-link","hasText":"TEST doc"}},
  {"expect":{"css":".file-doc h3[data-level=\"1\"]","hasText":"TEST doc"}},
  {"expect":{"css":".file-doc h3[data-level=\"2\"]","hasText":"TEST part"}},
  {"expect":{"css":".file-doc blockquote","hasText":"TEST quoted note"}},
  {"expect":{"css":".file-view .file-meta","hasText":"Markdown"}},
  {"shot":"markdown"},
  {"click":{"css":".file-doc button.file-link","hasText":"TEST nested"}},
  {"expect":{"css":".file-doc h3","hasText":"TEST nested doc"}},
  {"expect":{"css":".file-view .file-path","hasText":"notes/TEST-nested.md"}},
  {"click":"role:button:Close TEST-nested.md"},
  {"expect":"css:.workspace-tabs .file-tab","visible":false}
]'
yv drive 'session:{{kinds_sid}}' --steps '[
  {"click":{"css":".prose button.file-link","hasText":"TEST grid"}},
  {"expect":{"css":".file-view .file-meta","hasText":"CSV · 2 rows · 3 columns"}},
  {"eval":"[...document.querySelectorAll(\".file-table thead th\")].map((th) => th.textContent)","equals":["","name","users, total","growth"]},
  {"eval":"[...document.querySelectorAll(\".file-table tbody tr:nth-child(2) td\")].map((td) => [td.textContent, td.className])","equals":[["2","row-num"],["Ember, Co.",""],["386","num"],["-2%","num"]]},
  {"shot":"csv"}
]'
yv drive 'session:{{kinds_sid}}' --steps '[{"click":{"css":".prose button.file-link","hasText":"TEST code"}},{"expect":{"css":".file-view .file-meta","hasText":"4 lines"}},{"expect":{"css":".file-line-hit","hasText":"three"}},{"expect":"css:.file-line","count":4},{"shot":"code"}]'
yv drive 'session:{{kinds_sid}}' --steps '[{"click":{"css":".prose button.file-link","hasText":"TEST dot"}},{"expect":"css:.file-image img"},{"eval":"new Promise((done) => { const img = document.querySelector(\".file-image img\"); const check = () => img.complete ? done([img.naturalWidth, img.src.startsWith(\"blob:\")]) : setTimeout(check, 50); check(); })","equals":[1,true]}]'
yv drive 'session:{{kinds_sid}}' --steps '[{"click":{"css":".prose button.file-link","hasText":"TEST bundle"}},{"expect":{"css":".file-view .file-empty","hasText":"No preview for this file"}},{"expect":{"css":".file-view button.file-download"},"enabled":true},{"shot":"no-preview"}]'
```

### numbered-steps-across-code-blocks
Viewports: both
Numbered steps with a code block under each keep their numbers in the answer:
the step after a code block shows 2, not 1 again.

```sh
yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"TEST-numbered steps\n\n1. Install the deps:\n   \u0060\u0060\u0060sh\n   bun install\n   \u0060\u0060\u0060\n2. Build the UI:\n   \u0060\u0060\u0060sh\n   bunx vite build\n   \u0060\u0060\u0060\n3. Start the daemon."}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive 'session:{{sid_echo}}' --steps '[{"expect":{"css":".prose li","hasText":"Start the daemon."}},{"expect":{"css":"pre.code","hasText":"bunx vite build"}},{"eval":"[...document.querySelectorAll(\".prose ol\")].slice(-3).map(o => [o.start, o.textContent])","equals":[[1,"Install the deps:"],[2,"Build the UI:"],[3,"Start the daemon."]]},{"shot":"numbered-steps"}]'
```

### history-reloads-after-resync
Viewports: both
When the socket falls behind, yardd resends a snapshot on the same connection, and a running session's next event can arrive before the page asks for history again. The conversation must reload instead of going blank with only "Working" left. The drive delivers that snapshot and an event straight to the page's socket handler.

```sh
yv drive 'session:{{sid_echo}}' --steps '[{"expect": "css:.thread", "text": "Echo:"}, {"eval": "(() => { const send = WebSocket.prototype.send; window.__sent = 0;\n  WebSocket.prototype.send = function (d) { if (this.readyState === 1) { window.__appWs = this; window.__sent++; } return send.call(this, d); };\n  return true; })()", "equals": true}, {"eval": "(() => { location.hash = '\''#/s/{{sid_think}}'\''; return true; })()", "equals": true}, {"wait": 800}, {"eval": "(() => { location.hash = '\''#/s/{{sid_echo}}'\''; return true; })()", "equals": true}, {"expect": "css:.thread", "text": "Echo:"}, {"eval": "window.__sent > 0", "equals": true}, {"eval": "new Promise((done) => {\n  const app = window.__appWs;\n  if (!app) return done('\''no app socket'\'');\n  const probe = new WebSocket(app.url);\n  probe.onmessage = (e) => {\n    const msg = JSON.parse(e.data);\n    if (msg.op !== '\''snapshot'\'') return;\n    probe.close();\n    app.onmessage({ data: e.data });\n    app.onmessage({ data: JSON.stringify({ op: '\''event'\'', sessionId: '\''{{sid_echo}}'\'', seq: 1000, event: { type: '\''turn_started'\'' } }) });\n    done('\''injected'\'');\n  };\n})", "equals": "injected"}, {"wait": 1500}, {"shot": "after-resync"}, {"expect": "css:.thread", "text": "Echo:"}]'
```

## Gotchas
- Recipes share one isolated yardd and run in order, and several send to `sid_echo`: by `reply-to-part-of-answer` its first answer is scrolled out of view at 390 px, so that recipe scrolls the paragraph into view before selecting it (a user only selects what they see).
- A failed turn leaves the session status `idle` in the sidebar, with the red error line in the thread.
- 2026-11-18: `pr-link-keyboard-and-touch` failed once in `map run all` and passed when run alone at the same build (`88cb5a8`). The tap itself behaved: no card, the link was followed. The mobile drive still failed on one failed API call, `pr.peek` → "no such pull request, or no access to it", so some PR link was peeked. In the full run, earlier recipes have already sent to `sid_echo`, and that includes the #9999 link, which is meant to be gone. Not yet settled: whether a tap on a phone starts a peek of some other link (a product gap) or this is ordering between recipes. Don't add `--allow-errors` to make it pass.
