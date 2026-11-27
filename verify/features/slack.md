# Slack

The Slack connector on the Connectors page, under Messaging, the owner's own
Slack in the sidebar (his DMs and group DMs, read and answered as him), and
agents' own Slack apps. The card connects the owner's own Slack: an app of his with
user scopes (his direct messages and the channels he is in, as him), made from
a configuration token (one click for a workspace an agent's app already has),
approved in Slack (which comes back to `https://yard.test/slack/oauth`, the
https address yardd serves its page on, though the page is http), then one
app-level token. Its steps show as Token, Approve, Connect; connected, the
header reads "As @test-owner in TEST-workspace" and the footer shows only each
token's kind and last four characters, with Disconnect. Below it, people
trusted to request tool use are listed with Remove. (The shared Switchyard bot, its default agent and
its channel rows were retired.)

An agent has a Slack app of its own, so people @mention it by its name: Add
to Slack in its header walks through a configuration refresh token (`xoxe-…`,
once per workspace), the app made from a manifest, its install (Approve in
Slack, no bot token pasted) and its app-level token (one screen, Open Basic
Information), then says "<Name> is in Slack" with its connection. A message
from a person reaches its own session for that DM or channel thread. It shows
with a Slack badge (an aubergine dot and "From TEST-Owner on Slack · #general") above
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
yv iso down --name map-slack-remote || true
kill "$(cat "$D/pid")" 2>/dev/null || true
rm -rf "$D"
```

## Recipes

### not-connected
Viewports: both
Not connected, the card says what it is for, numbers three steps (Token
first) and asks for the configuration refresh token, with Get config token
opening Slack's app settings. It explains who may request tool use.

```sh
yv drive connectors --viewport desktop --steps '[{"expect":"role:heading:Messaging"},{"expect":{"css":".slack-card .connector-account-state"},"text":"Not connected"},{"expect":"css:.slack-card .slack-progress li","count":3},{"expect":{"css":".slack-card .slack-progress li[aria-current=\"step\"]","hasText":"Token"}},{"expect":{"css":".slack-card"},"text":"Your direct messages and the channels you choose"},{"expect":"label:Configuration refresh token"},{"eval":"document.querySelector(\".slack-card a.login-open\").href","equals":"https://api.slack.com/apps"},{"expect":{"css":".slack-card"},"text":"Who can request tool use"},{"eval":"!document.querySelector(\".slack-card\").innerText.includes(\"Default agent\")","equals":true},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), true)","equals":true},{"shot":"not-connected"}]'
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
yv drive agents --viewport mobile --steps '[{"expect":{"css":".agent-card[aria-label=\"TEST-Reviewer\"]","hasText":"In Slack"}},{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"expect":"role:heading:Slack places"},{"click":{"css":".agent-place","hasText":"#general · thread"}},{"expect":{"css":".origin-badge.origin-slack","hasText":"From TEST-Owner on Slack · #general"}}]'
```

### allow-from-an-agents-app
Viewports: desktop
A guest's Codex request waits in its DM place. The app DMs its installer with
all three decisions. A guest's own click is acknowledged and leaves the card
open. Always allow in Switchyard approves this request and adds the person
to the tool-use list, where Remove revokes future trust.

```sh
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","agentId":"{{reviewer}}","origin":"http://127.0.0.1","ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.botToken","agentId":"{{reviewer}}","token":"xoxb-agent-TEST","ref":"bot"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.appToken","agentId":"{{reviewer}}","token":"xapp-1-agent-TEST","ref":"app"}' --wait '{"op":"slack"}'
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e '.connections >= 1' > /dev/null && break; sleep 0.1; done
TS=$(curl -sf -XPOST "$(v slack)/_fake/message" -d '{"app":"agent","channel":"DSTRANGER","user":"USTRANGER","text":"TEST-read the release checklist and summarize the verification steps for this deployment"}' | jq -r .ts)
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e '.posts | any(.channel == "D0OWNER" and .blocks[1].type == "actions")' > /dev/null && break; sleep 0.1; done
DM=$(curl -sf "$(v slack)/_fake/state" | jq -ec '.posts[] | select(.channel == "D0OWNER" and .blocks[1].type == "actions")')
echo "$DM" | jq -e '.app == "agent" and ([.blocks[1].elements[].text.text] == ["Approve once", "Always allow TEST-Stranger", "Deny"])'
CLICK=$(echo "$DM" | jq -c '{channel, ts, user:"USTRANGER", action:"slack_access_allow", value:.blocks[1].elements[0].value}')
ACK=$(curl -sf -XPOST "$(v slack)/_fake/action" -d "$CLICK" | jq -r .envelope)
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e --arg ack "$ACK" '.acks | index($ack)' > /dev/null && break; sleep 0.1; done
for vp in desktop mobile; do
yv drive agents --viewport "$vp" --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"expect":"role:heading:Slack places"},{"expect":{"css":".agent-place","hasText":"DM with TEST-Stranger"},"text":"Needs approval"},{"expect":"css:.agent-place time"},{"shot":"places"},{"click":{"css":".agent-place","hasText":"DM with TEST-Stranger"}},{"expect":"css:.approval-title","text":"Approve tool access for this Slack request?"},{"expect":"css:.approval-what","text":"TEST-read the release checklist"},{"expect":"role:button:Approve once","enabled":true},{"expect":"role:button:Always allow TEST-Stranger","enabled":true},{"expect":"role:button:Deny","enabled":true},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"approval"}]'
done
yv drive agents --viewport desktop --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"click":{"css":".agent-place","hasText":"DM with TEST-Stranger"}},{"click":"role:button:Always allow TEST-Stranger"},{"expect":"css:.approval","text":"Always allowed"},{"expect":"css:.thread","text":"Echo:"},{"shot":"approved"}]'
for i in $(seq 100); do curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '.reactions[$ts] == ["white_check_mark"]' > /dev/null && break; sleep 0.1; done
curl -sf "$(v slack)/_fake/state" | jq -e --arg ts "$TS" '(.ephemerals | length == 0) and (.posts | any(.channel == "D0OWNER" and .blocks == [] and (.text | startswith("Always allowed")))) and .reactions[$ts] == ["white_check_mark"]'
yv drive connectors --viewport desktop --steps '[{"expect":{"css":".slack-card .connector","hasText":"TEST-Stranger"},"text":"Can request tool use"},{"shot":"allowed"},{"click":"role:button:Remove TEST-Stranger"},{"click":"role:button:Confirm removing TEST-Stranger"},{"expect":{"css":".slack-card .connector","hasText":"TEST-Stranger"},"count":0}]'
```

### answer-in-the-slack-thread
Viewports: both
A mention of TEST-Reviewer's app in #general goes to a new place session for that thread. The
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
yv drive agents --viewport $vp --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"expect":"role:heading:Slack places"},{"click":{"css":".agent-place","hasText":"#general · thread","nth":0}},{"expect":{"css":".origin-badge.origin-slack","hasText":"From TEST-Owner on Slack · #general"}},{"eval":"getComputedStyle(document.querySelector(\".origin-slack .origin-dot\")).backgroundColor","equals":"rgb(74, 21, 75)"},{"expect":{"css":".user-message .bubble","hasText":"TEST-check the release notes"}},{"expect":{"css":".thread"},"text":"Echo:"},{"eval":"(() => { const all = document.querySelectorAll(\".origin-badge\"); const b = all[all.length - 1].getBoundingClientRect(); const m = all[all.length - 1].nextElementSibling.getBoundingClientRect(); return b.bottom <= m.top && Math.abs(b.right - m.right) <= 6 && document.documentElement.scrollWidth <= document.documentElement.clientWidth; })()","equals":true},{"shot":"thread"}]'
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

### your-direct-messages
Viewports: both
Connected, the owner's DMs and group DMs with a message in the last 60 days
are in the sidebar's Slack section, newest first, with unread counts (the
group DM has one); one quiet for longer is not. A conversation shows its
messages grouped by author and day, Slack's markup as text (bold, a link, a
mention by name), reactions, and "1 reply"; the thread opens in a pane (the
whole screen on a phone). Load older brings the messages older than 60 days
from Slack, and then there is no more to load.

```sh
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","me":true,"ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.userToken","token":"xoxp-you-TEST","ref":"user"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.appToken","token":"xapp-1-you-TEST","ref":"app"}' --wait '{"op":"slack"}'
for i in $(seq 100); do yv state --full 2>/dev/null | grep -q D0ANA && break; sleep 0.2; done
yv drive 'slack:TEST-Ana' --viewport desktop --steps '[{"expect":{"css":".slack-dms .slack-row","hasText":"TEST-Ana, TEST-Stranger"},"text":"1"},{"eval":"!document.querySelector(\".slack-dms\").innerText.includes(\"TEST-Old\")","equals":true},{"expect":"role:heading:TEST-Ana"},{"expect":{"css":".slack-text strong","hasText":"bold"}},{"expect":{"css":".slack-text a","hasText":"a link"}},{"eval":"document.querySelector(\".slack-text a\").getAttribute(\"href\")","equals":"https://example.com"},{"expect":{"css":".slack-mention","hasText":"@TEST-Ana"}},{"expect":"role:button:thumbsup, 1"},{"shot":"dm"},{"click":"role:button:Load older messages"},{"expect":{"css":".slack-messages"},"text":"TEST-old news"},{"expect":"role:button:Load older messages","count":0},{"click":"role:button:1 reply"},{"expect":{"css":".slack-thread"},"text":"TEST-sure, noon"},{"shot":"thread"}]'
yv drive 'slack:TEST-Ana' --viewport mobile --steps '[{"expect":{"css":".slack-messages"},"text":"TEST-lunch tomorrow?"},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"dm"},{"click":"role:button:1 reply"},{"expect":{"css":".slack-thread"},"text":"TEST-sure, noon"},{"eval":"getComputedStyle(document.querySelector(\".slack-main\")).display","equals":"none"},{"shot":"thread"}]'
```

### reply-react-and-read
Viewports: both
From Switchyard, as the owner: a message posted in the DM shows at once and
reaches Slack with his user token (what he typed stays plain text); a
reaction from a message's actions; a reply in the thread. Opening the group
DM reads it: its unread count goes, here and in Slack (`conversations.mark`).

```sh
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","me":true,"ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.userToken","token":"xoxp-you-TEST","ref":"user"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.appToken","token":"xapp-1-you-TEST","ref":"app"}' --wait '{"op":"slack"}'
for i in $(seq 100); do yv state --full 2>/dev/null | grep -q D0ANA && break; sleep 0.2; done
for vp in desktop mobile; do
# Each viewport reacts on a message of its own: the same reaction again would take it off.
if [ $vp = desktop ]; then M="TEST-yes,"; else M="TEST-are you around"; fi
yv drive 'slack:TEST-Ana' --viewport $vp --steps '[{"fill":"label:Message TEST-Ana","value":"TEST-on my way <'$vp'>"},{"click":"role:button:Send"},{"expect":{"css":".slack-messages"},"text":"TEST-on my way <'$vp'>"},{"click":{"css":".slack-message","hasText":"'"$M"'"}},{"click":"role:button:React white_check_mark"},{"expect":"role:button:white_check_mark, 1"},{"shot":"sent"},{"click":"css:.slack-replies"},{"fill":"label:Reply in the thread","value":"TEST-thread reply '$vp'"},{"click":{"css":".slack-thread button[aria-label=Send]"}},{"expect":{"css":".slack-thread"},"text":"TEST-thread reply '$vp'"},{"shot":"thread-reply"}]'
done
curl -sf "$(v slack)/_fake/state" | jq -e '(.posts | map(select(.app == "you")) | (any(.text == "TEST-on my way &lt;desktop&gt;") and any(.thread_ts != null and (.text | startswith("TEST-thread reply"))))) and (.calls | any(.method == "reactions.add" and .token == "user"))'
yv drive 'slack:TEST-Stranger' --viewport desktop --steps '[{"expect":{"css":".slack-messages"},"text":"TEST-hello all"},{"expect":{"css":".slack-row.active .slack-unread"},"count":0}]'
for i in $(seq 50); do curl -sf "$(v slack)/_fake/state" | jq -e '.marks | any(.channel == "G0TRIO")' > /dev/null && break; sleep 0.2; done
curl -sf "$(v slack)/_fake/state" | jq -e '.marks | any(.channel == "G0TRIO")'
```

### someone-new-writes
Viewports: desktop
A first message from someone the owner never wrote to (a DM yardd has not
seen) comes in live: the new conversation tops the Slack section with one
unread, and opens.

```sh
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","me":true,"ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.userToken","token":"xoxp-you-TEST","ref":"user"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.appToken","token":"xapp-1-you-TEST","ref":"app"}' --wait '{"op":"slack"}'
for i in $(seq 100); do yv state --full 2>/dev/null | grep -q D0ANA && break; sleep 0.2; done
curl -sf -XPOST "$(v slack)/_fake/message" -d '{"app":"you","user":"UNEW","channel":"D0NEW","text":"TEST-hi, I am new here"}' > /dev/null
yv drive 'slack:TEST-UNEW' --viewport desktop --steps '[{"expect":{"css":".slack-messages"},"text":"TEST-hi, I am new here"},{"expect":"role:heading:TEST-UNEW"},{"shot":"new-dm"}]'
```

### import-a-channel
Viewports: both
Connected, the card says direct messages all come and channels only when
imported. Choose channels lists the ones he is in (#general, and
#secret-plans marked private), each with an Import switch; switched on,
#general's messages load and it shows in the sidebar's Slack section with a
# mark, and opens like a DM. Switched off, it leaves the sidebar.

```sh
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","me":true,"ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.userToken","token":"xoxp-you-TEST","ref":"user"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.appToken","token":"xapp-1-you-TEST","ref":"app"}' --wait '{"op":"slack"}'
for i in $(seq 100); do yv state --full 2>/dev/null | grep -q D0ANA && break; sleep 0.2; done
yv api '{"op":"slack.inbox.import","channel":"C0GENERAL","on":false,"ref":"off"}' --wait '{"op":"slack.inbox.imported"}'
yv drive connectors --viewport desktop --steps '[{"expect":{"css":".slack-channels"},"text":"all of them, new ones too"},{"click":"role:button:Choose channels"},{"expect":{"role":"switch","name":"Import #general"}},{"expect":{"css":".slack-channel","hasText":"#secret-plans"},"text":"private"},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), true)","equals":true},{"shot":"channels"},{"click":{"role":"switch","name":"Import #general"}},{"eval":"document.querySelector(\"[aria-label=\\\"Import #general\\\"]\").getAttribute(\"aria-checked\")","equals":"true"},{"expect":{"css":".slack-dms .slack-row","hasText":"#general"}}]'
yv drive 'slack:#general' --viewport desktop --steps '[{"expect":"role:heading:#general"},{"expect":{"css":".slack-messages"},"text":"TEST-shipping today"},{"expect":{"css":".slack-row.active .slack-face"},"text":"#"},{"shot":"channel"}]'
yv drive 'slack:#general' --viewport mobile --steps '[{"expect":{"css":".slack-messages"},"text":"TEST-standup in 5"},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"channel"}]'
yv drive connectors --viewport mobile --steps '[{"click":"role:button:Choose channels"},{"click":{"role":"switch","name":"Import #general"}},{"eval":"document.querySelector(\"[aria-label=\\\"Import #general\\\"]\").getAttribute(\"aria-checked\")","equals":"false"},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), document.documentElement.scrollWidth <= document.documentElement.clientWidth)","equals":true},{"shot":"channels"}]'
for i in $(seq 50); do yv state --full 2>/dev/null | grep -q C0GENERAL || break; sleep 0.2; done
```

### call-an-agent-from-slack
Viewports: both
In a Slack conversation's reply box, @ offers the agents whose own app is in
that workspace; picking one writes its handle, the box says it calls it, and
the message goes to Slack with a real mention of its bot (so its app answers
there, in a place of its own, if it is in the conversation). Typed by hand in
any case (`@TEST-Reviewer`, as its name shows) it is a call too, exactly when
the box says so. Anything else he types stays plain text.

```sh
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","agentId":"{{reviewer}}","origin":"http://127.0.0.1","ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.botToken","agentId":"{{reviewer}}","token":"xoxb-agent-TEST","ref":"bot"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.appToken","agentId":"{{reviewer}}","token":"xapp-1-agent-TEST","ref":"app"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","me":true,"ref":"grant2"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.userToken","token":"xoxp-you-TEST","ref":"user"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.appToken","token":"xapp-1-you-TEST","ref":"app2"}' --wait '{"op":"slack"}'
for i in $(seq 100); do yv state --full 2>/dev/null | grep -q G0TRIO && break; sleep 0.2; done
for vp in desktop mobile; do
yv drive 'slack:TEST-Stranger' --viewport $vp --steps '[{"click":"label:Message TEST-Ana, TEST-Stranger"},{"type":"@rev"},{"expect":{"role":"option","name":"TEST-Reviewer @test-reviewer"}},{"shot":"suggest"},{"click":{"role":"option","name":"TEST-Reviewer @test-reviewer"}},{"type":"check the deploy '$vp'"},{"expect":{"css":".slack-compose"},"text":"Calls TEST-Reviewer"},{"click":"role:button:Send"},{"expect":{"css":".slack-messages"},"text":"check the deploy '$vp'"},{"shot":"called"},{"fill":"label:Message TEST-Ana, TEST-Stranger","value":"@TEST-Reviewer typed '$vp'"},{"expect":{"css":".slack-compose"},"text":"Calls TEST-Reviewer"},{"click":"role:button:Send"},{"expect":{"css":".slack-messages"},"text":"typed '$vp'"}]'
done
curl -sf "$(v slack)/_fake/state" | jq -e '[.posts[] | select(.app == "you" and (.text | startswith("<@UAGENT> check the deploy")))] | length == 2'
curl -sf "$(v slack)/_fake/state" | jq -e '[.posts[] | select(.app == "you" and (.text | startswith("<@UAGENT> typed")))] | length == 2'
```

### call-an-agent-on-another-host
Viewports: both
His Slack is this yardd's; TEST-Far runs on a host (TEST-slack-box, a second
isolated yardd on the same fake Slack) with its own app there (@test-far, bot
UAGENT2), and TEST-Reviewer here (@test-reviewer, UAGENT). The reply box
offers both, and one message calls both: each handle goes to Slack as a real
mention of its own bot, and in his inbox each reads as its agent. With the
host gone from this yardd, TEST-Far is still offered and still called: the
mention goes through Slack to its app, not through this yardd. Its app's
icon is not sent to the offline host, so no window size gets a refusal.

```sh
yv var set slack_remote_port "$(yv iso up --name map-slack-remote --env "YARD_SLACK_API=$(v slack)/api/,YARD_ALLOW_ORIGIN=https://yard.test" | jq -r .iso.port)"
yv api '{"op":"host.add","name":"TEST-slack-box","kind":"direct","url":"ws://127.0.0.1:{{slack_remote_port}}/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-slack-box","status":"connected","ownAgents":true}]}' --save slack_box='hosts[name=TEST-slack-box].id'
yv api '{"op":"agent.save","agent":{"name":"TEST-Far","engine":"codex","model":"fake-fast","effort":"low","color":"teal","about":"TEST-runs on the server","host":"{{slack_box}}"},"ref":"far"}' --wait '{"op":"agent.saved"}' --save far=agent.id
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","agentId":"{{far}}","origin":"http://127.0.0.1","ref":"grant-far"}' --wait '{"op":"slack","ref":"grant-far"}'
yv api '{"op":"slack.app.botToken","agentId":"{{far}}","token":"xoxb-agent-2-TEST","ref":"bot-far"}' --wait '{"op":"slack","ref":"bot-far"}'
yv api '{"op":"slack.app.appToken","agentId":"{{far}}","token":"xapp-1-agent-TEST","ref":"app-far"}' --wait '{"op":"slack","ref":"app-far"}'
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","agentId":"{{reviewer}}","origin":"http://127.0.0.1","ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.botToken","agentId":"{{reviewer}}","token":"xoxb-agent-TEST","ref":"bot"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.appToken","agentId":"{{reviewer}}","token":"xapp-1-agent-TEST","ref":"app"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","me":true,"ref":"grant2"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.userToken","token":"xoxp-you-TEST","ref":"user"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.appToken","token":"xapp-1-you-TEST","ref":"app2"}' --wait '{"op":"slack"}'
for i in $(seq 100); do yv state --full 2>/dev/null | grep -q G0TRIO && break; sleep 0.2; done
for vp in desktop mobile; do
yv drive 'slack:TEST-Stranger' --viewport $vp --steps '[{"click":"label:Message TEST-Ana, TEST-Stranger"},{"type":"@"},{"expect":{"role":"option","name":"TEST-Reviewer @test-reviewer"}},{"expect":{"role":"option","name":"TEST-Far @test-far"}},{"shot":"suggest-both"},{"type":"test-f"},{"click":{"role":"option","name":"TEST-Far @test-far"}},{"type":"and @test-r"},{"click":{"role":"option","name":"TEST-Reviewer @test-reviewer"}},{"type":"look '$vp'"},{"expect":{"css":".slack-compose"},"text":"Calls TEST-Reviewer, TEST-Far"},{"click":"role:button:Send"},{"expect":{"css":".slack-messages"},"text":"look '$vp'"},{"expect":{"css":".slack-messages .slack-mention","hasText":"TEST-Far"}},{"shot":"called-both"}]'
done
curl -sf "$(v slack)/_fake/state" | jq -e '[.posts[] | select(.app == "you" and (.text | startswith("<@UAGENT2> and <@UAGENT> look")))] | length == 2'
yv iso down --name map-slack-remote
for i in $(seq 100); do yv state --full 2>/dev/null | jq -e '.snapshot.hosts | any(.name == "TEST-slack-box" and .status != "connected")' > /dev/null && break; sleep 0.2; done
yv state --full | jq -e '.snapshot.hosts | any(.name == "TEST-slack-box" and .status != "connected")'
for vp in desktop mobile; do
yv drive 'slack:TEST-Stranger' --viewport $vp --steps '[{"click":"label:Message TEST-Ana, TEST-Stranger"},{"type":"@test-f"},{"click":{"role":"option","name":"TEST-Far @test-far"}},{"type":"still there '$vp'?"},{"expect":{"css":".slack-compose"},"text":"Calls TEST-Far"},{"click":"role:button:Send"},{"expect":{"css":".slack-messages"},"text":"still there '$vp'?"},{"shot":"called-offline"}]'
done
curl -sf "$(v slack)/_fake/state" | jq -e '[.posts[] | select(.app == "you" and (.text | startswith("<@UAGENT2> still there")))] | length == 2'
# The recipes after this one know no such host.
yv api '{"op":"host.remove","hostId":"{{slack_box}}"}' --wait '{"op":"hosts"}'
```

### pick-an-agent-with-the-keys
Viewports: both
With two agents' apps in the workspace (TEST-Reviewer, @test-reviewer, bot
UAGENT; TEST-Reviewer Two, @test-reviewer-two, bot UAGENT2), the reply box's
@ list works like Slack's own picker: the first row is highlighted (the
textarea's `aria-activedescendant`, the row's `aria-selected`), Down and Up
move it round the ends, Enter writes the highlighted handle (a bare `@` is
never sent), Escape closes the list without sending and typing opens it
again. A handle typed in full is a complete mention: Enter sends it. The
thread pane's reply box does the same. The rows' text is centred in them. On
a phone the list fits the screen and a tap picks.

```sh
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","agentId":"{{reviewer}}","origin":"http://127.0.0.1","ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.botToken","agentId":"{{reviewer}}","token":"xoxb-agent-TEST","ref":"bot"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.appToken","agentId":"{{reviewer}}","token":"xapp-1-agent-TEST","ref":"app"}' --wait '{"op":"slack"}'
yv api '{"op":"agent.save","agent":{"name":"TEST-Reviewer Two","engine":"codex","model":"fake-fast","effort":"low","color":"teal","about":"TEST-reviews too"},"ref":"two"}' --wait '{"op":"agent.saved"}' --save reviewer_two=agent.id
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","agentId":"{{reviewer_two}}","origin":"http://127.0.0.1","ref":"grant-two"}' --wait '{"op":"slack","ref":"grant-two"}'
yv api '{"op":"slack.app.botToken","agentId":"{{reviewer_two}}","token":"xoxb-agent-2-TEST","ref":"bot-two"}' --wait '{"op":"slack","ref":"bot-two"}'
yv api '{"op":"slack.app.appToken","agentId":"{{reviewer_two}}","token":"xapp-1-agent-TEST","ref":"app-two"}' --wait '{"op":"slack","ref":"app-two"}'
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","me":true,"ref":"grant2"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.userToken","token":"xoxp-you-TEST","ref":"user"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.appToken","token":"xapp-1-you-TEST","ref":"app2"}' --wait '{"op":"slack"}'
for i in $(seq 100); do yv state --full 2>/dev/null | grep -q G0TRIO && break; sleep 0.2; done
# The highlighted row, as the textarea names it.
HL='{"eval":"document.getElementById(document.activeElement.getAttribute(\"aria-activedescendant\")).textContent"'
ONE='{"eval":"document.querySelectorAll(\"[role=option][aria-selected=true]\").length","equals":1}'
CENTRED='{"eval":"[...document.querySelectorAll(\"[role=option]\")].every((o) => { const r = o.getBoundingClientRect(), t = o.querySelector(\"strong\").getBoundingClientRect(); return Math.abs((t.top + t.bottom) / 2 - (r.top + r.bottom) / 2) <= 2; })","equals":true}'
yv drive 'slack:TEST-Stranger' --viewport desktop --steps '[{"click":"label:Message TEST-Ana, TEST-Stranger"},{"type":"@test-rev"},{"expect":"role:option","count":2},'"$HL"',"equals":"TEST-Reviewer@test-reviewer"},'"$ONE"','"$CENTRED"',{"press":"ArrowDown"},'"$HL"',"equals":"TEST-Reviewer Two@test-reviewer-two"},'"$ONE"',{"shot":"highlighted"},{"press":"ArrowDown"},'"$HL"',"equals":"TEST-Reviewer@test-reviewer"},{"press":"ArrowUp"},'"$HL"',"equals":"TEST-Reviewer Two@test-reviewer-two"},{"press":"Enter"},{"expect":"role:option","count":0},{"expect":"label:Message TEST-Ana, TEST-Stranger","value":"@test-reviewer-two "},{"type":"keys desktop"},{"expect":{"css":".slack-compose"},"text":"Calls TEST-Reviewer Two"},{"press":"Enter"},{"expect":{"css":".slack-messages"},"text":"keys desktop"},{"type":"@"},{"expect":"role:option","count":2},{"press":"Enter"},{"expect":"label:Message TEST-Ana, TEST-Stranger","value":"@test-reviewer "},{"fill":"label:Message TEST-Ana, TEST-Stranger","value":""},{"type":"@"},{"press":"Escape"},{"expect":"role:option","count":0},{"expect":"label:Message TEST-Ana, TEST-Stranger","value":"@"},{"type":"t"},{"expect":"role:option","count":2},{"fill":"label:Message TEST-Ana, TEST-Stranger","value":""},{"type":"ping exact desktop @test-reviewer"},{"expect":"role:option","count":2},'"$HL"',"equals":"TEST-Reviewer@test-reviewer"},{"press":"Enter"},{"expect":{"css":".slack-messages"},"text":"ping exact desktop"},{"expect":"label:Message TEST-Ana, TEST-Stranger","value":""},{"shot":"sent"},{"click":{"css":".slack-message","hasText":"keys desktop"}},{"click":"role:button:Reply in thread"},{"click":"label:Reply in the thread"},{"type":"@test-rev"},{"expect":{"css":".slack-thread [role=option]"},"count":2},{"press":"ArrowDown"},'"$HL"',"equals":"TEST-Reviewer Two@test-reviewer-two"},{"shot":"thread-highlighted"},{"press":"Escape"},{"expect":"role:option","count":0},{"expect":"css:.slack-thread"},{"type":"-"},{"press":"Backspace"},{"press":"ArrowDown"},{"press":"Enter"},{"expect":"label:Reply in the thread","value":"@test-reviewer-two "},{"type":"thread keys"},{"press":"Enter"},{"expect":{"css":".slack-thread"},"text":"thread keys"}]'
yv drive 'slack:TEST-Stranger' --viewport mobile --steps '[{"click":"label:Message TEST-Ana, TEST-Stranger"},{"type":"@test-rev"},{"expect":"role:option","count":2},'"$CENTRED"',{"eval":"(r => r.left >= 0 && r.right <= innerWidth && r.top >= 0)(document.querySelector(\".slack-compose .mention-list\").getBoundingClientRect())","equals":true},{"shot":"suggest"},{"tap":{"role":"option","name":"TEST-Reviewer Two @test-reviewer-two"}},{"expect":"role:option","count":0},{"type":"tap mobile"},{"click":"role:button:Send"},{"expect":{"css":".slack-messages"},"text":"tap mobile"},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"sent"},{"click":{"css":".slack-message","hasText":"tap mobile"}},{"click":"role:button:Reply in thread"},{"click":"label:Reply in the thread"},{"type":"@test-rev"},{"expect":{"css":".slack-thread [role=option]"},"count":2},{"eval":"(r => r.left >= 0 && r.right <= innerWidth && r.top >= 0)(document.querySelector(\".slack-thread .mention-list\").getBoundingClientRect())","equals":true},{"shot":"thread-suggest"},{"tap":{"role":"option","name":"TEST-Reviewer Two @test-reviewer-two"}},{"type":"thread tap mobile"},{"click":{"css":".slack-thread button[aria-label=Send]"}},{"expect":{"css":".slack-thread"},"text":"thread tap mobile"}]'
S=$(curl -sf "$(v slack)/_fake/state")
echo "$S" | jq -e '[.posts[] | select(.app == "you" and .thread_ts == null and (.text | startswith("<@UAGENT2> keys desktop")))] | length == 1'
echo "$S" | jq -e '[.posts[] | select(.app == "you" and .text == "ping exact desktop <@UAGENT>")] | length == 1'
echo "$S" | jq -e '[.posts[] | select(.app == "you" and .thread_ts != null and (.text | startswith("<@UAGENT2> thread keys")))] | length == 1'
echo "$S" | jq -e '[.posts[] | select(.app == "you" and .thread_ts == null and (.text | startswith("<@UAGENT2> tap mobile")))] | length == 1'
echo "$S" | jq -e '[.posts[] | select(.app == "you" and .thread_ts != null and (.text | startswith("<@UAGENT2> thread tap mobile")))] | length == 1'
# A bare @ never went out, nor a half-typed handle.
echo "$S" | jq -e '[.posts[] | select(.app == "you" and (.text == "@" or (.text | test("@test-rev"))))] | length == 0'
# The recipes after this one know one agent's app.
yv api '{"op":"slack.app.remove","agentId":"{{reviewer_two}}","ref":"rm-two"}' --wait '{"op":"slack","ref":"rm-two"}'
```

### one-icon-per-picture
Viewports: both
An agent's app gets its face as its icon once: a window of either size works
out the same picture key, so opening the page at desktop and then at mobile
uploads one icon, not one per window size.

```sh
yv api '{"op":"agent.save","agent":{"name":"TEST-Painter","engine":"codex","model":"fake-fast","effort":"low","color":"amber","face":"glasses"},"ref":"painter"}' --wait '{"op":"agent.saved"}' --save painter=agent.id
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","agentId":"{{painter}}","origin":"http://127.0.0.1","ref":"grant"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.botToken","agentId":"{{painter}}","token":"xoxb-agent-TEST","ref":"bot"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.app.appToken","agentId":"{{painter}}","token":"xapp-1-agent-TEST","ref":"app"}' --wait '{"op":"slack"}'
key() { yv state --full | jq -r --arg id "$(v painter)" '.snapshot.slack.agentApps[$id].iconKey // ""'; }
icons() { curl -sf "$(v slack)/_fake/state" | jq '.icons | length'; }
N=$(icons)
yv drive agents --viewport desktop --steps '[{"expect":{"css":".agent-card[aria-label=\"TEST-Painter\"]"}},{"shot":"painter"}]'
for i in $(seq 100); do [ -n "$(key)" ] && [ "$(icons)" -gt "$N" ] && break; sleep 0.2; done
sleep 1
K=$(key); N=$(icons)
test -n "$K"
yv drive agents --viewport mobile --steps '[{"expect":{"css":".agent-card[aria-label=\"TEST-Painter\"]"}},{"wait":2000},{"shot":"painter"}]'
sleep 1
test "$(key)" = "$K"
test "$(icons)" = "$N"
yv api '{"op":"slack.app.remove","agentId":"{{painter}}","ref":"rm"}' --wait '{"op":"slack"}'
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

