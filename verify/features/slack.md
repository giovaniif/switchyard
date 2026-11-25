# Slack

The Slack connector on the Connectors page, under Messaging. Not set up, the
card shows a four-step guide: Create on Slack (api.slack.com with the app
manifest filled in), Copy manifest, Show manifest, then two token fields
(app-level `xapp-…`, bot `xoxb-…`) checked for their kind before anything is
sent. Connected, the header reads "As @switchyard in TEST-workspace"; a
refused token reads Not working with Slack's error and a Retry. Rows list
people who wrote to the bot and are not allowed (Allow, Dismiss), people who
may talk to agents (Remove), the default agent and a row per channel the bot
was mentioned in, each with an agent picker. The footer shows only each
token's kind and last four characters, with Replace tokens and Disconnect.

A message from an allowed person reaches the agent it names, the thread's
agent, the channel's or the default agent; it shows in the agent's
conversation with a Slack badge (an aubergine dot and "From TEST-Owner on
Slack · #general") above the bubble, the shared origin badge of agents.md, and
the answer is posted back in its Slack thread with :eyes: swapped for
:white_check_mark:.

An agent can also have a Slack app of its own, so people @mention it by its
name: Add to Slack in its header walks through a configuration refresh token
(`xoxe-…`, once per workspace), the app made from a manifest, its install
(Approve in Slack, which comes back to `https://yard.test/slack/oauth`, the
https address yardd serves its page on, though this page is http: no bot
token pasted) and its app-level token (one screen, Open Basic Information),
then says "<Name> is in Slack" with its connection. A mention of its bot goes
to that agent without naming it, and the answer comes back through its app.
With a public address for Slack (opt-in, `YARD_SLACK_PUBLIC_URL` or the
`slack.publicUrl` op; the dialog does not offer it) the configuration token
is the only one: the dialog
numbers two steps, the install comes back to Switchyard on its own, and
Slack's events come over HTTP to `/slack/events`, signed. Its app wears the
agent's face as its icon (a 1024-pixel PNG drawn from the page's avatar),
and a new name or purpose reaches Slack (`apps.manifest.update`), its handle
made from the name (`test-archivist`).

