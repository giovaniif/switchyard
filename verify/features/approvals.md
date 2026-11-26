# Approvals

In "Ask first" mode a command waits for the user: the thread shows "Allow this
command?" with Allow once, Always allow and Deny, and the sidebar row says
"Needs approval". The decision goes to the engine and the turn continues.

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

### pending-shows-card
Expect at both viewports: the card with its three buttons, and the row status
(in the sidebar drawer at 390 px).

```sh
yv drive 'session:{{sid_cmd}}' --viewport desktop --steps '[{"expect":"text:Allow this command?"},{"expect":"role:button:Allow once"},{"expect":"role:button:Always allow"},{"expect":{"role":"button","name":"Deny","exact":true}},{"expect":"css:.session-row","text":"Needs approval"},{"shot":"approval"}]'
yv drive 'session:{{sid_cmd}}' --viewport mobile --steps '[{"expect":"text:Allow this command?"},{"expect":"role:button:Allow once"},{"expect":"role:button:Always allow"},{"expect":{"role":"button","name":"Deny","exact":true}},{"shot":"approval"},{"click":"role:button:Open sessions"},{"expect":"css:.session-row","text":"Needs approval"}]'
yv state --expect '{"sessions":[{"id":"{{sid_cmd}}","status":"needs_approval"}]}'
```

### allow-once
Allowing runs the command; the fake answers "Ran it." and the session goes idle.

```sh
yv drive 'session:{{sid_cmd}}' --viewport desktop --steps '[{"click":"role:button:Allow once"},{"expect":"css:.thread","text":"Ran it."}]'
yv state --expect '{"sessions":[{"id":"{{sid_cmd}}","status":"idle"}]}'
```

### allow-then-follow-up
After an allowed command the turn ends and the session is idle, so the next
message starts a turn instead of waiting in the queue. (The answer used to be
logged after the engine heard it; a fast engine could end the turn first and
leave the session `running`, with the follow-up stuck in the queue.)

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[cmd] TEST-allow then follow up"}' --wait '{"op":"created"}' --save follow_sid=sessionId
yv drive 'session:{{follow_sid}}' --viewport desktop --steps '[{"click":"role:button:Allow once"},{"expect":"css:.thread","text":"Ran it."},{"fill":"label:Message","value":"TEST-next step"},{"press":"Enter"},{"expect":"css:.thread","text":"Echo: TEST-next step"}]'
yv state --expect '{"sessions":[{"id":"{{follow_sid}}","status":"idle"}]}'
```

### deny
Denying skips the command; the fake answers "Skipped it.".

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[cmd] TEST-deny me"}' --wait '{"op":"created"}' --save deny_sid=sessionId
yv drive 'session:{{deny_sid}}' --viewport desktop --steps '[{"click":{"role":"button","name":"Deny","exact":true}},{"expect":"css:.thread","text":"Skipped it."},{"click":"role:button:Worked for"},{"expect":"css:.thread","text":"Denied"}]'
```

### waiting-then-denied-command-did-not-run
Viewports: both
While the card asks, the command's row reads "Waiting to run" with no live
pulse: nothing runs before the answer. Denied, the turn's fold counts "1
command not run" and the row reads "Did not run · denied", not a red
"failed". The same for Codex and for OpenCode (ACP), whose approvals name
the call they ask about.

```sh
for vp in desktop mobile; do
  for eng in codex opencode; do
    if [ $eng = codex ]; then model=fake-fast; cmd="echo approved"; else model=opencode/fake-free; cmd="echo hi"; fi
    yv api '{"op":"session.create","engine":"'$eng'","model":"'$model'","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[cmd] TEST-deny '$eng' '$vp'"}' --wait '{"op":"created"}' --save deny2_sid=sessionId
    yv drive 'session:{{deny2_sid}}' --viewport $vp --steps '[{"expect":"text:Allow this command?","timeout":15000},{"expect":{"css":".tool-row","hasText":"'"$cmd"'"},"text":"Waiting to run"},{"expect":"css:.thread .tool-row .pulse","count":0},{"shot":"waiting-'$eng'"},{"click":{"role":"button","name":"Deny","exact":true}},{"expect":"css:.thread","text":"Skipped it."},{"expect":"role:button:Worked for","text":"1 command not run"},{"click":"role:button:Worked for"},{"expect":{"css":".tool-row","hasText":"'"$cmd"'"},"text":"Did not run"},{"expect":{"css":".tool-row","hasText":"'"$cmd"'"},"text":"denied"},{"expect":"css:.thread .tool-row .bad","count":0},{"shot":"denied-'$eng'"}]'
  done
done
```

