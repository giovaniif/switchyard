# OpenCode, Grok and Gemini

OpenCode (`opencode acp`), Grok (`grok agent stdio`) and Gemini (`gemini --acp`) run through yardd's
ACP driver. Where their CLI is installed, the account picker lists its identities and the separate model picker lists its models; a session on either streams its reply,
asks before commands and edits (Ask first), and takes the approval like the
other engines. Messages sent mid-turn wait for the next turn: neither engine
takes input inside a running one. The scripted `opencode` and `grok` in
`crates/yardd/tests/fake-codex` stand in for these engines; `[cmd]`, `[edit]`,
`[think]`, `[slow]`, `[stuck]`, `[fail]` work as for the fake Codex. Gemini exposes models and modes on session/new instead of configOptions.

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

### picker-lists-opencode-and-grok
Expect at both viewports: the account picker lists OpenCode, Grok and Gemini identities. Choosing Grok opens its own model list.

```sh
STEPS='[{"click":"css:.account-trigger"},{"expect":{"role":"menuitemradio","name":"OpenCode · Default"}},{"expect":{"role":"menuitemradio","name":"Gemini · Default"}},{"click":{"role":"menuitemradio","name":"Grok · Default"}},{"click":"css:button[aria-label^=\"Model \"]"},{"expect":{"css":".picker-model","hasText":"Fake Large"}},{"shot":"picker"}]'
yv drive new --viewport desktop --steps "$STEPS"
yv drive new --viewport mobile --steps "$(jq -nc --argjson steps "$STEPS" '[{"click":"role:button:Session setup"}] + $steps')"
```

### opencode-session-streams-a-reply
Expect at both viewports: a session started on OpenCode's free model answers
"Echo: <prompt>" and its row names the model.

```sh
PICK='[{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"OpenCode · Default"}},{"click":"css:button[aria-label^=\"Model \"]"},{"click":{"css":".picker-model","hasText":"Fake Free (opencode)"}}]'
REST='[{"fill":"label:Message","value":"TEST-hello from OpenCode"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-hello from OpenCode","timeout":15000},{"shot":"opencode-session"}]'
yv drive new --viewport desktop --steps "$(jq -nc --argjson a "$PICK" --argjson b "$REST" '$a + $b')"
yv drive new --viewport mobile --steps "$(jq -nc --argjson a "$PICK" --argjson b "$REST" '[{"click":"role:button:Session setup"}] + $a + [{"press":"Escape"}] + $b')"
```

### grok-command-asks-first
Expect at both viewports: a Grok session in Ask first shows the command's
approval card; Allow once runs it and the reply follows.

```sh
PICK='[{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Grok · Default"}},{"click":"css:button[aria-label^=\"Model \"]"},{"click":{"css":".picker-model","hasText":"Fake Small"}}]'
REST='[{"fill":"label:Message","value":"[cmd] TEST-check with Grok"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Allow this command?","timeout":15000},{"shot":"approval"},{"click":"role:button:Allow once"},{"expect":"css:.thread","text":"Ran it.","timeout":15000}]'
yv drive new --viewport desktop --steps "$(jq -nc --argjson a "$PICK" --argjson b "$REST" '$a + $b')"
yv drive new --viewport mobile --steps "$(jq -nc --argjson a "$PICK" --argjson b "$REST" '[{"click":"role:button:Session setup"}] + $a + [{"press":"Escape"}] + $b')"
```

### opencode-hands-off-from-its-picker
Expect on desktop: inside an OpenCode session the picker folds Claude, Codex
and Grok into rows that say switching hands off.

Viewports: desktop

```sh
yv drive new --viewport desktop --steps '[{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"OpenCode · Default"}},{"click":"css:button[aria-label^=\"Model \"]"},{"click":{"css":".picker-model","hasText":"Fake Free (opencode)"}},{"fill":"label:Message","value":"TEST-hand-off source"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-hand-off source","timeout":15000},{"click":"css:button[aria-label^=\"Model \"]"},{"expect":"role:button:Grok · 2 models · switching hands off"},{"expect":"role:button:Codex · 2 models · switching hands off"}]'
```