The fake: `verify/fakes/fake-slack` (Web API and Socket Mode on loopback),
started by Setup; `curl "$(v slack)/_fake/message"` makes Slack send a
message, `curl "$(v slack)/_fake/state"` reads what the bot posted and its
reactions. Users UOWNER (TEST-Owner) and USTRANGER; channel C0GENERAL
(#general); a token with "bad" in it is refused.

## Setup

```sh
D=$(mktemp -d /tmp/yv-fake-slack-XXXXXX)
bun verify/fakes/fake-slack --port-file "$D/port" > "$D/log" 2>&1 < /dev/null &
echo $! > "$D/pid"
for i in $(seq 100); do [ -s "$D/port" ] && break; sleep 0.1; done
yv iso up --env "YARD_SLACK_API=http://127.0.0.1:$(cat "$D/port")/api/,YARD_ALLOW_ORIGIN=https://yard.test"
yv var set slack_dir "$D"
yv var set slack "http://127.0.0.1:$(cat "$D/port")"
yv api '{"op":"agent.save","agent":{"name":"TEST-Reviewer","engine":"codex","model":"fake-fast","effort":"low","color":"rose","about":"TEST-reviews pull requests"},"ref":"slack-setup"}' --wait '{"op":"agent.saved"}'
```

## Cleanup

```sh
D=$(v slack_dir)
yv iso down
kill "$(cat "$D/pid")" 2>/dev/null || true
rm -rf "$D"
```

## Recipes

### setup-guide
Not set up: the guide, the manifest (Socket Mode on), and a token pasted in
the wrong field is named as such with Connect disabled. The step numbers and
the token fields share the card's left edge.

```sh
yv api '{"op":"slack.disconnect"}' --wait '{"op":"slack","slack":{"state":"off"}}'
yv drive connectors --steps '[{"expect":"role:heading:Messaging"},{"expect":{"css":".slack-card"},"text":"Not set up"},{"expect":{"role":"link","name":"Create on Slack"}},{"expect":"role:button:Copy manifest"},{"click":"role:button:Show manifest"},{"expect":"css:.slack-manifest","text":"\"socket_mode_enabled\": true"},{"fill":"label:App-level token","value":"xoxb-TEST-wrong-kind"},{"expect":{"css":".slack-token-form"},"text":"That is the bot token"},{"expect":{"css":".slack-token-form button","hasText":"Connect"},"enabled":false},{"eval":"(() => { const steps = document.querySelector(\".slack-steps\").getBoundingClientRect(); const field = document.querySelector(\".slack-field\").getBoundingClientRect(); return Math.abs(steps.left - field.left) < 1 && document.documentElement.scrollWidth <= document.documentElement.clientWidth; })()","equals":true},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), true)","equals":true},{"shot":"setup"}]'
```

### refused-token-then-connected
Viewports: both
A refused bot token shows Not working with Slack's error; replacing only
that token connects, and the header names the bot and workspace.

```sh
yv api '{"op":"slack.disconnect"}' --wait '{"op":"slack","slack":{"state":"off"}}'
yv drive connectors --viewport desktop --steps '[{"fill":"label:App-level token","value":"xapp-1-A0TEST-TESTsecretapptoken"},{"fill":"label:Bot token","value":"xoxb-bad-TESTtoken"},{"click":{"css":".slack-token-form button","hasText":"Connect"}},{"expect":{"css":".slack-card"},"text":"Not working"},{"expect":{"css":".slack-card"},"text":"The bot token was refused (auth.test: invalid_auth)."},{"expect":{"css":".slack-card button","hasText":"Retry"}},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), true)","equals":true},{"shot":"refused"}]'
yv drive connectors --viewport mobile --steps '[{"expect":{"css":".slack-card"},"text":"The bot token was refused (auth.test: invalid_auth)."},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), document.documentElement.scrollWidth <= document.documentElement.clientWidth)","equals":true},{"shot":"refused"}]'
yv drive connectors --viewport desktop --steps '[{"click":{"css":".slack-card button","hasText":"Replace tokens"}},{"fill":"label:Bot token","value":"xoxb-TEST-TESTsecretbottoken"},{"click":{"css":".slack-token-form button","hasText":"Save"}},{"expect":{"css":".slack-card"},"text":"As @switchyard in TEST-workspace"},{"expect":{"css":".slack-card .connector-account-state"},"text":"Connected"},{"shot":"connected"}]'
yv drive connectors --viewport mobile --steps '[{"expect":{"css":".slack-card"},"text":"Connected"},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), document.documentElement.scrollWidth <= document.documentElement.clientWidth)","equals":true},{"shot":"connected"}]'
```

### allow-from-a-direct-message
Viewports: desktop
Someone not on the list who writes to the bot is ignored, told once
(ephemeral), and listed with Allow; Allow moves them to "Can talk to agents".

```sh
yv api '{"op":"slack.tokens","appToken":"xapp-1-A0TEST-TESTsecretapptoken","botToken":"xoxb-TEST-TESTsecretbottoken"}' --wait '{"op":"slack","slack":{"state":"connected"}}'
yv api '{"op":"slack.disallow","user":"UOWNER"}' --wait '{"op":"slack"}'
curl -sf -XPOST "$(v slack)/_fake/message" -d '{"user":"UOWNER","text":"TEST-Reviewer hello"}' > /dev/null
yv drive connectors --viewport desktop --steps '[{"expect":{"css":".slack-card .connector","hasText":"TEST-Owner"},"text":"wrote by direct message · UOWNER"},{"expect":{"css":".slack-card .connector","hasText":"TEST-Owner"},"text":"Not allowed"},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), true)","equals":true},{"shot":"request"},{"click":"role:button:Allow TEST-Owner"},{"expect":{"css":".slack-card .connector","hasText":"TEST-Owner"},"text":"Can talk to agents"},{"shot":"allowed"}]'
curl -sf "$(v slack)/_fake/state" | jq -e '(.ephemerals | length) >= 1 and (.posts | length) == 0'
```

### answer-in-the-slack-thread
Viewports: both
With TEST-Reviewer as the default agent (picked on the card), an @mention in
#general goes to its conversation; the answer is posted in that thread and
the message ends with :white_check_mark:. The conversation shows the message
with its Slack badge (aubergine dot) right-aligned over the bubble, then the
answer.

```sh
yv api '{"op":"slack.tokens","appToken":"xapp-1-A0TEST-TESTsecretapptoken","botToken":"xoxb-TEST-TESTsecretbottoken"}' --wait '{"op":"slack","slack":{"state":"connected"}}'
yv api '{"op":"slack.allow","user":"UOWNER"}' --wait '{"op":"slack"}'
yv drive connectors --viewport desktop --steps '[{"click":"role:button:Default agent"},{"click":{"role":"menuitemradio","name":"TEST-Reviewer"}},{"expect":"role:button:Default agent","text":"TEST-Reviewer"}]'
TS=$(curl -sf -XPOST "$(v slack)/_fake/message" -d '{"user":"UOWNER","channel":"C0GENERAL","text":"<@UBOT> TEST-check the release notes"}' | jq -r .ts)
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '.reactions[$ts] == ["white_check_mark"]' > /dev/null && break; sleep 0.2; done
curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '(.posts | map(select(.thread_ts == $ts and .channel == "C0GENERAL")) | length == 1 and (.[0].text | contains("TEST-check the release notes"))) and .reactions[$ts] == ["white_check_mark"]'
yv drive agents --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"expect":{"css":".origin-badge.origin-slack","hasText":"From TEST-Owner on Slack · #general"}},{"eval":"getComputedStyle(document.querySelector(\".origin-slack .origin-dot\")).backgroundColor","equals":"rgb(74, 21, 75)"},{"expect":{"css":".user-message .bubble","hasText":"TEST-check the release notes"}},{"expect":{"css":".thread"},"text":"Echo:"},{"eval":"(() => { const b = document.querySelector(\".origin-badge\").getBoundingClientRect(); const m = document.querySelector(\".origin-badge + .bubble\").getBoundingClientRect(); return b.bottom <= m.top && Math.abs(b.right - m.right) <= 6 && document.documentElement.scrollWidth <= document.documentElement.clientWidth; })()","equals":true},{"shot":"thread"}]'
```

### tokens-stay-in-yardd
Viewports: desktop
The page shows only each token's kind and last four characters; the tokens
are in no log line, and their file is readable by its owner alone.

```sh
yv api '{"op":"slack.tokens","appToken":"xapp-1-A0TEST-TESTsecretapptoken","botToken":"xoxb-TEST-TESTsecretbottoken"}' --wait '{"op":"slack","slack":{"state":"connected"}}'
yv drive connectors --viewport desktop --steps '[{"expect":"css:.slack-tokens","text":"xapp-…oken · xoxb-…oken"},{"eval":"!document.body.innerText.includes(\"TESTsecret\")","equals":true}]'
! grep -q "TESTsecret" "$(v data)/yardd.log"
ls -l "$(v data)/slack.json" | cut -c1-10 | grep -qx -- "-rw-------"
```

### an-agent-setting-up-is-not-reached-from-slack
Viewports: both
A New agent nobody has set up yet (its placeholder name, "Hi! What should I
do?") is not in the bot's "Which agent?" list, and its placeholder name in
Slack reaches nobody: it still waits for the owner's first words.

```sh
yv api '{"op":"slack.tokens","appToken":"xapp-1-A0TEST-TESTsecretapptoken","botToken":"xoxb-TEST-recipe-bot"}' --wait '{"op":"slack","slack":{"state":"connected"}}'
yv api '{"op":"slack.allow","user":"UOWNER"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.route","agent":""}' --wait '{"op":"slack"}'
yv api '{"op":"agent.new","engine":"codex","model":"fake-fast","effort":"low"}' --wait '{"op":"agent.saved"}' --save blank_name=agent.name
N=$(v blank_name)
TS=$(curl -sf -XPOST "$(v slack)/_fake/message" -d '{"user":"UOWNER","text":"TEST-anyone there?"}' | jq -r .ts)
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '.posts | any(.thread_ts == $ts)' > /dev/null && break; sleep 0.2; done
curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" --arg n "$N" '.posts | map(select(.thread_ts == $ts)) | .[0].text | startswith("Which agent?") and contains("TEST-Reviewer") and (contains($n) | not)'
TS=$(curl -sf -XPOST "$(v slack)/_fake/message" -d "{\"user\":\"UOWNER\",\"text\":\"$N TEST-set yourself up\"}" | jq -r .ts)
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '.posts | any(.thread_ts == $ts)' > /dev/null && break; sleep 0.2; done
curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '.posts | map(select(.thread_ts == $ts)) | .[0].text | startswith("Which agent?")'
yv drive agents --steps '[{"click":"css:.agent-card[aria-label=\"{{blank_name}}\"] .agent-message"},{"expect":"role:heading:Hi! What should I do?"},{"expect":"css:.user-message","count":0},{"expect":"role:button:Discard"},{"shot":"still-blank"}]'
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

## Gotchas

- The land gate's secret check (`verify/src/land.ts`, `SECRET_TEXT`) blocks
  a PR whose added lines hold `xoxb-`/`xoxp-` plus 20 or more characters,
  fake or not. A new recipe that sets tokens uses a short bot token
  (`xoxb-TEST-recipe-bot`); yardd and the fake only need `xoxb-` and 12
  characters.
- Not covered here (no real Slack can be reached from tests): a real
  workspace's events, rate limits and Slack's own manifest import, update
  and icon upload. The install redirect to an https address from an http
  window is covered by `tests/slack.rs` (YARD_ALLOW_ORIGIN) and here through
  the public address; the browser is never sent to Slack.