### waiting-then-denied-edit-was-not-made
Viewports: both
The same for a file edit (Grok and OpenCode, whose edit rows the engine
titles "Edited"): while the card asks, the row reads "Waiting to edit";
denied, the fold counts "1 file not edited" and the row reads "Did not edit
· denied". Until 2026-11-24 it read "Edited notes.txt · denied" and "1 file
not run", for a file nobody edited.

```sh
for vp in desktop mobile; do
  for eng in grok opencode; do
    if [ $eng = grok ]; then model=fake-small; else model=opencode/fake-free; fi
    yv api '{"op":"session.create","engine":"'$eng'","model":"'$model'","effort":"","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[edit] TEST-deny edit '$eng' '$vp'"}' --wait '{"op":"created"}' --save deny_edit_sid=sessionId
    yv drive 'session:{{deny_edit_sid}}' --viewport $vp --steps '[{"expect":"text:Allow this?","timeout":15000},{"expect":{"css":".tool-row","hasText":"notes.txt"},"text":"Waiting to edit"},{"expect":{"css":".tool-row","hasText":"Edited"},"count":0},{"shot":"waiting-edit-'$eng'"},{"click":{"role":"button","name":"Deny","exact":true}},{"expect":"css:.thread","text":"Left it alone."},{"expect":"role:button:Worked for","text":"1 file not edited"},{"click":"role:button:Worked for"},{"expect":{"css":".tool-row","hasText":"notes.txt"},"text":"Did not edit"},{"expect":{"css":".tool-row","hasText":"notes.txt"},"text":"denied"},{"expect":{"css":".tool-row","hasText":"Edited"},"count":0},{"shot":"denied-edit-'$eng'"}]'
  done
done
```

### always-allow
"Always allow" runs it and records the choice.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[cmd] TEST-always"}' --wait '{"op":"created"}' --save always_sid=sessionId
yv drive 'session:{{always_sid}}' --viewport desktop --steps '[{"click":"role:button:Always allow"},{"expect":"css:.thread","text":"Ran it."},{"click":"role:button:Worked for"},{"expect":"css:.thread","text":"Always allowed"}]'
```

### double-click-allow-once
A double click on "Allow once" sends one decision: the buttons are disabled
once it is sent, so the second click does nothing. (They used to stay live
until yardd answered, and the second click got "approval is no longer
pending".) The two clicks are a task apart, so React renders in between, as
it does for a real double click.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[cmd] TEST-double click"}' --wait '{"op":"created"}' --save dbl_sid=sessionId
yv drive 'session:{{dbl_sid}}' --viewport desktop --steps '[{"expect":"role:button:Allow once"},{"eval":"(async () => { const b = [...document.querySelectorAll(\".approval button\")].find((x) => x.textContent.trim() === \"Allow once\"); b.click(); await new Promise((r) => setTimeout(r, 0)); b.click(); return \"clicked twice\"; })()","equals":"clicked twice"},{"expect":"css:.thread","text":"Ran it."},{"wait":500}]'
yv state --expect '{"sessions":[{"id":"{{dbl_sid}}","status":"idle"}]}'
```

### approval-wait-is-not-work
"Worked for" times the agent, not the user: a command that waited 5 s for
"Allow once" in a turn that otherwise takes well under a second reads
"Worked for 0s" or "1s", not "5s". (The wait used to be counted: an approval
answered the next morning read "Worked for 9h".)

```sh
for vp in desktop mobile; do
  yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"[cmd] TEST-wait for me '$vp'"}' --wait '{"op":"created"}' --save wait_sid=sessionId
  yv drive 'session:{{wait_sid}}' --viewport $vp --steps '[{"expect":"role:button:Allow once"},{"wait":5000},{"click":"role:button:Allow once"},{"expect":"css:.thread","text":"Ran it."},{"expect":"role:button:Worked for","text":"Ran 1 command"},{"shot":"worked-for-'$vp'"},{"eval":"/^Worked for [01]s/.test(document.querySelector(\".fold-row\").textContent)","equals":true}]'
done
```

