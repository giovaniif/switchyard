# Connectors

The Connectors page (`#/connectors`, the plug row in the sidebar's top nav)
shows one card per engine account on the selected host. All connector states are visible by default. Filter tools exposes Connected, Available, Not working and All.
Search matches service names and sources, with eight rows per page. Sources include claude.ai
connectors, plugins, config files, project `.mcp.json` and, for Codex, ChatGPT
apps. Each row has the service's name, where it comes from, its state
(Connected, Needs sign-in, Not working with the server's error, Connecting,
Off), a tool count, and a "widgets" tag when some of its tools come with an MCP
Apps widget. Within All, Needs sign-in comes first, then Not working. Each card sums it up
("1 connected · 2 need sign-in · 1 not working").

Connect starts the CLI's own sign-in: a claude.ai connector opens claude.ai
and the row turns Connected by itself; a server that redirects to a page on the
host asks for that page's address to be pasted back (Claude Code takes it
directly; for Codex yardd delivers it to the loopback page on the host). A wrong
address is refused with the CLI's reason and Try again. A ChatGPT app links to
chatgpt.com. "Add connector" adds a catalog server (Linear first) or any https
address to the account's own config and opens its sign-in; Remove takes one
out. After a sign-in the account's running Claude sessions dial the
server again.

A tool with a widget (`_meta.ui.resourceUri`) shows it in the thread under its
call, in a frame with `sandbox="allow-scripts"`, and keeps it in view when the
turn folds, between "Worked for" and the answer.

The fakes: Claude's `connectors` mode lists `claude.ai Datadog` (signs in 1 s
after Connect), `plugin:engineering:sentry` (pasted address with `code=`),
`chart` and a failed `plugin:engineering:gmail`; Codex lists `chart`,
`fake-oauth` (a real loopback redirect page) and the ChatGPT app GitHub. A
`[chart]` prompt makes either engine call `chart`'s `show_chart`
(verify/fakes/mcp-chart).

## Setup

```sh
yv iso up
yv iso seed
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"[chart] TEST-p95 latency for checkout"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv api '{"op":"session.create","engine":"claude","model":"claude-haiku-5-5","effort":"low","cwd":"{{plain}}","text":"[chart] TEST-claude chart"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
```

## Cleanup

```sh
yv iso down
```

## Recipes

### connectors-list
The plug row opens the page. Claude's card: Datadog via
claude.ai needs sign-in, gmail is not working with its error, chart is
connected with the widgets tag. Codex's card: fake-oauth needs sign-in and
GitHub links to ChatGPT.

```sh
yv drive connectors --steps '[{"click":{"role":"button","name":"Claude","exact":true}},{"click":{"role":"button","name":"Filter tools","exact":true}},{"click":{"role":"button","name":"Connected","exact":true}},{"expect":"role:heading:Connectors"},{"expect":{"css":".connectors-card","hasText":"Claude"},"text":"1 connected · 2 need sign-in · 1 not working"},{"expect":"css:.connectors-card","count":1},{"expect":"css:.connector","count":1},{"expect":"css:.connector","text":"chart"},{"expect":"css:.connector","text":"widgets"},{"shot":"connected-only"},{"click":{"role":"button","name":"Available","exact":true}},{"expect":{"css":".connector","hasText":"Datadog"},"text":"claude.ai"},{"expect":{"css":".connector","hasText":"Datadog"},"text":"Needs sign-in"},{"click":{"role":"button","name":"Not working","exact":true}},{"expect":{"css":".connector","hasText":"gmail"},"text":"No URL configured for this server"},{"click":{"role":"button","name":"Codex","exact":true}},{"click":{"role":"button","name":"Filter tools","exact":true}},{"expect":{"css":".connectors-card","hasText":"Codex"},"text":"1 connected · 2 need sign-in"},{"click":{"role":"button","name":"Available","exact":true}},{"expect":"role:link:Connect on ChatGPT"},{"expect":{"css":".connector","hasText":"fake-oauth"},"text":"Needs sign-in"},{"shot":"codex-available"}]'
```

