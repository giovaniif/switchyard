# Connectors

The Connectors page (`#/connectors`, the plug row in the sidebar's top nav)
shows one card per service (Linear, Sentry, ...) across every engine account
on the selected host, however many accounts and sources list it: claude.ai
connectors, plugins, config files, project `.mcp.json`, for Codex ChatGPT
apps, and the set Switchyard holds itself for OpenCode, Grok and Gemini, which
cannot sign a server in. Cards stand in three sections. Needs you: not working
on some account, with the server's error and Retry. Connected: working on at
least one account. Available: listed but never signed in, the catalog (Linear
first) that nobody lists yet, and "Your own server".

A card has the service's mark (or its initial), its state, one line on what it
is for, and one engine mark per account: solid where it is connected, with a
dot where it is listed but signed out or broken, dashed where it is not there.
A chip per account narrows the page to that account, and search matches names,
descriptions and sources.

Connect signs the service in inside its card. With more than one account that
could have it, it asks which first, then walks them one after another: added
to the account's own config where it is missing, then the CLI's own sign-in
(a claude.ai connector opens claude.ai and connects by itself; a server that
redirects to a page on the host asks for that page's address to be pasted
back). A wrong address is refused with the CLI's reason and Try again. A
ChatGPT app links to chatgpt.com. A card's name opens its sheet: each
account's state with Connect, Retry and Remove, and which agents and
automations may use it. After a sign-in the account's running Claude sessions
dial the server again.

A session that needs a service it cannot reach asks for it with Switchyard's
`request_connector` tool: a connect card in the conversation, signed in there
or skipped, after which the agent goes on by itself.

A tool with a widget (`_meta.ui.resourceUri`) shows it in the thread under its
call, in a frame with `sandbox="allow-scripts"`, and keeps it in view when the
turn folds, between "Worked for" and the answer.

The fakes: Claude's `connectors` mode lists `claude.ai Datadog` (signs in 1 s
after Connect), `plugin:engineering:sentry` (pasted address with `code=`),
`chart` and a failed `plugin:engineering:gmail`; Codex lists `chart`,
`fake-oauth` (a real loopback redirect page) and the ChatGPT app GitHub. A
`[chart]` prompt makes either engine call `chart`'s `show_chart`
(verify/fakes/mcp-chart). `verify/fakes/mcp-oauth` is a remote MCP server that
signs in with OAuth, for the connectors Switchyard holds.

## Setup

```sh
D=$(mktemp -d /tmp/yv-fake-oauth-XXXXXX)
bun verify/fakes/mcp-oauth --port-file "$D/port" > "$D/log" 2>&1 < /dev/null &
echo $! > "$D/pid"
for i in $(seq 100); do [ -s "$D/port" ] && break; sleep 0.1; done
yv iso up
yv iso seed
yv var set oauth_dir "$D"
yv var set oauth "http://127.0.0.1:$(cat "$D/port")"
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"[chart] TEST-p95 latency for checkout"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"low","cwd":"{{plain}}","text":"[chart] TEST-claude chart"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
```

## Cleanup

```sh
D=$(v oauth_dir)
yv iso down
kill "$(cat "$D/pid")" 2>/dev/null || true
rm -rf "$D"
```

## Recipes

### connectors-page
Viewports: both
The plug row opens the page. Needs you holds Gmail, Not working with its
error and Retry. Connected holds chart with the widgets tag, on 2 of the 3
accounts. Available offers Datadog and Sentry (listed on Claude, signed out),
fake-oauth, the catalog (Linear first of those nobody lists) and "Your own
server"; GitHub, a ChatGPT app, links to ChatGPT.

