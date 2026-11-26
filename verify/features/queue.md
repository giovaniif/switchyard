# Queue

The resource guard queues a new session (status `queued`, with the reason)
while load is above the core count or free memory is under `YARD_MIN_FREE_GB`.
"Start anyway" starts it at once. This area runs with forced pressure
(`YARD_FORCE_PRESSURE`).

## Setup

```sh
yv iso up --pressure
yv iso seed --profile empty
```

## Cleanup

```sh
yv iso down
```

## Recipes

### new-session-queues
A new session waits with its reason shown, and the sidebar says Queued.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"TEST-queued job"}' --wait '{"op":"created"}' --save q_sid=sessionId
yv state --expect '{"sessions":[{"id":"{{q_sid}}","status":"queued"}]}'
yv drive 'session:{{q_sid}}' --steps '[{"expect":"css:.queued-line","text":"Queued:"},{"expect":"role:button:Start anyway"},{"shot":"queued"}]'
```

### start-anyway
"Start anyway" runs the queued prompt.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"TEST-start me anyway"}' --wait '{"op":"created"}' --save q2_sid=sessionId
yv drive 'session:{{q2_sid}}' --viewport desktop --steps '[{"click":"role:button:Start anyway"},{"expect":"css:.thread","text":"Echo: TEST-start me anyway"}]'
yv state --expect '{"sessions":[{"id":"{{q2_sid}}","status":"idle"}]}'
```

### double-click-start-anyway
A double click on "Start anyway" starts the session once and shows no error:
the button is disabled from the first click until the session leaves the
queue. (It used to stay live, and the second click got "this session is not
queued".) The two clicks are a task apart, so React renders in between, as it
does for a real double click.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"TEST-start me once"}' --wait '{"op":"created"}' --save q3_sid=sessionId
yv drive 'session:{{q3_sid}}' --viewport desktop --steps '[{"expect":"role:button:Start anyway"},{"eval":"(async () => { const b = [...document.querySelectorAll(\".queued-line button\")].find((x) => x.textContent.trim() === \"Start anyway\"); b.click(); await new Promise((r) => setTimeout(r, 0)); b.click(); return \"clicked twice\"; })()","equals":"clicked twice"},{"expect":"css:.thread","text":"Echo: TEST-start me once"},{"wait":500}]'
yv state --expect '{"sessions":[{"id":"{{q3_sid}}","status":"idle"}]}'
```

### agent-queues-and-starts-anyway
Viewports: desktop
An agent's first message goes through the same guard: under pressure it
waits with "Queued: …" and Start anyway, and Start anyway answers it.

```sh
yv api '{"op":"agent.save","agent":{"name":"TEST-Waiter","engine":"codex","model":"fake-fast","effort":"low"}}' --wait '{"op":"agent.saved"}'
yv drive new --viewport desktop --steps '[{"click":"role:button:Agents"},{"click":"css:.agent-card[aria-label=\"TEST-Waiter\"] .agent-message"},{"fill":"label:Message","value":"TEST-agent under pressure"},{"press":"Enter"},{"expect":"css:.queued-line","text":"Queued:"},{"shot":"agent-queued"},{"click":"role:button:Start anyway"},{"expect":"css:.thread","text":"Echo: TEST-agent under pressure"},{"expect":"role:button:Start anyway","count":0}]'
```

### queued-session-composer-says-start-first
A queued session takes no messages until it starts (yardd answers "this
session is queued; start it first"), so Send stays disabled. The message box
says so: it read "Reply", and typing plus Enter did nothing, with no reason
shown.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"TEST-queued reply"}' --wait '{"op":"created"}' --save q4_sid=sessionId
yv drive 'session:{{q4_sid}}' --steps '[{"expect":"css:.queued-line","text":"Queued:"},{"expect":"placeholder:Queued: Start anyway to reply"},{"fill":"label:Message","value":"TEST-one more thing"},{"expect":"role:button:Send","enabled":false},{"shot":"queued-composer"}]'
yv state --expect '{"sessions":[{"id":"{{q4_sid}}","status":"queued"}]}'
```

### hosts-page-shows-pressure
The Hosts page says the host is under pressure on its card, and its Details
say why new sessions wait.

```sh
yv drive hosts --steps '[{"expect":{"css":".host-summary .host-state-busy"},"text":"Under pressure"},{"click":"role:tab:System"},{"expect":"css:.host-inspector-panel","text":"New sessions are queued: pressure is forced on"}]'
```

## Gotchas
- Queued sessions survive a restart and keep their prompt.
