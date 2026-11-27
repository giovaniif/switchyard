# Models, efforts and permission modes

The composer's model button lists the current engine's models (each other
engine folded into a row of its own) and the current model's efforts.
Same-engine switches apply from the next turn, and so does a Codex speed. Picking another
engine's model asks to hand off, then starts a new session in the same folder
with a brief of the work so far. The permission picker (Ask first, Accept
edits, Plan only, Full access) changes the session at once.

## Setup

```sh
yv iso up --env YARD_TEST_CODEX_LONG_CATALOG=1
yv iso seed
```

## Cleanup

```sh
yv iso down
```

## Recipes

### model-list-scrolls-under-the-header
Viewports: both
A ten-model Codex catalog scrolls inside the picker. The speed control stays in
place while scrolling, and reopening the picker shows the selected model.

```sh
STEPS='[
  {"click":"role:button:Model Fake Fast"},
  {"expect":"role:button:Speed Standard","inViewport":true},
  {"eval":"(() => { const list = document.querySelector(\".model-popover\"); return list.scrollHeight - list.clientHeight >= 36; })()","equals":true},
  {"eval":"(() => { const list = document.querySelector(\".model-popover\"); return list.offsetWidth - list.clientWidth >= 6; })()","equals":true},
  {"eval":"window.modelHeadingTop = document.querySelector(\".model-panel .popover-heading\").getBoundingClientRect().top"},
  {"shot":"model-list-top"},
  {"press":"End"},
  {"eval":"(() => { const list = document.querySelector(\".model-popover\"); const panel = list.closest(\".model-panel\"); return list.scrollTop > 0 && panel.scrollTop === 0 && panel.querySelector(\".popover-heading\").getBoundingClientRect().top === window.modelHeadingTop; })()","equals":true},
  {"expect":"role:button:Speed Standard","inViewport":true},
  {"shot":"model-list-bottom"},
  {"click":{"role":"button","name":"Fake Option 8","exact":true}},
  {"click":"role:button:Model Fake Option 8"},
  {"eval":"(() => { const list = document.querySelector(\".model-popover\"); const row = list.querySelector(\"[aria-current=true]\").getBoundingClientRect(); const clip = list.getBoundingClientRect(); return list.scrollTop > 0 && row.top >= clip.top - 1 && row.bottom <= clip.bottom + 1; })()","equals":true},
  {"press":"Home"},
  {"click":{"role":"button","name":"Fake Fast","exact":true}}
]'
yv drive 'session:{{sid_echo}}' --viewport desktop --show-scrollbars --steps "$STEPS"
yv drive 'session:{{sid_echo}}' --viewport mobile --show-scrollbars --steps "$(jq -nc --argjson steps "$STEPS" '[{"click":"role:button:Session setup"}] + $steps')"
```

### switch-model-same-engine
Choosing Fake Deep in a Codex session saves the new model.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Model Fake Fast"},{"click":{"role":"button","name":"Fake Deep","exact":true}},{"press":"Escape"},{"expect":"role:button:Fake Deep"}]'
yv state --expect '{"sessions":[{"id":"{{sid_echo}}","model":"fake-deep"}]}'
```

### effort-is-its-own-chip
Viewports: both
Effort is a chip of its own next to the model's: the model chip and its menu
show models only, the effort chip opens a menu of the model's efforts, and
picking one changes the session's effort, kept by yardd across a reload (the
session keeps its model). It uses a session of its own on Fake Fast (Low or
High).

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","text":"TEST-effort chip"}' --wait '{"op":"created"}' --save effort_sid=sessionId
yv drive 'session:TEST-effort chip' --viewport desktop --steps '[{"expect":"role:button:Model Fake Fast"},{"expect":"role:button:Effort Low"},{"click":"role:button:Model Fake Fast"},{"expect":{"css":".model-popover .picker-item","hasText":"Fake Fast"}},{"expect":{"css":".model-popover .picker-head","hasText":"Effort"},"count":0},{"press":"Escape"},{"click":"role:button:Effort Low"},{"expect":"role:menuitemradio:High"},{"shot":"effort-menu"},{"click":"role:menuitemradio:High"},{"expect":"role:button:Effort High"},{"expect":"role:button:Model Fake Fast"},{"reload":true},{"expect":"role:button:Effort High"},{"click":"role:button:Effort High"},{"click":"role:menuitemradio:Low"},{"expect":"role:button:Effort Low"}]'
yv drive 'session:TEST-effort chip' --viewport mobile --steps '[{"click":"role:button:Session setup"},{"expect":"role:button:Model Fake Fast"},{"expect":"role:button:Effort Low"},{"click":"role:button:Model Fake Fast"},{"expect":{"css":".model-popover .picker-item","hasText":"Fake Fast"}},{"expect":{"css":".model-popover .picker-head","hasText":"Effort"},"count":0},{"press":"Escape"},{"click":"role:button:Effort Low"},{"expect":"role:menuitemradio:High"},{"shot":"effort-menu"},{"click":"role:menuitemradio:High"},{"expect":"role:button:Effort High"},{"expect":"role:button:Model Fake Fast"},{"reload":true},{"click":"role:button:Session setup"},{"expect":"role:button:Effort High"},{"click":"role:button:Effort High"},{"click":"role:menuitemradio:Low"},{"expect":"role:button:Effort Low"}]'
```