```sh
yv drive connectors --steps '[{"expect": "role:heading:Connectors"}, {"expect": "css:.cx-card", "count": 2}, {"expect": {"css": "section[aria-label=\"Needs you\"] .cx-card", "hasText": "Gmail"}, "text": "No URL configured for this server"}, {"expect": {"css": ".cx-card", "hasText": "Gmail"}, "text": "Not working"}, {"expect": {"css": "section[aria-label=\"Connected\"] .cx-card", "hasText": "chart"}, "text": "widgets"}, {"expect": {"css": ".cx-card", "hasText": "chart"}, "text": "2 of 3 accounts"}, {"expect": "role:button:Connect Datadog"}, {"expect": "role:button:Connect Sentry"}, {"expect": "role:button:Connect Linear"}, {"expect": "role:button:Connect fake-oauth"}, {"expect": "role:link:Connect GitHub on ChatGPT"}, {"expect": {"css": ".cx-avail", "hasText": "Your own server"}}, {"eval": "document.documentElement.scrollWidth <= document.documentElement.clientWidth", "equals": true}, {"shot": "page"}]'
```

### one-account-at-a-time
Viewports: both
A chip per account narrows the page to that account. Codex has no Gmail and
no Datadog; the set Switchyard holds for OpenCode, Grok and Gemini starts
empty, with the catalog to connect. On a phone the chips stay on one row.

```sh
yv drive connectors --steps '[{"click": {"role": "button", "name": "Codex", "exact": true}}, {"expect": {"css": ".cx-card", "hasText": "Gmail"}, "count": 0}, {"expect": {"css": ".cx-card", "hasText": "chart"}, "text": "Connected"}, {"expect": "role:button:Connect Datadog"}, {"expect": "role:button:Connect fake-oauth"}, {"click": {"role": "button", "name": "Claude", "exact": true}}, {"expect": {"css": ".cx-card", "hasText": "Gmail"}, "text": "Not working"}, {"expect": "role:button:Connect fake-oauth", "count": 0}, {"click": {"role": "button", "name": "OpenCode · Grok · Gemini", "exact": true}}, {"expect": "css:.cx-card", "count": 0}, {"expect": "role:button:Connect Linear"}, {"eval": "(() => { const tops = [...document.querySelectorAll(\".cx-chip\")].map((c) => Math.round(c.getBoundingClientRect().top)); return tops.length === 4 && tops.every((t) => t === tops[0]); })()", "equals": true}, {"shot": "held-empty"}, {"click": {"role": "button", "name": "All accounts", "exact": true}}, {"expect": "css:.cx-card", "count": 2}]'
```

### search
Viewports: both
Search matches a service's name, what it is for and where it comes from, and
says so when nothing matches.

```sh
yv drive connectors --steps '[{"fill": "label:Search connectors", "value": "datadog"}, {"expect": "css:.cx-avail", "count": 1}, {"expect": "css:.cx-card", "count": 0}, {"fill": "label:Search connectors", "value": "engineering plugin"}, {"expect": {"css": ".cx-card", "hasText": "Gmail"}}, {"expect": {"css": ".cx-avail", "hasText": "Sentry"}}, {"expect": "css:.cx-avail", "count": 1}, {"fill": "label:Search connectors", "value": "no-such-service"}, {"expect": "text:No connectors match your search.", "visible": true}, {"shot": "empty-search"}, {"click": "role:button:Clear search"}, {"expect": "css:.cx-card", "count": 2}]'
```

### retry-a-failed-server
Retry on a Not working card asks the account to dial the server again and
lists the servers afresh. The fake's gmail has no URL, so it stays Not working
with the same error, and the click raises no error. The daemon's side is read
back: `mcp.reconnect` answers with a new `mcp` listing for the account.

```sh
yv drive connectors --steps '[{"expect": {"css": ".cx-card", "hasText": "Gmail"}, "text": "Not working"}, {"click": {"css": ".cx-card:has-text(\"Gmail\") button", "hasText": "Retry"}}, {"wait": 1500}, {"expect": {"css": ".cx-card", "hasText": "Gmail"}, "text": "No URL configured for this server"}, {"expect": {"css": ".cx-card:has-text(\"Gmail\") button", "hasText": "Retry"}}, {"shot": "retry-failed"}]'
yv api '{"op":"mcp.reconnect","engine":"claude","server":"plugin:engineering:gmail"}' --wait '{"op":"mcp","engine":"claude","servers":[{"name":"plugin:engineering:gmail","status":"failed"}]}'
```

