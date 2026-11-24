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
yv iso up --env "YARD_SLACK_API=http://127.0.0.1:$(cat "$D/port")/api/"
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
yv drive connectors --steps '[{"expect":"role:heading:Messaging"},{"expect":{"css":".slack-card"},"text":"Not set up"},{"expect":{"role":"link","name":"Create on Slack"}},{"expect":"role:button:Copy manifest"},{"click":"role:button:Show manifest"},{"expect":"css:.slack-manifest","text":"\"socket_mode_enabled\": true"},{"fill":"label:App-level token","value":"xoxb-TEST-wrong-kind"},{"expect":{"css":".slack-token-form"},"text":"That is the bot token"},{"expect":{"css":".slack-token-form button","hasText":"Connect"},"enabled":false},{"eval":"(() => { const steps = document.querySelector(\".slack-steps\").getBoundingClientRect(); const field = document.querySelector(\".slack-field\").getBoundingClientRect(); return Math.abs(steps.left - field.left) < 1 && document.documentElement.scrollWidth <= innerWidth; })()","equals":true},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), true)","equals":true},{"shot":"setup"}]'
```

### refused-token-then-connected
Viewports: both
A refused bot token shows Not working with Slack's error; replacing only
that token connects, and the header names the bot and workspace.

```sh
yv api '{"op":"slack.disconnect"}' --wait '{"op":"slack","slack":{"state":"off"}}'
yv drive connectors --viewport desktop --steps '[{"fill":"label:App-level token","value":"xapp-1-A0TEST-TESTsecretapptoken"},{"fill":"label:Bot token","value":"xoxb-bad-TESTtoken"},{"click":{"css":".slack-token-form button","hasText":"Connect"}},{"expect":{"css":".slack-card"},"text":"Not working"},{"expect":{"css":".slack-card"},"text":"The bot token was refused (auth.test: invalid_auth)."},{"expect":{"css":".slack-card button","hasText":"Retry"}},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), true)","equals":true},{"shot":"refused"}]'
yv drive connectors --viewport mobile --steps '[{"expect":{"css":".slack-card"},"text":"The bot token was refused (auth.test: invalid_auth)."},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), document.documentElement.scrollWidth <= innerWidth)","equals":true},{"shot":"refused"}]'
yv drive connectors --viewport desktop --steps '[{"click":{"css":".slack-card button","hasText":"Replace tokens"}},{"fill":"label:Bot token","value":"xoxb-TEST-TESTsecretbottoken"},{"click":{"css":".slack-token-form button","hasText":"Save"}},{"expect":{"css":".slack-card"},"text":"As @switchyard in TEST-workspace"},{"expect":{"css":".slack-card .connector-account-state"},"text":"Connected"},{"shot":"connected"}]'
yv drive connectors --viewport mobile --steps '[{"expect":{"css":".slack-card"},"text":"Connected"},{"eval":"(document.querySelector(\".connector-apps\").scrollIntoView({block:\"start\"}), document.documentElement.scrollWidth <= innerWidth)","equals":true},{"shot":"connected"}]'
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
yv drive agents --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"expect":{"css":".origin-badge.origin-slack","hasText":"From TEST-Owner on Slack · #general"}},{"eval":"getComputedStyle(document.querySelector(\".origin-slack .origin-dot\")).backgroundColor","equals":"rgb(74, 21, 75)"},{"expect":{"css":".user-message .bubble","hasText":"TEST-check the release notes"}},{"expect":{"css":".thread"},"text":"Echo:"},{"eval":"(() => { const b = document.querySelector(\".origin-badge\").getBoundingClientRect(); const m = document.querySelector(\".origin-badge + .bubble\").getBoundingClientRect(); return b.bottom <= m.top && Math.abs(b.right - m.right) <= 6 && document.documentElement.scrollWidth <= innerWidth; })()","equals":true},{"shot":"thread"}]'
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

## Gotchas

- Not covered here (no real Slack can be reached from tests): a real
  workspace's events, rate limits and Slack's own manifest import.
