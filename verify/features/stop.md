# Stop

Stop (`session.interrupt`) ends a running turn. If the engine ignores it, yardd
ends the turn itself after 5 s (`turn_completed` `interrupted`, with a note); a
stuck Claude sidecar is killed and replaced on the next message.

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

### stop-slow-turn
A turn that honours interrupts stops at once: the Stop button goes away and
the session is idle. The cut-off text is marked "Stopped after …" under it,
even though a turn with no tool calls has no work-log fold to carry the label.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"[slow] TEST-long job"}' --wait '{"op":"created"}' --save slow_sid=sessionId
yv drive 'session:{{slow_sid}}' --viewport desktop --steps '[{"expect":"css:.thread","text":"Working"},{"click":{"role":"button","name":"Stop","exact":true}},{"expect":{"role":"button","name":"Stop","exact":true},"visible":false},{"expect":"css:.thread","text":"Stopped after"},{"shot":"stopped-text-turn"}]'
yv state --expect '{"sessions":[{"id":"{{slow_sid}}","status":"idle"}]}'
```

### stop-stuck-codex-turn
A Codex turn that ignores Stop is closed by yardd within about 5 s, with a note.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"[stuck] TEST-ignores stop"}' --wait '{"op":"created"}' --save stuck_sid=sessionId
yv drive 'session:{{stuck_sid}}' --viewport desktop --steps '[{"click":{"role":"button","name":"Stop","exact":true}},{"expect":"css:.thread","text":"Codex did not stop the turn, so it was ended here.","timeout":15000}]'
yv state --expect '{"sessions":[{"id":"{{stuck_sid}}","status":"idle"}]}'
```

### stop-stuck-claude-turn
A Claude sidecar that ignores Stop is killed; the next message starts a new one.

```sh
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"medium","cwd":"{{repo}}","worktree":true,"text":"[stuck] TEST-claude ignores stop"}' --wait '{"op":"created"}' --save cstuck_sid=sessionId
yv drive 'session:{{cstuck_sid}}' --viewport desktop --steps '[{"click":{"role":"button","name":"Stop","exact":true}},{"expect":"css:.thread","text":"Claude Code did not stop, so its process was ended.","timeout":15000},{"fill":"label:Message","value":"TEST-after stop"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-after stop"}]'
```

### stop-stuck-opencode-turn
An OpenCode turn that ignores Stop is ended by yardd within about 5 s, with a
note. What it had written stays in that turn: the next answer starts clean
(it used to read "WorkingEcho: …").
Viewports: desktop

```sh
yv api '{"op":"session.create","engine":"opencode","model":"opencode/fake-free","effort":"","cwd":"{{repo}}","worktree":true,"text":"[stuck] TEST-opencode ignores stop"}' --wait '{"op":"created"}' --save ocstuck_sid=sessionId
yv drive 'session:{{ocstuck_sid}}' --viewport desktop --steps '[{"expect":"css:.thread","text":"Working","timeout":15000},{"click":{"role":"button","name":"Stop","exact":true}},{"expect":"css:.thread","text":"OpenCode did not stop the turn, so it was ended here.","timeout":15000},{"fill":"label:Message","value":"TEST-after the stuck turn"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-after the stuck turn","timeout":15000},{"expect":{"css":".thread","hasText":"WorkingEcho"},"count":0},{"shot":"after"}]'
```