### a-card-opens-its-sheet
Viewports: both
A card's name opens its sheet: a side sheet on desktop, from the bottom on a
phone, inside the viewport. It lists each account with how the service stands
there, and Escape closes it. An Available card opens one too, with Connect per
account.

```sh
yv drive connectors --steps '[{"click": {"css": ".cx-open", "hasText": "chart"}}, {"expect": "role:dialog:chart", "inViewport": true}, {"expect": {"css": ".cx-row", "hasText": "Claude"}, "text": "Connected"}, {"expect": {"css": ".cx-row", "hasText": "Codex"}, "text": "Connected"}, {"expect": {"css": ".cx-row", "hasText": "OpenCode · Grok · Gemini"}, "text": "held by Switchyard"}, {"shot": "sheet"}, {"press": "Escape"}, {"expect": "role:dialog:chart", "count": 0}, {"click": {"css": ".cx-open", "hasText": "Datadog"}}, {"expect": "role:dialog:Datadog", "inViewport": true}, {"expect": "role:button:Connect Datadog for Claude"}, {"expect": "role:button:Connect Datadog for Codex"}, {"click": "role:button:Close Datadog"}, {"expect": "role:dialog:Datadog", "count": 0}]'
```

### claude-sign-in
Viewports: desktop
Connect on Datadog asks which accounts first (it could be added to Codex and
to Switchyard's set too); with Claude alone it connects by itself and the card
says so until dismissed. Sentry asks for the address: Finish is muted until
one is typed (`.btn` once had no disabled style, #175), one without `code=`
is refused with Claude Code's reason, then the right one connects it. Both
cards then stand under Connected.

```sh
yv drive connectors --viewport desktop --allow-errors --steps '[{"click": "role:button:Connect Datadog"}, {"expect": "css:.cx-pick", "text": "Connect for"}, {"shot": "pick-accounts"}, {"click": {"css": ".cx-pick label", "hasText": "Codex"}}, {"click": {"css": ".cx-pick label", "hasText": "OpenCode · Grok · Gemini"}}, {"click": "role:button:Continue"}, {"expect": {"css": ".cx-card", "hasText": "Datadog"}, "text": "Running sessions have it now"}, {"shot": "datadog-connected"}, {"click": {"css": ".cx-card:has-text(\"Datadog\") button", "hasText": "Dismiss"}}, {"expect": {"css": "section[aria-label=\"Connected\"] .cx-card", "hasText": "Datadog"}, "text": "Connected"}, {"click": "role:button:Connect Sentry"}, {"click": {"css": ".cx-pick label", "hasText": "Codex"}}, {"click": {"css": ".cx-pick label", "hasText": "OpenCode · Grok · Gemini"}}, {"click": "role:button:Continue"}, {"expect": "role:link:Open the sign-in page"}, {"expect": {"css": ".cx-card", "hasText": "Sentry"}, "text": "paste that page"}, {"expect": "role:button:Finish", "enabled": false}, {"eval": "(() => { const f = [...document.querySelectorAll(\"button.btn\")].find((b) => b.textContent.trim() === \"Finish\"); const live = [...document.querySelectorAll(\"button.btn\")].find((b) => b !== f && !b.disabled && b.getClientRects().length); return String(getComputedStyle(f).color !== getComputedStyle(live).color); })()", "equals": "true"}, {"shot": "sentry-paste"}, {"fill": "label:Address of the page that did not load", "value": "http://localhost:39999/callback"}, {"click": "role:button:Finish"}, {"expect": {"css": ".cx-card", "hasText": "Sentry"}, "text": "missing authorization code"}, {"click": {"css": ".cx-card:has-text(\"Sentry\") button", "hasText": "Try again"}}, {"fill": "label:Address of the page that did not load", "value": "http://localhost:39999/callback?code=TEST"}, {"click": "role:button:Finish"}, {"expect": {"css": ".cx-card", "hasText": "Sentry"}, "text": "Running sessions have it now"}, {"click": {"css": ".cx-card:has-text(\"Sentry\") button", "hasText": "Dismiss"}}, {"expect": "css:section[aria-label=\"Connected\"] .cx-card", "count": 3}, {"shot": "claude-signed-in"}]'
```

### codex-sign-in-pasted
Viewports: desktop
fake-oauth is on Codex alone, so Connect goes straight to its sign-in. Its
page redirects to a loopback page on the host; pasting that address finishes
it through yardd.

```sh
yv drive connectors --viewport desktop --steps '[{"click": "role:button:Connect fake-oauth"}, {"expect": {"css": ".cx-card", "hasText": "fake-oauth"}, "text": "paste that page"}, {"eval": "(() => { const a = [...document.querySelectorAll(\"a\")].find((x) => x.textContent.includes(\"Open the sign-in page\")); const cb = new URL(a.href).searchParams.get(\"redirect_uri\") + \"?code=TEST-code&state=s1\"; const i = document.querySelector(\"input[aria-label=\\\"Address of the page that did not load\\\"]\"); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, \"value\").set.call(i, cb); i.dispatchEvent(new Event(\"input\", { bubbles: true })); return cb.startsWith(\"http://127.0.0.1:\"); })()", "equals": true}, {"click": "role:button:Finish"}, {"expect": {"css": ".cx-card", "hasText": "fake-oauth"}, "text": "Running sessions have it now"}, {"click": {"css": ".cx-card:has-text(\"fake-oauth\") button", "hasText": "Dismiss"}}, {"expect": {"css": ".cx-card", "hasText": "fake-oauth"}, "text": "Connected"}, {"shot": "codex-signed-in"}]'
```

### connect-linear-on-two-accounts
Viewports: desktop
Linear is in nobody's config. Connect, with Claude and Codex ticked, adds it
to each and walks their sign-ins one after the other in the same card: Claude
(a pasted address), then Codex (its loopback page). The card then reads 2 of
3 accounts. In its sheet, Remove on Claude's row asks first (Confirm), then
takes it out of Claude's config; Codex keeps it.

```sh
yv drive connectors --viewport desktop --steps '[{"click": "role:button:Connect Linear"}, {"click": {"css": ".cx-pick label", "hasText": "OpenCode · Grok · Gemini"}}, {"click": "role:button:Continue"}, {"expect": {"css": ".cx-card", "hasText": "Linear"}, "text": "Claude · 1 of 2"}, {"expect": {"css": ".cx-card", "hasText": "Linear"}, "text": "paste that page"}, {"fill": "label:Address of the page that did not load", "value": "http://localhost:39998/callback?code=TEST"}, {"click": "role:button:Finish"}, {"expect": {"css": ".cx-card", "hasText": "Linear"}, "text": "Codex · 2 of 2"}, {"expect": "role:link:Open the sign-in page"}, {"eval": "(() => { const a = [...document.querySelectorAll(\"a\")].find((x) => x.textContent.includes(\"Open the sign-in page\")); const cb = new URL(a.href).searchParams.get(\"redirect_uri\") + \"?code=TEST-code&state=s1\"; const i = document.querySelector(\"input[aria-label=\\\"Address of the page that did not load\\\"]\"); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, \"value\").set.call(i, cb); i.dispatchEvent(new Event(\"input\", { bubbles: true })); return cb.startsWith(\"http://127.0.0.1:\"); })()", "equals": true}, {"click": "role:button:Finish"}, {"expect": {"css": ".cx-card", "hasText": "Linear"}, "text": "Running sessions have it now"}, {"click": {"css": ".cx-card:has-text(\"Linear\") button", "hasText": "Dismiss"}}, {"expect": {"css": ".cx-card", "hasText": "Linear"}, "text": "2 of 3 accounts"}, {"shot": "linear-two-accounts"}, {"click": {"css": ".cx-open", "hasText": "Linear"}}, {"expect": {"css": ".cx-row", "hasText": "Claude"}, "text": "Your config"}, {"click": "role:button:Remove Linear from Claude"}, {"click": "role:button:Confirm removing Linear from Claude"}, {"expect": {"css": ".cx-row", "hasText": "Claude"}, "text": "not added"}, {"expect": {"css": ".cx-row", "hasText": "Codex"}, "text": "Connected"}, {"shot": "linear-removed-from-claude"}]'
```

### double-click-on-remove-only-asks
Viewports: both
A double click on Remove in a connector's sheet leaves "Confirm" up and keeps
the connector. Remove turns into Confirm in the same button, so the second
click of a double click lands on it; it used to answer it and the connector
went at once (like #519 for a session's Delete). The eval sends what a browser
sends for a double click: two mousedown, mouseup, click runs at the same
point, the second with `detail: 2`. Runs after connect-linear-on-two-accounts,
which leaves Linear on Codex.

```sh
yv drive connectors --steps '[{"click": {"css": ".cx-open", "hasText": "Linear"}}, {"expect": "role:button:Remove Linear from Codex"}, {"eval": "(async () => { const b = [...document.querySelectorAll(\"button\")].find((x) => x.getAttribute(\"aria-label\") === \"Remove Linear from Codex\"); b.scrollIntoView({ block: \"center\" }); const r = b.getBoundingClientRect(); const x = r.left + r.width / 2, y = r.top + r.height / 2; const click = (el, detail) => { for (const type of [\"mousedown\", \"mouseup\", \"click\"]) el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window, button: 0, clientX: x, clientY: y, detail })); }; click(b, 1); await new Promise((res) => setTimeout(res, 0)); const second = document.elementFromPoint(x, y).closest(\"button\"); const label = second ? second.getAttribute(\"aria-label\") : \"none\"; second && click(second, 2); await new Promise((res) => setTimeout(res, 300)); return label + \"|\" + [...document.querySelectorAll(\".cx-row\")].find((row) => row.textContent.includes(\"Codex\")).textContent.includes(\"Connected\"); })()", "equals": "Confirm removing Linear from Codex|true"}, {"expect": "role:button:Confirm removing Linear from Codex"}, {"shot": "confirm-still-up"}]'
```

### your-own-server
Viewports: both
“Your own server” takes a name and an https address for one account. A plain
`http://` address is refused before anything is sent, with the button off;
Switchyard's own set also takes a server on the host itself. Cancel sends
nothing.

```sh
yv drive connectors --steps '[{"click": {"css": ".cx-avail-own button"}}, {"expect": "role:dialog:Add your own server", "inViewport": true}, {"expect": "role:button:Add and sign in", "enabled": false}, {"fill": "label:Connector name", "value": "metabase"}, {"fill": "label:Connector address", "value": "http://metabase.local/mcp"}, {"expect": "role:dialog:Add your own server", "text": "https://"}, {"expect": "role:button:Add and sign in", "enabled": false}, {"fill": "label:Connector address", "value": "http://127.0.0.1:9/mcp"}, {"expect": "role:button:Add and sign in", "enabled": false}, {"click": "role:button:Account for the new connector"}, {"click": "role:menuitemradio:OpenCode · Grok · Gemini"}, {"expect": "role:button:Add and sign in", "enabled": true}, {"shot": "own-server"}, {"click": "role:button:Cancel"}, {"expect": "role:dialog:Add your own server", "count": 0}, {"expect": {"css": ".cx-card", "hasText": "metabase"}, "count": 0}]'
```

### held-by-switchyard
Viewports: desktop
OpenCode, Grok and Gemini cannot sign a server in, so Switchyard holds
connectors for them. A server added to that set (the fake OAuth server from
Setup) stands under Available, signed out. Its sign-in is yardd's own: the
authorize page redirects to yardd's loopback page, which the isolated browser
may not open, so the recipe follows it with curl, as a browser on the host
would. The card then reads Connected with the server's 2 tools, held by
Switchyard, and the server saw one registration and one authorization.

```sh
yv api '{"op":"mcp.add","engine":"switchyard","name":"TEST-issues","url":"'"$(v oauth)"'/mcp"}' --wait '{"op":"mcp.change","engine":"switchyard","name":"TEST-issues","state":"done"}'
yv drive connectors --viewport desktop --steps '[{"expect": "role:button:Connect TEST-issues"}, {"click": {"css": ".cx-open", "hasText": "TEST-issues"}}, {"expect": {"css": ".cx-row", "hasText": "OpenCode · Grok · Gemini"}, "text": "held by Switchyard"}, {"shot": "held-signed-out"}]'
yv api '{"op":"mcp.auth","engine":"switchyard","server":"TEST-issues"}' --wait '{"op":"mcp.auth","engine":"switchyard","state":"open"}' --save held_auth=url
curl -sf -L "$(v held_auth)" | grep -q "TEST-issues is connected"
yv drive connectors --viewport desktop --steps '[{"expect": {"css": "section[aria-label=\"Connected\"] .cx-card", "hasText": "TEST-issues"}, "text": "Connected"}, {"click": {"css": ".cx-open", "hasText": "TEST-issues"}}, {"expect": "role:dialog:TEST-issues", "text": "2 tools"}, {"expect": {"css": ".cx-row", "hasText": "OpenCode · Grok · Gemini"}, "text": "Connected"}, {"expect": "role:button:Remove TEST-issues from OpenCode · Grok · Gemini"}, {"shot": "held-connected"}]'
test "$(curl -sf "$(v oauth)/state" | jq -c '[.registrations,.authorizations]')" = "[1,1]"
```

### connect-card-in-a-conversation
Viewports: desktop
A session that needs a service asks with Switchyard's `request_connector`
tool (the scripted engines call it from `[[tool …]]`). Its turn ends, the row
reads Needs connector, and the card stands under its answer with the reason.
Notion is in nobody's config: Connect adds it to the session's account and
signs it in there, in the card. The card folds to one line, the mark goes, and
the agent's next turn starts by itself with yardd's word that it is connected
(the fake echoes it).

```sh
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"low","cwd":"{{plain}}","text":"TEST-read the launch plan [[tool request_connector {\"service\":\"Notion\",\"reason\":\"To read the TEST-launch plan.\"}]]"}' --wait '{"op":"event","event":{"type":"turn_completed"}}' --save sid_connect=sessionId
yv state --expect '{"sessions":[{"id":"{{sid_connect}}","status":"idle","needsConnector":{"service":"Notion"}}]}'
yv drive 'session:{{sid_connect}}' --viewport desktop --steps '[{"expect": "css:.connect", "text": "Connect Notion"}, {"expect": "css:.connect", "text": "To read the TEST-launch plan."}, {"expect": "css:.connect", "text": "Claude on"}, {"expect": {"css": ".session-row.active .row-status"}, "text": "Needs connector"}, {"shot": "asked"}, {"click": "role:button:Connect Notion"}, {"expect": "role:link:Open the sign-in page"}, {"shot": "signing-in"}, {"fill": "label:Address of the page that did not load", "value": "http://localhost:39997/callback?code=TEST"}, {"click": "role:button:Finish"}, {"expect": "css:.connect-line", "text": "Notion connected"}, {"expect": "css:.thread", "text": "Echo: Notion is connected now."}, {"expect": "css:.connect", "count": 0}, {"shot": "connected"}]'
yv state --expect '{"sessions":[{"id":"{{sid_connect}}","status":"idle","needsConnector":null}]}'
```

### connect-card-skipped
Viewports: both
Not now answers the card without connecting: it folds to a line that says so,
and the agent is told and goes on. A Codex session asks the same way.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"TEST-check the funnel [[tool request_connector {\"service\":\"PostHog\",\"reason\":\"To read the TEST-signup funnel.\"}]]"}' --wait '{"op":"event","event":{"type":"turn_completed"}}' --save sid_skip=sessionId
yv drive 'session:{{sid_skip}}' --viewport mobile --steps '[{"expect": "css:.connect", "text": "Connect PostHog"}, {"expect": "css:.connect", "inViewport": true}, {"shot": "asked"}, {"click": "role:button:Not now"}, {"expect": "css:.connect-line", "text": "PostHog not connected. The agent was told."}, {"expect": "css:.thread", "text": "Echo: The user chose not to connect PostHog for now."}, {"shot": "skipped"}]'
yv drive 'session:{{sid_skip}}' --viewport desktop --steps '[{"expect": "css:.connect-line", "text": "PostHog not connected. The agent was told."}, {"expect": "css:.connect", "count": 0}]'
```

### an-agent-asks-and-the-owner-allows
Viewports: desktop
An agent's connectors are the owner's to set, and a new agent starts with
none. chart is connected on its account but off for it, so its request reads
"Let TEST-Triage use chart?": Allow adds chart to the agent's list and tells
it to go on. The connector's sheet shows the same list from the other side,
under Who may use it: the switch is now on, and turning it off takes chart
away from the agent again.

```sh
yv api '{"op":"agent.save","agent":{"name":"TEST-Triage","engine":"claude","model":"claude-haiku-5-5","effort":"low","color":"blue","about":"TEST-triages alerts"},"ref":"cx-agent"}' --wait '{"op":"agent.saved"}' --save triage=agent.id
yv api '{"op":"agent.ask","agentId":"{{triage}}","text":"TEST-draw it [[tool request_connector {\"service\":\"chart\",\"reason\":\"To draw the TEST-latency chart.\"}]]"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv drive agents --viewport desktop --steps '[{"click": {"css": ".sidebar button", "hasText": "TEST-Triage"}}, {"expect": "css:.connect", "text": "Let TEST-Triage use chart?"}, {"expect": "css:.connect", "text": "off for this agent"}, {"shot": "agent-asks"}, {"click": "role:button:Allow"}, {"expect": "css:.connect-line", "text": "chart connected"}, {"expect": "css:.thread", "text": "Echo: chart is connected now."}, {"shot": "agent-allowed"}]'
yv drive connectors --viewport desktop --steps '[{"click": {"css": ".cx-open", "hasText": "chart"}}, {"expect": "role:dialog:chart", "text": "Who may use it"}, {"eval": "document.querySelector(\"[aria-label=\\\"TEST-Triage may use chart\\\"]\").getAttribute(\"aria-checked\")", "equals": "true"}, {"shot": "who-may-use-it"}, {"click": {"role": "switch", "name": "TEST-Triage may use chart"}}, {"expect": "css:[aria-label=\"TEST-Triage may use chart\"][aria-checked=\"false\"]"}]'
```

### codex-chart
The `[chart]` session shows the chart under its call in a sandboxed frame,
outside the folded work, above "Here is the chart.".

```sh
yv drive 'session:TEST-p95 latency' --allow-errors --steps '[{"expect":"css:.thread","text":"Here is the chart."},{"expect":{"css":".fold-row"}},{"expect":"css:.widget iframe.widget-frame[sandbox=\"allow-scripts\"]"},{"expect":{"css":".widget iframe"},"inViewport":true},{"wait":1000},{"shot":"chart"}]'
```

### claude-chart
The same for a Claude session.

```sh
yv drive 'session:TEST-claude chart' --allow-errors --steps '[{"expect":"css:.thread","text":"Here is the chart."},{"expect":{"css":".fold-row"}},{"expect":"css:.widget iframe.widget-frame[sandbox=\"allow-scripts\"]"},{"expect":{"css":".widget iframe"},"inViewport":true},{"wait":1000},{"shot":"chart"}]'
```

### widget-calls-and-sandbox (manual: `yv drive` cannot reach inside a frame)
Inside each chart, "More
points" goes through `widget.call` to the server and redraws with 12 points;
"Open docs" opens a new tab; the widget's `fetch` is blocked by its CSP and it
cannot read `window.parent.document` (SecurityError). Checked with a Playwright
script against `iso` on 2026-11-16; yardd's side is covered by
`crates/yardd/tests/connectors.rs`.

### many-services-stay-findable
Viewports: both
An account with many servers shows a card for each, with no pages to step
through: search is how one is found. A very long name wraps inside its card
and the page does not scroll sideways.

```sh
yv iso up --name map-connectors-many
bun -e 'const fs=require("node:fs"); fs.writeFileSync(process.argv[1]+"/claude-mcp.txt", Array.from({length:21},(_,i)=>"TEST-service-"+String(i).padStart(2,"0")+(i===20?"-very-long-service-name".repeat(6):"")+" https://example.com/mcp").join("\n")+"\n")' "$(yv var get fake --iso map-connectors-many | jq -r .value)"
yv drive connectors --iso map-connectors-many --steps '[{"click": {"role": "button", "name": "Claude", "exact": true}}, {"expect": {"css": ".cx-card, .cx-avail", "hasText": "TEST-service"}, "count": 21}, {"expect": {"css": ".cx-card, .cx-avail", "hasText": "TEST-service-20"}, "inViewport": false}, {"fill": "label:Search connectors", "value": "TEST-service-20"}, {"expect": {"css": ".cx-card, .cx-avail", "hasText": "TEST-service"}, "count": 1}, {"eval": "document.documentElement.scrollWidth <= document.documentElement.clientWidth", "equals": true}, {"eval": "(() => { const c = [...document.querySelectorAll(\".cx-card, .cx-avail\")].find((x) => x.textContent.includes(\"TEST-service-20\")).getBoundingClientRect(); return c.left >= 0 && c.right <= innerWidth; })()", "equals": true}, {"shot": "long-service-name"}, {"fill": "label:Search connectors", "value": ""}, {"expect": {"css": ".cx-card, .cx-avail", "hasText": "TEST-service"}, "count": 21}, {"shot": "many-services"}]'
yv iso down --name map-connectors-many
```

## Gotchas
- The Messaging section's Slack card lists its steps and channels as `li.connector`; the service cards are `.cx-card` (Needs you, Connected) and `.cx-avail` (Available).
- A card that is signing in stays in the section where Connect was clicked until its "Connected" line is dismissed, so a recipe dismisses it before counting sections.
- Connect on a catalog service ticks every account that could have it, Switchyard's set included; the sign-in recipes untick it, since adding the real address there would make yardd call the real service.
- claude-sign-in passes `--allow-errors`: the refused address is an `mcp.auth`
  failure on purpose.
- The chart recipes pass `--allow-errors`: the fake widget tries
  `fetch("https://example.com/")` on purpose, and the console error that the
  CSP blocked it is the proof the sandbox holds.
- Sign-ins persist in the fake's state for the life of the isolated yardd, so
  the sign-in recipes run once per `iso up`, in the order written.
- The isolated browser reaches only its own yardd, so held-by-switchyard follows the fake's authorize redirect with curl.
- Not run by the map: a real provider's OAuth, and a real OpenCode, Grok or Gemini turn calling a held connector's tool (`crates/yardd/tests/held.rs` covers the bridge on the scripted agents).
