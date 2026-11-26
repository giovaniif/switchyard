# Slack

The Slack connector on the Connectors page, under Messaging, and agents'
own Slack apps. The card connects the owner's own Slack: an app of his with
user scopes (his direct messages and the channels he is in, as him), made from
a configuration token (one click for a workspace an agent's app already has),
approved in Slack (which comes back to `https://yard.test/slack/oauth`, the
https address yardd serves its page on, though the page is http), then one
app-level token. Its steps show as Token, Approve, Connect; connected, the
header reads "As @test-owner in TEST-workspace" and the footer shows only each
token's kind and last four characters, with Disconnect. Below it, people who
wrote to an agent's app and are not allowed (Allow, Dismiss) and people who
may talk to agents (Remove). (The shared Switchyard bot, its default agent and
its channel rows were retired.)

An agent has a Slack app of its own, so people @mention it by its name: Add
to Slack in its header walks through a configuration refresh token (`xoxe-…`,
once per workspace), the app made from a manifest, its install (Approve in
Slack, no bot token pasted) and its app-level token (one screen, Open Basic
Information), then says "<Name> is in Slack" with its connection. A message
from an allowed person reaches it; it shows in the agent's conversation with a
Slack badge (an aubergine dot and "From TEST-Owner on Slack · #general") above
the bubble, the shared origin badge of agents.md, and the answer is posted
back in its Slack thread with :eyes: swapped for :white_check_mark:.
With a public address for Slack (opt-in, `YARD_SLACK_PUBLIC_URL` or the
`slack.publicUrl` op; the dialog does not offer it) the configuration token
is the only one: the dialog numbers two steps, the install comes back to
Switchyard on its own, and Slack's events come over HTTP to `/slack/events`,
signed. Its app wears the agent's face as its icon (a 1024-pixel PNG drawn
from the page's avatar), and a new name or purpose reaches Slack
(`apps.manifest.update`), its handle made from the name (`test-archivist`).

The fake: `verify/fakes/fake-slack` (Web API and Socket Mode on loopback),
started by Setup; `curl "$(v slack)/_fake/message"` makes Slack send a
message (`app`: `agent` for an agent's app, `you` for the owner's), `curl
"$(v slack)/_fake/state"` reads what was posted and the reactions. Users
UOWNER (TEST-Owner) and USTRANGER; channel C0GENERAL (#general); a token with
"bad" in it is refused.

## Setup

```sh
D=$(mktemp -d /tmp/yv-fake-slack-XXXXXX)
bun verify/fakes/fake-slack --port-file "$D/port" > "$D/log" 2>&1 < /dev/null &
echo $! > "$D/pid"
for i in $(seq 100); do [ -s "$D/port" ] && break; sleep 0.1; done
yv iso up --env "YARD_SLACK_API=http://127.0.0.1:$(cat "$D/port")/api/,YARD_ALLOW_ORIGIN=https://yard.test,YARD_SLACK_WATCH_MS=1000"
yv var set slack_dir "$D"
yv var set slack "http://127.0.0.1:$(cat "$D/port")"
yv api '{"op":"agent.save","agent":{"name":"TEST-Reviewer","engine":"codex","model":"fake-fast","effort":"low","color":"rose","about":"TEST-reviews pull requests"},"ref":"slack-setup"}' --wait '{"op":"agent.saved"}' --save reviewer=agent.id
```

## Cleanup

```sh
D=$(v slack_dir)
yv iso down
kill "$(cat "$D/pid")" 2>/dev/null || true
rm -rf "$D"
```

## Recipes

### not-connected
Viewports: both
Not connected, the card says what it is for, numbers three steps (Token
first) and asks for the configuration refresh token, with Get config token
opening Slack's app settings. Nobody may talk to agents yet.

```sh
yv drive connectors --viewport desktop --steps '[{"expect":"role:heading:Messaging"},{"expect":{"css":".slack-card .connector-account-state"},"text":"Not connected"},{"expect":"css:.slack-card .slack-progress li","count":3},{"expect":{"css":".slack-card .slack-progress li[aria-current=\"step\"]","hasText":"Token"}},{"expect":{"css":".slack-card"},"text":"Your direct messages and the channels you choose"},{"expect":"label:Configuration refresh token"},{"eval":"document.querySelector(\".slack-card a.login-open\").href","equals":"https://api.slack.com/apps"},{"expect":{"css":".slack-card"},"text":"Who can talk to agents"},{"eval":"!document.querySelector(\".slack-card\").innerText.includes(\"Default agent\")","equals":true},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), true)","equals":true},{"shot":"not-connected"}]'
yv drive connectors --viewport mobile --steps '[{"expect":"label:Configuration refresh token"},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), document.documentElement.scrollWidth <= document.documentElement.clientWidth)","equals":true},{"shot":"not-connected"}]'
```

### an-agents-own-slack-app
Viewports: both
Add to Slack, in TEST-Reviewer's header, takes the configuration token and
makes its own app (@agent in the fake). The page is http, yet Approve in Slack
comes back to the https address yardd serves its page on
(`https://yard.test/slack/oauth`), so no bot token is pasted; the last step is
one screen with Open Basic Information (the app's own page) and the app-level
token. Then it is in Slack and connected. A mention of its bot in #general
reaches it without its name and is answered in the thread through its app.

```sh
yv api '{"op":"slack.allow","user":"UOWNER"}' --wait '{"op":"slack"}'
yv drive agents --viewport desktop --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"click":"role:button:Add to Slack"},{"expect":"role:dialog:Add TEST-Reviewer to Slack"},{"expect":"css:.slack-progress li","count":3},{"fill":"label:Configuration refresh token","value":"xoxe-1-TEST-refresh"},{"click":"role:button:Create Slack app"},{"expect":{"css":".slack-progress li[aria-current=\"step\"]","hasText":"Approve"}},{"expect":"role:link:Approve in Slack"},{"expect":"label:Bot token","count":0},{"eval":"new URL(document.querySelector(\".slack-modal a.btn-primary\").href).searchParams.get(\"redirect_uri\")","equals":"https://yard.test/slack/oauth"},{"shot":"approve"},{"eval":"fetch(\"/slack/oauth?code=TEST-code&state=\" + new URL(document.querySelector(\".slack-modal a.btn-primary\").href).searchParams.get(\"state\")).then((r) => r.text()).then((t) => t.includes(\"is installed in Slack\"))","equals":true},{"expect":{"css":".slack-progress li[aria-current=\"step\"]","hasText":"Connect"}},{"expect":"role:link:Open Basic Information"},{"eval":"document.querySelector(\".slack-modal a.login-open\").href","equals":"https://api.slack.com/apps/A0AGENT/general"},{"shot":"app-token"},{"fill":"label:App-level token","value":"xapp-1-agent-TEST"},{"click":{"css":".slack-token button[type=submit]","hasText":"Connect"}},{"expect":"role:dialog:TEST-Reviewer is in Slack"},{"expect":"css:.slack-connection.is-live","text":"Connected"},{"expect":"css:.slack-modal","text":"Mention @test-reviewer"},{"shot":"in-slack"},{"click":"role:button:Done"},{"expect":"css:.agent-slack-button.is-live","text":"In Slack"}]'
yv drive agents --viewport mobile --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"click":"role:button:In Slack"},{"expect":"role:dialog:TEST-Reviewer is in Slack"},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"in-slack"}]'
TS=$(curl -sf -XPOST "$(v slack)/_fake/message" -d '{"app":"agent","user":"UOWNER","channel":"C0GENERAL","text":"<@UAGENT> TEST-from its own app"}' | jq -r .ts)
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '.reactions[$ts] == ["white_check_mark"]' > /dev/null && break; sleep 0.2; done
curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '(.posts | map(select(.thread_ts == $ts)) | length == 1 and (.[0].text | contains("TEST-from its own app"))) and (.manifests[0].display_information.name == "TEST-Reviewer") and (.manifests[0].features.bot_user.display_name == "test-reviewer") and (.calls | any(.method == "oauth.v2.access" and .params.redirect_uri == "https://yard.test/slack/oauth"))'
yv drive agents --viewport mobile --steps '[{"expect":{"css":".agent-card[aria-label=\"TEST-Reviewer\"]","hasText":"In Slack"}},{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"expect":{"css":".origin-badge.origin-slack","hasText":"From TEST-Owner on Slack · #general"}}]'
```

### allow-from-an-agents-app
Viewports: desktop
Someone not on the list who writes to an agent's app is ignored, told once
(ephemeral), and listed on the card with Allow; Allow moves them to "Can
talk to agents", and Remove takes them off again.

```sh
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","agentId":"{{reviewer}}","origin":"http://127.0.0.1","ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.botToken","agentId":"{{reviewer}}","token":"xoxb-agent-TEST","ref":"bot"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.appToken","agentId":"{{reviewer}}","token":"xapp-1-agent-TEST","ref":"app"}' --wait '{"op":"slack"}'
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e '.connections >= 1' > /dev/null && break; sleep 0.1; done
curl -sf -XPOST "$(v slack)/_fake/message" -d '{"app":"agent","user":"USTRANGER","text":"TEST-hello"}' > /dev/null
yv drive connectors --viewport desktop --steps '[{"expect":{"css":".slack-card .connector","hasText":"TEST-Stranger"},"text":"wrote to an agent directly · USTRANGER"},{"expect":{"css":".slack-card .connector","hasText":"TEST-Stranger"},"text":"Not allowed"},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), true)","equals":true},{"shot":"request"},{"click":"role:button:Allow TEST-Stranger"},{"expect":{"css":".slack-card .connector","hasText":"TEST-Stranger"},"text":"Can talk to agents"},{"shot":"allowed"},{"click":"role:button:Remove TEST-Stranger"},{"click":"role:button:Confirm removing TEST-Stranger"},{"expect":{"css":".slack-card .connector","hasText":"TEST-Stranger"},"count":0}]'
curl -sf "$(v slack)/_fake/state" | jq -e '(.ephemerals | map(select(.user == "USTRANGER")) | length) == 1'
```

### answer-in-the-slack-thread
Viewports: both
A mention of TEST-Reviewer's app in #general goes to its conversation; the
answer is posted in that thread through its app and the message ends with
:white_check_mark:. The conversation shows the message with its Slack badge
(aubergine dot) right-aligned over the bubble, then the answer.

```sh
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","agentId":"{{reviewer}}","origin":"http://127.0.0.1","ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.botToken","agentId":"{{reviewer}}","token":"xoxb-agent-TEST","ref":"bot"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.appToken","agentId":"{{reviewer}}","token":"xapp-1-agent-TEST","ref":"app"}' --wait '{"op":"slack"}'
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e '.connections >= 1' > /dev/null && break; sleep 0.1; done
yv api '{"op":"slack.allow","user":"UOWNER","name":"TEST-Owner"}' --wait '{"op":"slack"}'
TS=$(curl -sf -XPOST "$(v slack)/_fake/message" -d '{"app":"agent","user":"UOWNER","channel":"C0GENERAL","text":"<@UAGENT> TEST-check the release notes"}' | jq -r .ts)
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '.reactions[$ts] == ["white_check_mark"]' > /dev/null && break; sleep 0.2; done
curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '(.posts | map(select(.thread_ts == $ts and .channel == "C0GENERAL" and .app == "agent")) | length == 1 and (.[0].text | contains("TEST-check the release notes"))) and .reactions[$ts] == ["white_check_mark"]'
for vp in desktop mobile; do
yv drive agents --viewport $vp --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"expect":{"css":".origin-badge.origin-slack","hasText":"From TEST-Owner on Slack · #general"}},{"eval":"getComputedStyle(document.querySelector(\".origin-slack .origin-dot\")).backgroundColor","equals":"rgb(74, 21, 75)"},{"expect":{"css":".user-message .bubble","hasText":"TEST-check the release notes"}},{"expect":{"css":".thread"},"text":"Echo:"},{"eval":"(() => { const all = document.querySelectorAll(\".origin-badge\"); const b = all[all.length - 1].getBoundingClientRect(); const m = all[all.length - 1].nextElementSibling.getBoundingClientRect(); return b.bottom <= m.top && Math.abs(b.right - m.right) <= 6 && document.documentElement.scrollWidth <= document.documentElement.clientWidth; })()","equals":true},{"shot":"thread"}]'
done
```

### connect-your-slack
Viewports: both
With a configuration token already given for an agent's app, connecting the
owner's own Slack starts with one click on the workspace (Another workspace
shows the token field). Approve in Slack comes back to
`https://yard.test/slack/oauth` though the page is http; then one screen asks
for the app-level token (Open Basic Information is his app's own page, A0YOU
in the fake), and a refused one is said plainly (that refusal is the
drive's one expected API error, hence `--allow-errors`). Connected, the card reads
"As @test-owner in TEST-workspace". His app has user scopes only and no bot.

```sh
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","ref":"grant"}' --wait '{"op":"slack"}'
yv drive connectors --viewport desktop --steps '[{"click":"role:button:Another workspace"},{"expect":"label:Configuration refresh token"}]'
yv drive connectors --viewport desktop --steps '[{"expect":"css:.slack-card .slack-workspaces button","count":1},{"shot":"pick-workspace"},{"click":"css:.slack-card .slack-workspaces button"},{"expect":{"css":".slack-card .slack-progress li[aria-current=\"step\"]","hasText":"Approve"}},{"expect":{"css":".slack-card .connector-account-state"},"text":"Approve in Slack"},{"eval":"new URL(document.querySelector(\".slack-card a.btn-primary\").href).searchParams.get(\"redirect_uri\")","equals":"https://yard.test/slack/oauth"},{"expect":{"css":".slack-card"},"text":"needs an admin to approve new apps"},{"shot":"approve"},{"eval":"fetch(\"/slack/oauth?code=TEST-code&state=\" + new URL(document.querySelector(\".slack-card a.btn-primary\").href).searchParams.get(\"state\")).then((r) => r.text()).then((t) => t.includes(\"Your Slack is approved\"))","equals":true},{"expect":{"css":".slack-card .slack-progress li[aria-current=\"step\"]","hasText":"Connect"}},{"eval":"document.querySelector(\".slack-card a.login-open\").href","equals":"https://api.slack.com/apps/A0YOU/general"},{"fill":"label:App-level token","value":"xapp-1-you-bad"},{"click":{"css":".slack-card .slack-token button[type=submit]","hasText":"Connect"}},{"expect":{"css":".slack-card [role=alert]"},"text":"Slack refused that token"},{"shot":"app-token"},{"fill":"label:App-level token","value":"xapp-1-you-TEST"},{"click":{"css":".slack-card .slack-token button[type=submit]","hasText":"Connect"}},{"expect":{"css":".slack-card"},"text":"As @test-owner in TEST-workspace"},{"expect":{"css":".slack-card .connector-account-state"},"text":"Connected"},{"expect":"css:.slack-card .slack-progress","count":0},{"shot":"connected"}]' --allow-errors
yv drive connectors --viewport mobile --steps '[{"expect":{"css":".slack-card"},"text":"As @test-owner in TEST-workspace"},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), document.documentElement.scrollWidth <= document.documentElement.clientWidth)","equals":true},{"shot":"connected"}]'
curl -sf "$(v slack)/_fake/state" | jq -e '(.manifests | map(select(.oauth_config.scopes.user)) | length == 1 and (.[0].features.bot_user == null) and (.[0].settings.socket_mode_enabled == true)) and (.calls | any(.method == "oauth.v2.access" and .params.client_id == "C0YOU"))'
```

### tokens-stay-in-yardd
Viewports: desktop
The page shows only each of the owner's tokens' kind and last four
characters; the tokens are in no log line, and their file is readable by its
owner alone. (Run alone, it connects his Slack by pasting his user token.)

```sh
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","me":true,"ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.userToken","token":"xoxp-you-TEST","ref":"user"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.appToken","token":"xapp-1-you-TEST","ref":"app"}' --wait '{"op":"slack"}'
yv drive connectors --viewport desktop --steps '[{"expect":"css:.slack-card .slack-tokens","text":"xoxp-…TEST · xapp-…TEST"},{"eval":"!document.body.innerText.includes(\"you-TEST\")","equals":true}]'
! grep -q "you-TEST" "$(v data)/yardd.log"
ls -l "$(v data)/slack.json" | cut -c1-10 | grep -qx -- "-rw-------"
```

### one-token-with-a-public-address
Viewports: both
With a public address for Slack (opt-in, set here by the `slack.publicUrl`
op; the dialog does not offer it), a new agent's app takes the configuration
token only: two steps (Token, Approve), the Approve link
comes back to `https://yard.test/slack/oauth` though the page is http,
and once approved the dialog says it is in Slack with Slack sending to
`/slack/events`, no connection token asked. A signed event over HTTP reaches
it and is answered in the thread; an unsigned one is refused. Its app gets the
agent's face as a 1024-pixel PNG icon, and renaming the agent renames its app
and handle in Slack.

```sh
yv api '{"op":"slack.allow","user":"UOWNER"}' --wait '{"op":"slack"}'
yv api '{"op":"agent.save","agent":{"name":"TEST-Scribe","engine":"codex","model":"fake-fast","effort":"low","color":"teal","face":"glasses"},"ref":"scribe"}' --wait '{"op":"agent.saved"}' --save scribe=agent.id
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.publicUrl","url":"https://slack.yard.test","ref":"pub"}' --wait '{"op":"slack","slack":{"publicUrl":"https://slack.yard.test"}}'
yv drive agents --viewport desktop --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Scribe\"] .agent-message"},{"click":"role:button:Add to Slack"},{"expect":"role:dialog:Add TEST-Scribe to Slack"},{"expect":"css:.slack-progress li","count":2},{"shot":"public-address"},{"click":{"css":".slack-workspaces button","nth":0}},{"expect":{"css":".slack-progress li[aria-current=\"step\"]","hasText":"Approve"}},{"expect":"css:.slack-modal","text":"then TEST-Scribe is in Slack"},{"eval":"new URL(document.querySelector(\".slack-modal a.btn-primary\").href).searchParams.get(\"redirect_uri\")","equals":"https://yard.test/slack/oauth"},{"shot":"approve"},{"eval":"fetch(\"/slack/oauth?code=TEST-code&state=\" + new URL(document.querySelector(\".slack-modal a.btn-primary\").href).searchParams.get(\"state\")).then((r) => r.text()).then((t) => t.includes(\"It is ready\"))","equals":true},{"expect":"role:dialog:TEST-Scribe is in Slack"},{"expect":"css:.slack-connection","text":"Slack sends its messages to https://slack.yard.test/slack/events"},{"expect":"label:App-level token","count":0},{"shot":"in-slack"}]'
P=$(v port)
R=$(curl -sf -XPOST "$(v slack)/_fake/event" -d "{\"url\":\"http://127.0.0.1:$P/slack/events\",\"user\":\"UOWNER\",\"channel\":\"C0GENERAL\",\"text\":\"<@UAGENT> TEST-over http\"}")
echo "$R" | jq -e '.results | length == 2 and all(.status == 200)'
TS=$(echo "$R" | jq -r .ts)
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '.reactions[$ts] == ["white_check_mark"]' > /dev/null && break; sleep 0.2; done
curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '.posts | map(select(.thread_ts == $ts)) | length == 1 and (.[0].text | contains("TEST-over http"))'
curl -sf -XPOST "$(v slack)/_fake/event" -d "{\"url\":\"http://127.0.0.1:$P/slack/events\",\"user\":\"UOWNER\",\"text\":\"TEST-forged\",\"sign\":\"bad\"}" | jq -e '.results | all(.status == 401)'
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e '.icons | any(.width == 1024 and .height == 1024 and .png)' > /dev/null && break; sleep 0.2; done
curl -sf "$(v slack)/_fake/state" | jq -e '.icons | any(.width == 1024 and .height == 1024 and .png and .type == "image/png")'
yv api '{"op":"agent.save","agent":{"id":"{{scribe}}","name":"TEST-Archivist"},"ref":"rename"}' --wait '{"op":"agent.saved"}'
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e '.updates | any(.manifest.display_information.name == "TEST-Archivist")' > /dev/null && break; sleep 0.2; done
curl -sf "$(v slack)/_fake/state" | jq -e '.updates | map(select(.manifest.display_information.name == "TEST-Archivist")) | length >= 1 and (.[0].manifest.features.bot_user.display_name == "test-archivist") and (.[0].manifest.settings.socket_mode_enabled == false)'
yv drive agents --viewport mobile --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Archivist\"] .agent-message"},{"click":"role:button:In Slack"},{"expect":"role:dialog:TEST-Archivist is in Slack"},{"expect":"css:.slack-modal","text":"Mention @test-archivist"},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"renamed"}]'
yv api '{"op":"slack.app.remove","agentId":"{{scribe}}","ref":"rm"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.publicUrl","url":"","ref":"pub"}' --wait '{"op":"slack","slack":{"publicUrl":""}}'
```

### a-watched-channel
Viewports: both
In an agent's Slack dialog the owner watches a channel by its ID (#incidents,
C0INCIDENT in the fake; the name comes from Slack) and can stop watching it.
An integration's alert there (a `bot_message` with an attachment, no mention)
reaches the agent as one read-only notice after the batching window (1 s in
this area's Setup): the conversation shows it with the badge "Watched on
Slack · #incidents · from TEST-Datadog" and the note that the turn is
read-only (Plan), and the answer is posted in the alert's thread.

```sh
yv api '{"op":"agent.save","agent":{"name":"TEST-Oncall","engine":"codex","model":"fake-fast","effort":"low","color":"amber","about":"TEST-triages alerts"},"ref":"oncall"}' --wait '{"op":"agent.saved"}' --save oncall=agent.id
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","agentId":"{{oncall}}","origin":"http://127.0.0.1","ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.botToken","agentId":"{{oncall}}","token":"xoxb-agent-TEST","ref":"bot"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.appToken","agentId":"{{oncall}}","token":"xapp-1-agent-TEST","ref":"app"}' --wait '{"op":"slack"}'
yv drive agents --viewport desktop --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Oncall\"] .agent-message"},{"click":"role:button:In Slack"},{"expect":"role:dialog:TEST-Oncall is in Slack"},{"expect":"css:.slack-modal","text":"answers only when someone mentions it"},{"fill":"label:Slack channel ID","value":"incidents"},{"click":{"role":"button","name":"Watch","exact":true}},{"expect":"css:.slack-modal","text":"C0123ABCD"},{"fill":"label:Slack channel ID","value":"C0INCIDENT"},{"click":{"role":"button","name":"Watch","exact":true}},{"expect":{"css":".slack-mention-list li","hasText":"#incidents"}},{"expect":"css:.slack-modal","text":"TEST-Oncall watches these channels"},{"expect":"role:button:Stop watching"},{"shot":"watching"}]' --allow-errors
TS=$(curl -sf -XPOST "$(v slack)/_fake/message" -d '{"app":"agent","channel":"C0INCIDENT","user":null,"bot_id":"BTESTDD","subtype":"bot_message","username":"TEST-Datadog","text":"","attachments":[{"title":"[P1] TEST-High error rate on api","text":"Error rate 12%"}]}' | jq -r .ts)
for i in $(seq 150); do curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '.reactions[$ts] == ["white_check_mark"]' > /dev/null && break; sleep 0.2; done
curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '(.posts | map(select(.thread_ts == $ts and .app == "agent")) | length == 1) and .reactions[$ts] == ["white_check_mark"]'
yv drive agents --viewport desktop --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Oncall\"] .agent-message"},{"expect":{"css":".origin-badge.origin-slack","hasText":"Watched on Slack · #incidents · from TEST-Datadog"}},{"expect":{"css":".user-message .bubble","hasText":"TEST-High error rate on api"}},{"expect":{"css":".thread"},"text":"This turn is read-only (Plan)"},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"notice"}]'
yv drive agents --viewport mobile --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Oncall\"] .agent-message"},{"expect":{"css":".origin-badge.origin-slack","hasText":"Watched on Slack · #incidents"}},{"expect":{"css":".thread"},"text":"This turn is read-only (Plan)"},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"notice"},{"click":"role:button:In Slack"},{"expect":{"css":".slack-mention-list li","hasText":"#incidents"}},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"watching"},{"click":"role:button:Stop watching"},{"expect":"css:.slack-modal","text":"answers only when someone mentions it"}]'
yv api '{"op":"slack.app.remove","agentId":"{{oncall}}","ref":"rm"}' --wait '{"op":"slack"}'
```

## Gotchas

- The land gate's secret check (`verify/src/land.ts`, `SECRET_TEXT`) blocks
  a PR whose added lines hold `xoxb-`/`xoxp-` plus 20 or more characters,
  fake or not. Recipes use the fake's short tokens (`xoxb-agent-TEST`,
  `xapp-1-you-TEST`).
- Recipes share one daemon and one fake, in order: `an-agents-own-slack-app`
  needs a workspace with no configuration token yet (it fills the token
  field), so anything that grants one comes after it. Later recipes set up
  what they need through the API (a grant, TEST-Reviewer's app with its bot
  token pasted), so each also runs alone.
- Not covered here (no real Slack can be reached from tests): a real
  workspace's events, rate limits and Slack's own manifest import, update
  and icon upload. The install redirect to an https address from an http
  window is covered by `tests/slack.rs` (YARD_ALLOW_ORIGIN) and here through
  the public address; the browser is never sent to Slack.