### double-click-on-disconnect-only-asks
Viewports: both
A double click on Disconnect (the owner's own Slack) or on a trusted person's
Remove leaves the ask up and changes nothing. Each turns into its Confirm in
the same button, so the second click of a double click lands on it; it used
to answer it, and his Slack app was deleted or the person lost tool use at
once (like #519 for a session's Delete, #530 for a connector's Remove). The
eval sends what a browser sends for a double click: two mousedown, mouseup,
click runs at the same point, the second with `detail: 2`. A separate click
on Confirm still removes the person and disconnects. Each viewport connects
first and trusts a person of its own (earlier recipes trust UOWNER
under its ID), so the recipe runs alone too; it comes last, as it disconnects.

```sh
for vp in desktop mobile; do
yv api '{"op":"slack.grant","token":"xoxe-1-TEST-refresh","me":true,"ref":"grant-'"$vp"'"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.userToken","token":"xoxp-you-TEST","ref":"user-'"$vp"'"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.me.appToken","token":"xapp-1-you-TEST","ref":"app-'"$vp"'"}' --wait '{"op":"slack"}'
yv api '{"op":"slack.allow","user":"U0TESTDBL","name":"TEST-Doubler","ref":"allow-'"$vp"'"}' --wait '{"op":"slack"}'
yv drive connectors --viewport "$vp" --steps '[{"expect": {"css": ".slack-card"}, "text": "As @test-owner in TEST-workspace"}, {"expect": {"css": ".slack-card .connector", "hasText": "TEST-Doubler"}, "text": "Can request tool use"}, {"eval": "(async () => { const b = [...document.querySelectorAll(\"button\")].find((x) => x.getAttribute(\"aria-label\") === \"Remove TEST-Doubler\"); b.scrollIntoView({ block: \"center\" }); const r = b.getBoundingClientRect(); const x = r.left + r.width / 2, y = r.top + r.height / 2; const click = (el, detail) => { for (const type of [\"mousedown\", \"mouseup\", \"click\"]) el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window, button: 0, clientX: x, clientY: y, detail })); }; click(b, 1); await new Promise((res) => setTimeout(res, 0)); const second = document.elementFromPoint(x, y).closest(\"button\"); const label = second ? second.getAttribute(\"aria-label\") : \"none\"; second && click(second, 2); await new Promise((res) => setTimeout(res, 1500)); return \"second click hit: \" + label; })()"}, {"expect": "role:button:Confirm removing TEST-Doubler"}, {"expect": {"css": ".slack-card .connector", "hasText": "TEST-Doubler"}}, {"eval": "(async () => { const b = [...document.querySelectorAll(\"button\")].find((x) => x.getAttribute(\"aria-label\") === \"Disconnect your Slack\"); b.scrollIntoView({ block: \"center\" }); const r = b.getBoundingClientRect(); const x = r.left + r.width / 2, y = r.top + r.height / 2; const click = (el, detail) => { for (const type of [\"mousedown\", \"mouseup\", \"click\"]) el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window, button: 0, clientX: x, clientY: y, detail })); }; click(b, 1); await new Promise((res) => setTimeout(res, 0)); const second = document.elementFromPoint(x, y).closest(\"button\"); const label = second ? second.getAttribute(\"aria-label\") : \"none\"; second && click(second, 2); await new Promise((res) => setTimeout(res, 1500)); return \"second click hit: \" + label; })()"}, {"expect": "role:button:Confirm disconnecting your Slack"}, {"expect": {"css": ".slack-card"}, "text": "As @test-owner in TEST-workspace"}, {"shot": "double-click-asks"}, {"click": "role:button:Confirm removing TEST-Doubler"}, {"expect": {"css": ".slack-card .connector", "hasText": "TEST-Doubler"}, "count": 0}, {"click": "role:button:Confirm disconnecting your Slack"}, {"expect": {"css": ".slack-card .connector-account-state"}, "text": "Not connected"}]'
done
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
- The fake has one second bot (UAGENT2, `xoxb-agent-2-TEST`), shared by
  TEST-Far in `call-an-agent-on-another-host` and TEST-Reviewer Two in
  `pick-an-agent-with-the-keys`; the inbox names `<@UAGENT2>` after the
  agent that took it last. So the keys recipe comes after the one that reads
  the name TEST-Far, and checks Slack's posts by bot id, not by name.
- Not covered here (no real Slack can be reached from tests): a real
  workspace's events, rate limits and Slack's own manifest import, update
  and icon upload. The install redirect to an https address from an http
  window is covered by `tests/slack.rs` (YARD_ALLOW_ORIGIN) and here through
  the public address; the browser is never sent to Slack.