### stuck-opencode-late-output-is-dropped
An OpenCode turn ended here after Stop can still stream later (its prompt is
still running on the agent's side). That late output belongs to no turn: it
is not shown, and it does not run into the next answer. Output that arrives
during the next turn cannot be told apart (ACP updates carry no prompt id).
Viewports: desktop

```sh
yv api '{"op":"session.create","engine":"opencode","model":"opencode/fake-free","effort":"","cwd":"{{repo}}","worktree":true,"text":"[late] TEST-opencode streams after stop"}' --wait '{"op":"created"}' --save oclate_sid=sessionId
yv drive 'session:{{oclate_sid}}' --viewport desktop --steps '[{"expect":"css:.thread","text":"Working","timeout":15000},{"click":{"role":"button","name":"Stop","exact":true}},{"expect":"css:.thread","text":"OpenCode did not stop the turn, so it was ended here.","timeout":15000},{"wait":3000},{"expect":{"css":".thread","hasText":"Late words"},"count":0},{"fill":"label:Message","value":"TEST-after the late output"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-after the late output","timeout":15000},{"expect":{"css":".thread","hasText":"Late words"},"count":0},{"expect":{"css":".thread","hasText":"Late thought"},"count":0},{"shot":"after"}]'
```

### stop-while-asking
Stop also ends a turn that is waiting for an approval: the card's buttons go
away and the session is idle. (The fake Codex does not answer the interrupt
while it waits, so yardd ends the turn after its grace period.)

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[cmd] TEST-stop while asking"}' --wait '{"op":"created"}' --save ask_stop_sid=sessionId
yv drive 'session:{{ask_stop_sid}}' --viewport desktop --steps '[{"expect":"role:button:Allow once"},{"click":{"role":"button","name":"Stop","exact":true}},{"expect":{"role":"button","name":"Stop","exact":true},"visible":false,"timeout":15000},{"expect":"role:button:Allow once","visible":false}]'
yv state --expect '{"sessions":[{"id":"{{ask_stop_sid}}","status":"idle"}]}'
```

### stop-while-asking-says-not-answered
Viewports: both
The card of an approval nobody answered says so once its turn ends; it does
not say "Denied", a choice the user never made. Its command row reads "Did
not run · not answered" and the fold "1 command not run". (A yardd restart
mid-question closes the turn the same way.)

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[cmd] TEST-stop unanswered"}' --wait '{"op":"created"}' --save unans_sid=sessionId
yv drive 'session:{{unans_sid}}' --viewport desktop --steps '[{"expect":"role:button:Allow once"},{"click":{"role":"button","name":"Stop","exact":true}},{"expect":{"role":"button","name":"Stop","exact":true},"visible":false,"timeout":15000}]'
yv drive 'session:{{unans_sid}}' --steps '[{"click":"role:button:1 command not run"},{"expect":"css:.approval","text":"Not answered before the turn ended"},{"expect":{"css":".tool-row","hasText":"echo approved"},"text":"Did not run"},{"expect":{"css":".tool-row","hasText":"echo approved"},"text":"not answered"},{"expect":"css:.thread .tool-row .bad","count":0},{"expect":{"css":".approval","hasText":"Denied"},"count":0},{"shot":"not-answered"}]'
```

### acp-stop-while-asking-says-not-run
Viewports: both
The same for an ACP engine (OpenCode), which fails the asked call itself on
Stop before the turn ends: a command reads "1 command not run" and "Did not
run", an edit "1 file not edited" and "Did not edit", and nothing in the fold
is marked failed. Until 2026-11-24 they read "Ran 1 command · 1 failed" and
"Waiting to edit notes.txt · failed".

```sh
for vp in desktop mobile; do
  yv api '{"op":"session.create","engine":"opencode","model":"opencode/fake-free","effort":"","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[cmd] TEST-acp halt cmd '$vp'"}' --wait '{"op":"created"}' --save acp_cmd_sid=sessionId
  yv drive 'session:{{acp_cmd_sid}}' --viewport $vp --steps '[{"expect":"role:button:Allow once","timeout":15000},{"click":{"role":"button","name":"Stop","exact":true}},{"expect":{"role":"button","name":"Stop","exact":true},"visible":false,"timeout":15000},{"click":"role:button:1 command not run"},{"expect":"css:.approval","text":"Not answered before the turn ended"},{"expect":{"css":".tool-row","hasText":"echo"},"text":"Did not run"},{"expect":"css:.thread .tool-row .bad","count":0},{"shot":"acp-cmd-not-run"}]'
  yv api '{"op":"session.create","engine":"opencode","model":"opencode/fake-free","effort":"","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[edit] TEST-acp halt edit '$vp'"}' --wait '{"op":"created"}' --save acp_edit_sid=sessionId
  yv drive 'session:{{acp_edit_sid}}' --viewport $vp --steps '[{"expect":"role:button:Allow once","timeout":15000},{"click":{"role":"button","name":"Stop","exact":true}},{"expect":{"role":"button","name":"Stop","exact":true},"visible":false,"timeout":15000},{"click":"role:button:1 file not edited"},{"expect":"css:.approval","text":"Not answered before the turn ended"},{"expect":{"css":".tool-row","hasText":"notes.txt"},"text":"Did not edit"},{"expect":{"css":".tool-row","hasText":"notes.txt"},"text":"not answered"},{"expect":"css:.thread .tool-row .bad","count":0},{"shot":"acp-edit-not-edited"}]'
done
```

## Gotchas
- The Stop button only shows while the session is running or waiting for approval.
- `role:button:Stop` matches by substring, so it also hits a sidebar row whose title contains "stop" (such as "TEST-stop while asking") and opens that session instead. Use `{"role":"button","name":"Stop","exact":true}`.