### permission-mode
Full access is saved on the session.

```sh
yv drive 'session:{{sid_think}}' --viewport desktop --steps '[{"click":"role:button:Ask first"},{"click":"role:menuitemradio:Full access"},{"expect":"role:button:Full access"}]'
yv state --expect '{"sessions":[{"id":"{{sid_think}}","permission":"full"}]}'
```

### hand-off-to-claude
Picking a Claude model asks first; "Hand off" starts a Claude session with the
same title in the same folder.

```sh
yv drive 'session:{{sid_edit}}' --viewport desktop --steps '[{"click":"role:button:Model Fake Fast"},{"click":{"css":".picker-fold"}},{"click":{"role":"button","name":"Haiku 5.5","exact":true}},{"expect":"role:group:Confirm hand-off"},{"click":{"role":"button","name":"Hand off","exact":true}},{"expect":"role:button:Haiku 5.5","timeout":15000}]'
yv state --expect '{"sessions":[{"engine":"claude","title":"[edit] TEST-fix the greeting"}]}'
```

### hand-off-notes-link-both-sessions
Both sessions keep the same title, so each hand-off note opens the other
session: "Handed off from Codex." goes back to the source, "Handed off to
Claude." goes to the new one.

```sh
yv drive 'session:{{sid_fail}}' --viewport desktop --steps '[{"click":"role:button:Model Fake Fast"},{"click":{"css":".picker-fold"}},{"click":{"role":"button","name":"Sonnet 5.5","exact":true}},{"click":{"role":"button","name":"Hand off","exact":true}},{"expect":"role:button:Sonnet 5.5","timeout":15000},{"expect":{"css":".note","hasText":"Handed off from Codex."}},{"click":{"role":"button","name":"Open session","exact":true}},{"expect":"role:button:Model Fake Fast"},{"expect":"css:.thread","text":"Handed off to Claude."},{"click":{"role":"button","name":"Open session","exact":true}},{"expect":"role:button:Sonnet 5.5"}]'
```

### hand-off-note-links-survive-a-deleted-sibling
A source handed off twice keeps each "Handed off to" note on the session that
hand-off made: after the first new session is deleted, the first note has no
link and the second still opens the second session. The source is the only
"TEST-plan the refactor" row on Fake Fast (the new sessions share its title).

```sh
yv api '{"op":"session.handoff","sessionId":"{{sid_think}}","engine":"claude","model":"claude-haiku-5-5","effort":"low"}' --wait '{"op":"created"}' --save handoff_first=sessionId
yv state --expect '{"sessions":[{"id":"{{handoff_first}}","status":"idle"}]}'
yv api '{"op":"session.handoff","sessionId":"{{sid_think}}","engine":"claude","model":"claude-sonnet-5-5","effort":"low"}' --wait '{"op":"created"}' --save handoff_second=sessionId
yv api '{"op":"session.delete","sessionId":"{{handoff_first}}"}' --wait '{"op":"session.deleted"}'
yv drive new --viewport desktop --steps '[{"click":{"css":".session-row:has-text(\"Fake Fast\")","hasText":"TEST-plan the refactor"}},{"expect":"css:.thread","text":"Handed off to Claude."},{"eval":"[...document.querySelectorAll(\".note\")].filter(n=>n.textContent.startsWith(\"Handed off to\")).map(n=>!!n.querySelector(\".note-link\")).join()","equals":"false,true"},{"click":{"role":"button","name":"Open session","exact":true}},{"expect":"role:button:Sonnet 5.5"}]'
```