### claude-question-answered
Claude's AskUserQuestion is a question card, not "Allow this?", even in Full
access: options to pick (one choice or several), Send stays off until every
question has an answer, and Claude hears the answers. (It used to show the
raw JSON with Allow once, and allowing it told Claude "The user did not
answer the questions.") The fake asks two: a database (one choice) and
checks (several).

```sh
for vp in desktop mobile; do
  yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"low","cwd":"{{repo}}","permission":"full","text":"[question] TEST-pick a stack '$vp'"}' --wait '{"op":"event","event":{"type":"approval_request"}}' --save q_sid=sessionId
  yv state --expect '{"sessions":[{"id":"{{q_sid}}","status":"needs_approval"}]}'
  yv drive 'session:{{q_sid}}' --viewport $vp --steps '[{"expect":"text:Claude has 2 questions"},{"expect":"css:.thread","text":"Asked"},{"expect":"role:button:Send answers","enabled":false},{"click":"role:radio:SQLite"},{"expect":"role:button:Send answers","enabled":false},{"click":"role:checkbox:Unit tests"},{"click":"role:checkbox:Lint"},{"shot":"question-'$vp'"},{"click":"role:button:Send answers"},{"expect":"css:.thread","text":"Answered: Which database should TEST-app use? → SQLite | Which checks should run before merging? → Unit tests, Lint"},{"click":"role:button:Worked for"},{"expect":"css:.question-answer","text":"SQLite"},{"shot":"answered-'$vp'"}]'
  yv state --expect '{"sessions":[{"id":"{{q_sid}}","status":"idle"}]}'
done
```

### claude-question-something-else-and-skip
"Something else" answers in the user's own words (it replaces a picked
option for a one-choice question); Skip tells Claude the user chose not to
answer and the card says "Skipped".

```sh
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"low","cwd":"{{repo}}","permission":"full","text":"[question] TEST-own words"}' --wait '{"op":"event","event":{"type":"approval_request"}}' --save own_sid=sessionId
yv drive 'session:{{own_sid}}' --viewport desktop --steps '[{"click":"role:radio:Postgres"},{"fill":"label:Something else: Which database should TEST-app use?","value":"DuckDB"},{"expect":"role:radio:Postgres","text":"Postgres"},{"click":"role:checkbox:End-to-end"},{"click":"role:button:Send answers"},{"expect":"css:.thread","text":"Which database should TEST-app use? → DuckDB | Which checks should run before merging? → End-to-end"}]'
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"low","cwd":"{{repo}}","permission":"full","text":"[question] TEST-skip it"}' --wait '{"op":"event","event":{"type":"approval_request"}}' --save skip_sid=sessionId
yv drive 'session:{{skip_sid}}' --viewport desktop --steps '[{"click":{"role":"button","name":"Skip","exact":true}},{"expect":"css:.thread","text":"Skipped the questions."},{"click":"role:button:Worked for"},{"expect":"css:.question-card","text":"Skipped"}]'
```

### codex-question-answered
Codex's request_user_input is the same card, titled "Codex has 2 questions",
in Full access too. (Codex used to get "not supported", and outside Plan mode
it never asked: yardd now starts it with
`features.default_mode_request_user_input`.) The fake asks a color (options,
plus its own answer) and a secret token (typed only, hidden); the token never
shows on the card or in the log (the fake's reply repeats it, to prove Codex
got it). Codex's question has no tool row, so a turn with no other steps has
no "Worked for" fold: the answered card stays in place.

```sh
for vp in desktop mobile; do
  yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","permission":"full","text":"[question] TEST-codex asks '$vp'"}' --wait '{"op":"event","event":{"type":"approval_request"}}' --save cq_sid=sessionId
  yv state --expect '{"sessions":[{"id":"{{cq_sid}}","status":"needs_approval"}]}'
  yv drive 'session:{{cq_sid}}' --viewport $vp --steps '[{"expect":"text:Codex has 2 questions"},{"expect":"role:button:Send answers","enabled":false},{"click":"role:radio:Blue"},{"expect":"role:button:Send answers","enabled":false},{"fill":"label:Answer: Paste the TEST-app deploy token.","value":"s3cret-TEST"},{"expect":"css:.question-card input[type=password]"},{"shot":"codex-question-'$vp'"},{"click":"role:button:Send answers"},{"expect":"css:.thread","text":"Answered: color=Blue | token=s3cret-TEST"},{"expect":"css:.question-answer","text":"Blue"},{"expect":"css:.question-card","text":"••••••"},{"shot":"codex-answered-'$vp'"}]'
  yv state --expect '{"sessions":[{"id":"{{cq_sid}}","status":"idle"}]}'
done
```

## Gotchas
- A decided approval folds into the turn's "Worked for … · Ran 1 command" row; open it to read "Allowed", "Always allowed" or "Denied". One the turn ended on (Stop, a failure, a yardd restart) reads "Not answered before the turn ended".
- `role:button:Allow` matches "Allow once" and "Always allow"; name buttons in full or use `exact`.
- A question card (Claude's AskUserQuestion, Codex's request_user_input) has no Allow buttons: its options are `role:radio` (one choice) or `role:checkbox` (several), the field is labelled "Something else: <question>", and Send reads "Send answer" or "Send answers". A Codex question with no options has only a field, labelled "Answer: <question>"; one whose options exclude other answers has no field.