### queued-message-waits-for-next-turn
Expect at both viewports: a message sent while an OpenCode turn runs is
queued with "Queued: sent when the current turn ends", not the Claude/Codex
"reads it after its current step" (neither engine takes input mid-turn, so
yardd sends it as the next turn). Stop gives it back to the reply box.
```sh
for vp in mobile desktop; do
  yv api '{"op":"session.create","engine":"opencode","model":"opencode/fake-free","effort":"","cwd":"{{repo}}","worktree":true,"text":"[slow] TEST-opencode long job '$vp'"}' --wait '{"op":"created"}' --save oc_queue_sid=sessionId
  yv drive 'session:{{oc_queue_sid}}' --viewport $vp --steps '[{"expect":"css:.thread","text":"Working","timeout":15000},{"fill":"label:Message","value":"TEST-after the opencode job"},{"press":"Enter"},{"expect":{"css":".pending","hasText":"TEST-after the opencode job"}},{"expect":"css:.pending","text":"Queued: sent when the current turn ends"},{"shot":"queued"},{"click":{"role":"button","name":"Stop","exact":true}},{"expect":{"role":"button","name":"Stop","exact":true},"visible":false,"timeout":15000},{"expect":"label:Message","value":"TEST-after the opencode job"},{"expect":"css:.pending","count":0}]'
done
```

### gemini-picker-session-and-approval
Expect at both viewports: Gemini lists its session-discovered models; its
session streams a reply, asks before a command and takes Allow once.

```sh
PICK='[{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Gemini · Default"}},{"click":"css:button[aria-label^=\"Model \"]"},{"click":{"css":".picker-model","hasText":"Fake Small"}}]'
REST='[{"fill":"label:Message","value":"[cmd] TEST-check with Gemini"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Allow this command?","timeout":15000},{"shot":"gemini-approval"},{"click":"role:button:Allow once"},{"expect":"css:.thread","text":"Ran it.","timeout":15000}]'
yv drive new --viewport desktop --steps "$(jq -nc --argjson a "$PICK" --argjson b "$REST" '$a + $b + [{"expect":"role:button:Model Fake Small"},{"shot":"gemini-reply"}]')"
yv drive new --viewport mobile --steps "$(jq -nc --argjson a "$PICK" --argjson b "$REST" '[{"click":"role:button:Session setup"}] + $a + [{"press":"Escape"}] + $b + [{"click":"role:button:Session setup"},{"expect":"role:button:Model Fake Small"},{"shot":"gemini-reply"}]')"
```

### opencode-setup-chip-has-no-effort
Expect on phones: an OpenCode model without efforts shows no effort in the
Session setup chip, before and after the session starts. The chip used to
keep the last model's "· High" (and its label said High), squeezed onto two
lines, while the setup sheet itself had no Effort row.

Viewports: mobile

```sh
yv drive new --viewport mobile --steps '[{"click":"role:button:Session setup"},{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"OpenCode · Default"}},{"click":"css:button[aria-label^=\"Model \"]"},{"click":{"css":".picker-model","hasText":"Fake Free (opencode)"}},{"press":"Escape"},{"expect":"role:button:Session setup: Fake Free (opencode), Default, Ask first"},{"expect":"css:.composer-setup .setup-effort","count":0},{"shot":"new-chip"},{"fill":"label:Message","value":"TEST-opencode chip"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-opencode chip","timeout":15000},{"expect":"role:button:Session setup: Fake Free (opencode), Default, Ask first"},{"expect":"css:.composer-setup .setup-effort","count":0},{"shot":"session-chip"}]'
```

### gemini-separate-account-terminal-login
Expect at both viewports: an added Gemini account is logged out and offers
its own terminal command and Check login. The long command wraps inside the
dialog. Closing it returns to Hosts without starting a model turn.

```sh
yv api '{"op":"account.add","engine":"gemini","name":"TEST-Gemini Work"}' --wait '{"op":"accounts"}'
yv drive hosts --steps '[{"click":"role:tab:Engine logins"},{"expect":{"css":".account-row","hasText":"TEST-Gemini Work"}},{"click":"role:button:Log in TEST-Gemini Work"},{"expect":"css:.login-command","text":"GEMINI_CLI_HOME="},{"expect":"role:button:Check login"},{"shot":"gemini-login"},{"click":"role:button:Close"}]'
```

## Gotchas
- None of these ACP engines reports plan usage, so the usage meters and the automations' usage limits ignore them.
- The fake's OpenCode lists one model until a provider (`fakeprov`) is logged in; the logins area adds one.

- Gemini uses `gemini --acp`, its models come from session/new, and its scripted login is pre-seeded by iso up. Fresh Google login runs in a terminal on the selected host; the app checks it without a model turn.
- Below 700 px the account, model, effort and permission pickers fold into one `Session setup` chip (#290): a recipe that picks a model at mobile opens it first (`role:button:Session setup`), and closes it (`Escape`) before Send, which the open sheet covers. The four picker recipes ran one step list at both viewports and failed at mobile from 2026-11-19 until this was split per viewport.