### codex-speed-reaches-the-turn
Viewports: both
A Codex model with service tiers (Fake Fast: Fast, Ultrafast) shows a Speed
button in the model picker's header; Fake Deep has none. A speed picked in a
session survives a reload and goes to the engine with the next turn
(`serviceTierForTurn`, read back from the fake's turn log); switching to a
model without tiers drops it, so the next turn goes at Standard. On the New
session page the speed picked before Send reaches the first turn. Each
viewport uses a session of its own.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"TEST-speed desktop"}' --wait '{"op":"created"}' --save speed_desktop=sessionId
yv drive 'session:{{speed_desktop}}' --viewport desktop --steps '[{"expect":"css:.thread","text":"Echo: TEST-speed desktop"},{"click":"role:button:Model Fake Fast"},{"click":"role:button:Speed Standard"},{"expect":{"role":"menuitemradio","name":"Standard Default speed"},"text":"Standard"},{"shot":"speed-menu"},{"click":"role:menuitemradio:Fast Higher speed"},{"expect":"role:button:Speed Fast"},{"press":"Escape"},{"reload":true},{"click":"role:button:Model Fake Fast"},{"expect":"role:button:Speed Fast"},{"press":"Escape"},{"fill":"label:Message","value":"TEST-fast turn desktop"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-fast turn desktop"},{"click":"role:button:Model Fake Fast"},{"click":{"role":"button","name":"Fake Deep","exact":true}},{"press":"Escape"},{"click":"role:button:Model Fake Deep"},{"expect":"css:.model-panel","text":"Fake Deep"},{"expect":"css:.speed-trigger","count":0},{"press":"Escape"},{"fill":"label:Message","value":"TEST-deep turn desktop"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-deep turn desktop"}]'
test "$(grep '"TEST-fast turn desktop"' "$(v fake)/turn-inputs.jsonl" | jq -r .serviceTierForTurn)" = priority
test "$(grep '"TEST-deep turn desktop"' "$(v fake)/turn-inputs.jsonl" | jq -r .serviceTierForTurn)" = null
```

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"TEST-speed mobile"}' --wait '{"op":"created"}' --save speed_mobile=sessionId
yv drive 'session:{{speed_mobile}}' --viewport mobile --steps '[{"expect":"css:.thread","text":"Echo: TEST-speed mobile"},{"click":"role:button:Session setup"},{"click":"role:button:Model Fake Fast"},{"click":"role:button:Speed Standard"},{"click":"role:menuitemradio:Ultrafast"},{"expect":"role:button:Speed Ultrafast"},{"shot":"speed-picked"},{"press":"Escape"},{"press":"Escape"},{"fill":"label:Message","value":"TEST-ultrafast turn mobile"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-ultrafast turn mobile"}]'
test "$(grep '"TEST-ultrafast turn mobile"' "$(v fake)/turn-inputs.jsonl" | jq -r .serviceTierForTurn)" = ultrafast
```

```sh
yv drive new --viewport desktop --steps '[{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Codex · Default"}},{"press":"Escape"},{"click":"role:button:Model Fake Fast"},{"click":"role:button:Speed Standard"},{"click":"role:menuitemradio:Ultrafast"},{"expect":"role:button:Speed Ultrafast"},{"press":"Escape"},{"click":"role:button:work"},{"fill":"role:textbox:Folder","value":"{{repo}}"},{"press":"Enter"},{"fill":"label:Message","value":"TEST-ultrafast start"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-ultrafast start"},{"click":"role:button:Model Fake Fast"},{"expect":"role:button:Speed Ultrafast"}]'
test "$(grep '"TEST-ultrafast start"' "$(v fake)/turn-inputs.jsonl" | jq -r .serviceTierForTurn)" = ultrafast
```

### claude-lists-what-claude-reports
Viewports: both
Claude's models are what Claude Code reports for the account (the fake
sidecar's `models`: Opus 5.5, Fable 5.2, Sonnet 5.5, Haiku 5.5), not a list
built into yardd, which had no Fable. Fable 5.2 is offered and picking it saves
the session on `claude-fable-5-2`. Codex, in a Claude session, is one folded
row that says how many models it holds (its models are not missing).

```sh
yv drive 'session:{{sid_claude}}' --viewport desktop --steps '[{"click":"css:.model-trigger"},{"expect":{"css":".picker-model","hasText":"Fable 5.2"}},{"expect":{"css":".picker-fold","hasText":"Codex · 10 models"}},{"shot":"claude-models"},{"click":{"role":"button","name":"Fable 5.2","exact":true}},{"expect":"css:.model-trigger","text":"Fable 5.2"}]'
yv state --expect '{"sessions":[{"id":"{{sid_claude}}","model":"claude-fable-5-2"}]}'
yv drive 'session:{{sid_claude}}' --viewport mobile --steps '[{"click":"role:button:Session setup"},{"click":"css:.model-trigger"},{"expect":{"css":".picker-model[aria-current=true]","hasText":"Fable 5.2"}},{"expect":{"css":".picker-fold","hasText":"Codex · 10 models"}},{"shot":"claude-models"},{"click":{"role":"button","name":"Haiku 5.5","exact":true}},{"expect":"css:.model-trigger","text":"Haiku 5.5"}]'
yv state --expect '{"sessions":[{"id":"{{sid_claude}}","model":"claude-haiku-5-5"}]}'
```

### effort-change-claude-mid-session (manual: not built, see ARCHITECTURE "Engines")
Changing a Claude session's effort mid-session goes through `applyFlagSettings`, which is not wired yet.

## Gotchas
- The model menu opens on the current engine; the other engine is one row ("Claude · N models · switching hands off") that expands, and its models are labelled "Picking one hands off to a new session." Plan usage in the picker is covered in [usage](usage.md).