### focused-search-and-status
An engine filter narrows the list to that engine’s accounts. Filter tools reveals the status filters. Available shows unused services. Search matches service names and a status filter
shows only usable connectors, with a clear empty state.

```sh
yv drive connectors --steps '[{"click":{"role":"button","name":"Claude","exact":true}},{"click":{"role":"button","name":"Filter tools","exact":true}},{"click":{"role":"button","name":"Connected","exact":true}},{"expect":"css:.connectors-card","count":1},{"expect":"css:.connector","count":1},{"expect":"css:.connector","text":"chart"},{"expect":{"css":".connector","hasText":"Datadog"},"count":0},{"click":{"role":"button","name":"Available","exact":true}},{"fill":"placeholder:Search connectors","value":"datadog"},{"expect":"css:.connector","count":1},{"expect":"css:.connector","text":"Datadog"},{"fill":"placeholder:Search connectors","value":"no-such-service"},{"expect":"text:No connectors match","visible":true},{"shot":"empty-search"},{"fill":"placeholder:Search connectors","value":""},{"click":{"role":"button","name":"Connected","exact":true}},{"expect":"css:.connector","count":1},{"expect":"css:.connector","text":"chart"},{"shot":"connected-filter"}]'
```



### filters-stay-on-one-row
Viewports: both
The four filters (Connected, Available, Not working, All) sit on one row, each
fully visible. At 390 px "All 4" used to wrap onto a line of its own, where it
looked like a separate control (#209). Nothing between the tabs and the card
clips their 3 px focus ring (a scrolling row did, on the first try of #225).

```sh
yv drive connectors --steps '[{"click":{"role":"button","name":"Claude","exact":true}},{"click":{"role":"button","name":"Filter tools","exact":true}},{"expect":"role:heading:Connectors"},{"expect":{"role":"button","name":"All","exact":true}},{"eval":"(() => { const group = document.querySelector(\".connector-filters\"); const box = group.getBoundingClientRect(); const tabs = [...group.querySelectorAll(\"button\")].map((b) => b.getBoundingClientRect()); return tabs.length === 4 && tabs.every((t) => t.top === tabs[0].top && t.left >= box.left && t.right <= box.right + 0.5); })()","equals":true},{"eval":"[...document.querySelectorAll(\".connector-filters button\")].every((tab) => { const r = tab.getBoundingClientRect(); for (let el = tab.parentElement; el && !el.classList.contains(\"connectors-card\"); el = el.parentElement) { const cs = getComputedStyle(el); if (cs.overflowX === \"visible\" && cs.overflowY === \"visible\") continue; const b = el.getBoundingClientRect(); if (r.top - 3 < b.top || r.bottom + 3 > b.bottom || r.left - 3 < b.left || r.right + 3 > b.right) return false; } return true; })","equals":true},{"shot":"filters"}]'
```

### retry-a-failed-server
Retry on a Not working row asks the account to dial the server again and
lists the servers afresh. The fake's gmail has no URL, so it stays Not working
with the same error, and the click raises no error. The daemon's side is read
back: `mcp.reconnect` answers with a new `mcp` listing for the account.

```sh
yv drive connectors --steps '[{"click":{"role":"button","name":"Claude","exact":true}},{"expect":{"css":".connector","hasText":"gmail"},"text":"Not working"},{"click":{"css":".connector:has-text(\"gmail\") button","hasText":"Retry"}},{"wait":1500},{"expect":{"css":".connector","hasText":"gmail"},"text":"No URL configured for this server"},{"expect":{"css":".connector:has-text(\"gmail\") button","hasText":"Retry"}},{"shot":"retry-failed"}]'
yv api '{"op":"mcp.reconnect","engine":"claude","server":"plugin:engineering:gmail"}' --wait '{"op":"mcp","engine":"claude","servers":[{"name":"plugin:engineering:gmail","status":"failed"}]}'
```

### claude-sign-in
Viewports: desktop
Datadog connects by itself after Connect. sentry asks for the address: one
without `code=` is refused with Claude Code's reason, then the right one
connects it. The card then reads 3 connected.

```sh
yv drive connectors --viewport desktop --allow-errors --steps '[{"click":{"role":"button","name":"Claude","exact":true}},{"click":{"role":"button","name":"Filter tools","exact":true}},{"click":{"role":"button","name":"Available","exact":true}},{"click":{"css":".connector:has-text(\"Datadog\") button","hasText":"Connect"}},{"expect":{"css":".connector","hasText":"Datadog"},"text":"Running sessions pick it up now"},{"expect":{"css":".connector","hasText":"Datadog"},"text":"12 tools"},{"click":{"css":".connector:has-text(\"sentry\") button","hasText":"Connect"}},{"expect":"role:link:Open the sign-in page"},{"expect":{"css":".connector","hasText":"sentry"},"text":"paste that page"},{"fill":"css:#connector-paste","value":"http://localhost:39999/callback"},{"click":"role:button:Finish"},{"expect":{"css":".connector","hasText":"sentry"},"text":"missing authorization code"},{"click":{"css":".connector:has-text(\"sentry\") button","hasText":"Try again"}},{"fill":"css:#connector-paste","value":"http://localhost:39999/callback?code=TEST"},{"click":"role:button:Finish"},{"expect":{"css":".connector","hasText":"sentry"},"text":"Running sessions pick it up now"},{"expect":{"css":".connectors-card","hasText":"Claude"},"text":"3 connected · 1 not working"},{"shot":"claude-signed-in"}]'
```

### codex-sign-in-pasted
Viewports: desktop
fake-oauth's sign-in page redirects to a loopback page on the host; pasting
that address finishes it through yardd and the card reads 2 connected.

```sh
yv drive connectors --viewport desktop --steps '[{"click":{"role":"button","name":"Codex","exact":true}},{"click":{"role":"button","name":"Filter tools","exact":true}},{"click":{"role":"button","name":"Available","exact":true}},{"click":{"css":".connector:has-text(\"fake-oauth\") button","hasText":"Connect"}},{"expect":{"css":".connector","hasText":"fake-oauth"},"text":"paste that page"},{"eval":"(() => { const a = [...document.querySelectorAll(\"a\")].find((x) => x.textContent.includes(\"Open the sign-in page\")); const cb = new URL(a.href).searchParams.get(\"redirect_uri\") + \"?code=TEST-code&state=s1\"; const i = document.querySelector(\"#connector-paste\"); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, \"value\").set.call(i, cb); i.dispatchEvent(new Event(\"input\", { bubbles: true })); return cb.startsWith(\"http://127.0.0.1:\"); })()","equals":true},{"click":"role:button:Finish"},{"expect":{"css":".connector","hasText":"fake-oauth"},"text":"Running sessions pick it up now"},{"expect":{"css":".connectors-card","hasText":"Codex"},"text":"2 connected · 1 needs sign-in"},{"shot":"codex-signed-in"}]'
```

### add-linear
Viewports: desktop
"Add connector" offers Linear first. Picking it on Claude's card and confirming Connect adds it to the account's config and opens its sign-in (a pasted address finishes it);
the same on Codex's card, through the loopback page. Remove (then Confirm)
takes Claude's out again. "Other address…" refuses a plain `http://` address
before sending anything.

```sh
yv drive connectors --viewport desktop --steps '[{"click":{"css":".connectors-card:has-text(\"Claude\") button","hasText":"Add connector"}},{"expect":{"css":".connector-catalog button","hasText":"Linear"}},{"click":{"role":"button","name":"Linear","exact":true}},{"click":{"role":"button","name":"Connect","exact":true,"within":"role:dialog:Add connector"}},{"expect":{"css":".connectors-card:has-text(\"Claude\") .connector","hasText":"Linear"},"text":"paste that page"},{"fill":"css:#connector-paste","value":"http://localhost:39998/callback?code=TEST"},{"click":"role:button:Finish"},{"expect":{"css":".connectors-card:has-text(\"Claude\") .connector","hasText":"Linear"},"text":"Running sessions pick it up now"},{"shot":"linear-claude"},{"click":"role:button:Remove Linear"},{"click":"role:button:Confirm removing Linear"},{"expect":{"css":".connectors-card:has-text(\"Claude\") .connector","hasText":"Linear"},"count":0},{"click":{"css":".connectors-card:has-text(\"Claude\") button","hasText":"Add connector"}},{"click":"role:button:Other address"},{"fill":"label:Connector name","value":"metabase"},{"fill":"label:Connector address","value":"http://metabase.local/mcp"},{"expect":{"css":".connector-add-popover"},"text":"https://"},{"click":"role:button:Cancel"},{"click":{"role":"button","name":"Codex","exact":true}},{"click":{"css":".connectors-card:has-text(\"Codex\") button","hasText":"Add connector"}},{"click":{"role":"button","name":"Linear","exact":true}},{"click":{"role":"button","name":"Connect","exact":true,"within":"role:dialog:Add connector"}},{"expect":{"css":".connectors-card:has-text(\"Codex\") .connector","hasText":"Linear"},"text":"paste that page"},{"eval":"(() => { const a = [...document.querySelectorAll(\"a\")].find((x) => x.textContent.includes(\"Open the sign-in page\")); const cb = new URL(a.href).searchParams.get(\"redirect_uri\") + \"?code=TEST-code&state=s1\"; const i = document.querySelector(\"#connector-paste\"); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, \"value\").set.call(i, cb); i.dispatchEvent(new Event(\"input\", { bubbles: true })); return cb.startsWith(\"http://127.0.0.1:\"); })()","equals":true},{"click":"role:button:Finish"},{"expect":{"css":".connectors-card:has-text(\"Codex\") .connector","hasText":"Linear"},"text":"Running sessions pick it up now"},{"shot":"linear-both"}]'
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

### large-catalog-stays-bounded
Many unused entries stay hidden in Connected. All is limited to eight rows,
Next reaches the final row, and changing search resets pagination. A long
service name wraps within the phone viewport.

```sh
yv iso up --name map-connectors-many
bun -e 'const fs=require("node:fs"); fs.writeFileSync(process.argv[1]+"/claude-mcp.txt", Array.from({length:21},(_,i)=>"TEST-service-"+String(i).padStart(2,"0")+(i===20?"-very-long-service-name".repeat(6):"")+" https://example.com/mcp").join("\n")+"\n")' "$(yv var get fake --iso map-connectors-many | jq -r .value)"
yv drive connectors --iso map-connectors-many --steps '[{"click":{"role":"button","name":"Claude","exact":true}},{"click":{"role":"button","name":"Filter tools","exact":true}},{"click":{"role":"button","name":"Connected","exact":true}},{"expect":"css:.connector","count":1},{"expect":{"css":".connector","hasText":"TEST-service"},"count":0},{"click":{"role":"button","name":"All","exact":true}},{"expect":"css:.connector","count":8},{"expect":"role:navigation:Connector pages","text":"1–8 of 25"},{"click":"role:button:Next"},{"expect":"role:navigation:Connector pages","text":"9–16 of 25"},{"click":"role:button:Next"},{"click":"role:button:Next"},{"expect":"css:.connector","count":1},{"expect":"role:button:Next","enabled":false},{"fill":"placeholder:Search connectors","value":"TEST-service-20"},{"expect":"css:.connector","count":1},{"expect":"css:.connector","text":"TEST-service-20"},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"long-service-name"},{"fill":"placeholder:Search connectors","value":""},{"expect":"role:navigation:Connector pages","text":"1–8 of 25"},{"shot":"bounded-catalog"}]'
yv iso down --name map-connectors-many
```

### disabled-button-looks-disabled
Viewports: both
An outlined button that cannot be used reads muted. In Add connector's "Other
address…" form, Continue stays disabled until a name and an address are
typed. Until then it is not the ink colour of a live outlined button, and
once both are filled it matches them. (`.btn` had no disabled style, so it
looked live and the click did nothing: #175.)

```sh
yv drive connectors --steps '[{"click":{"css":".connectors-card:has-text(\"Claude\") button","hasText":"Add connector"}},{"click":"role:button:Other address…"},{"expect":"role:button:Continue","enabled":false},{"eval":"(() => { const all = [...document.querySelectorAll(\"button.btn\")]; const c = all.find((b) => b.textContent.trim() === \"Continue\"); const live = all.find((b) => b !== c && !b.disabled && b.getClientRects().length); return String(getComputedStyle(c).color !== getComputedStyle(live).color); })()","equals":"true"},{"shot":"continue-disabled"},{"fill":"label:Connector name","value":"TEST-svc"},{"fill":"label:Connector address","value":"https://example.com/mcp"},{"expect":"role:button:Continue","enabled":true},{"eval":"(() => { const all = [...document.querySelectorAll(\"button.btn\")]; const c = all.find((b) => b.textContent.trim() === \"Continue\"); const live = all.find((b) => b !== c && !b.disabled && b.getClientRects().length); return String(getComputedStyle(c).color === getComputedStyle(live).color); })()","equals":"true"}]'
```

### rows-meet-header-and-footer
Viewports: both
An account card's connector rows start right under its header and end right
on its footer, with one line between each: no empty band. The search and
filters, once Filter tools opens them, keep their room under the header.
Before, the card's old 10 px gap (from before #290 made it a header, rows
and footer) left a white band framed by two lines under the header and
blank space above the footer.

```sh
yv drive connectors --steps '[{"expect":"css:.connectors-card .connector-list"},{"eval":"[...document.querySelectorAll(\".connectors-card\")].map((card) => { const head = card.querySelector(\".connectors-card-head\").getBoundingClientRect(); const listEl = card.querySelector(\".connector-list\"); const list = listEl.getBoundingClientRect(); const foot = card.querySelector(\".connector-card-footer\").getBoundingClientRect(); const lines = [getComputedStyle(card.querySelector(\".connectors-card-head\")).borderBottomWidth, getComputedStyle(listEl).borderTopWidth].filter((w) => w !== \"0px\").length; return [Math.round(list.top - head.bottom), Math.round(foot.top - list.bottom), lines].join(); }).join(\";\")","equals":"0,0,1;0,0,1"},{"shot":"rows-meet"},{"click":{"css":".connectors-card:has-text(\"Claude\") button","hasText":"Filter tools"}},{"expect":"label:Search connectors"},{"eval":"(() => { const card = document.querySelector(\".connectors-card\"); const head = card.querySelector(\".connectors-card-head\").getBoundingClientRect(); const ctl = card.querySelector(\".connector-filter-controls\").getBoundingClientRect(); const list = card.querySelector(\".connector-list\").getBoundingClientRect(); return ctl.top - head.bottom >= 8 && list.top - ctl.bottom >= 8; })()","equals":true},{"shot":"filters-open"}]'
```

## Gotchas
- claude-sign-in passes `--allow-errors`: the refused address is an `mcp.auth`
  failure on purpose.
- The chart recipes pass `--allow-errors`: the fake widget tries
  `fetch("https://example.com/")` on purpose, and the console error that the
  CSP blocked it is the proof the sandbox holds.
- Sign-ins persist in the fake's state for the life of the isolated yardd, so
  the sign-in recipes run once per `iso up`.
