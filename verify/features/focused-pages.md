# Focused pages

The approved design has compact host rows and one selected host inspector,
separate Sessions / Engine logins / System tabs, dedicated session and
subscription pages, a sidebar usage popover, and a separate account picker.
The model menu never changes the account of an existing conversation.

## Setup

```sh
yv iso up
yv iso seed
yv api '{"op":"account.add","engine":"claude","name":"TEST-Work"}' --wait '{"op":"accounts"}' --save work_acct='accounts[name=TEST-Work].id'
yv api '{"op":"account.add","engine":"codex","name":"TEST-Codex"}' --wait '{"op":"accounts"}' --save codex_acct='accounts[name=TEST-Codex].id'
yv api '{"op":"login.start","engine":"claude","account":"{{work_acct}}"}' --wait '{"op":"login"}'
yv api '{"op":"login.code","engine":"claude","account":"{{work_acct}}","code":"good-code"}' --wait '{"op":"engines","accounts":{"{{work_acct}}":{"loggedIn":true}}}' --timeout 20000
```

## Cleanup

```sh
yv iso down
```

## Recipes

### selected-host-tabs
Viewports: both
Only one inspector appears. Engine logins and queue policy are in separate tabs.

```sh
yv drive 'hosts' --steps '[{"expect":"role:button:Select iso-host"},{"expect":"css:.host-inspector","count":1},{"expect":"role:tab:Sessions"},{"expect":"css:.engines","count":0},{"click":"role:tab:Engine logins"},{"expect":"css:.engine-row","text":"Claude Code"},{"expect":"css:.account-row","text":"TEST-Work"},{"click":"role:tab:System"},{"expect":"css:.host-system","text":"Session capacity"},{"expect":"css:.host-inspector-panel","text":"New sessions are queued while load is above"},{"expect":"css:.engines","count":0},{"eval":"document.documentElement.scrollWidth === innerWidth && document.querySelector(\".main\").scrollWidth === document.querySelector(\".main\").clientWidth","equals":true},{"shot":"host-system"},{"click":"role:tab:Sessions"},{"shot":"host-sessions"}]'
```

### compact-usage-popover
Viewports: both
Usage takes one footer row. On New session the popover opens on the draft's
account (Claude · Default, 47%) and lists the others, TEST-Work's 91% included,
under Other accounts. It is contained by the viewport and closes on Escape,
restoring focus. Setup signs TEST-Work in: an account that needs sign-in shows
no percent (#358).

```sh
yv drive 'new' --viewport desktop --steps '[{"expect":"role:button:Subscription usage","text":"91% used"},{"eval":"document.querySelector(\".usage-dock\").getBoundingClientRect().height <= 48","equals":true},{"click":"role:button:Subscription usage"},{"expect":"css:.usage-active[aria-label=\"Claude \u00b7 Default\"]","text":"47% used"},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude \u00b7 TEST-Work","text":"91%"},{"expect":"role:region:Codex \u00b7 Default"},{"eval":"(() => { const r=document.querySelector(\".anchored-popover\").getBoundingClientRect(); return r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight; })()","equals":true},{"shot":"usage-popover"},{"press":"Escape"},{"expect":"role:dialog:Subscription usage","count":0},{"eval":"document.activeElement?.getAttribute(\"aria-label\")","equals":"Subscription usage"}]'
yv drive 'new' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":"role:button:Subscription usage","text":"91% used"},{"eval":"document.querySelector(\".usage-dock\").getBoundingClientRect().height <= 48","equals":true},{"click":"role:button:Subscription usage"},{"expect":"css:.usage-active[aria-label=\"Claude \u00b7 Default\"]","text":"47% used"},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude \u00b7 TEST-Work","text":"91%"},{"expect":"role:region:Codex \u00b7 Default"},{"eval":"(() => { const r=document.querySelector(\".anchored-popover\").getBoundingClientRect(); return r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight; })()","equals":true},{"shot":"usage-popover"},{"press":"Escape"},{"expect":"role:dialog:Subscription usage","count":0},{"eval":"document.activeElement?.getAttribute(\"aria-label\")","equals":"Subscription usage"}]'
```

### usage-popover-leads-with-trigger-account
Viewports: both
With no account selected (Settings, Hosts, Connectors) the popover opens on the
account whose meter the trigger shows: TEST-Work's 91%, with Default's 47%
under Other accounts. Until #323 it led with Default, so the number you
clicked was folded away.

```sh
yv drive 'settings' --viewport desktop --steps '[{"expect":"role:button:Subscription usage","text":"91% used"},{"click":"role:button:Subscription usage"},{"expect":"css:.usage-active[aria-label=\"Claude \u00b7 TEST-Work\"]","text":"91% used"},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude \u00b7 Default","text":"47%"},{"expect":"role:region:Codex \u00b7 Default"},{"eval":"(() => { const r=document.querySelector(\".anchored-popover\").getBoundingClientRect(); return r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight; })()","equals":true},{"shot":"lead-account"},{"press":"Escape"},{"expect":"role:dialog:Subscription usage","count":0},{"eval":"document.activeElement?.getAttribute(\"aria-label\")","equals":"Subscription usage"}]'
yv drive 'settings' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":"role:button:Subscription usage","text":"91% used"},{"click":"role:button:Subscription usage"},{"expect":"css:.usage-active[aria-label=\"Claude \u00b7 TEST-Work\"]","text":"91% used"},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude \u00b7 Default","text":"47%"},{"expect":"role:region:Codex \u00b7 Default"},{"eval":"(() => { const r=document.querySelector(\".anchored-popover\").getBoundingClientRect(); return r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight; })()","equals":true},{"shot":"lead-account"},{"press":"Escape"},{"expect":"role:dialog:Subscription usage","count":0},{"eval":"document.activeElement?.getAttribute(\"aria-label\")","equals":"Subscription usage"}]'
```

### usage-row-lines-up-with-host-rows
Viewports: both
The sidebar's Usage row spans the same width as the host rows under it. It used
to start 8 px left of them, against the sidebar's edge, and end 8 px short,
from the quiet button's negative margin.

```sh
yv drive 'new' --viewport desktop --steps '[{"expect":"role:button:Subscription usage"},{"expect":"css:.host-row"},{"eval":"const u = document.querySelector(\".usage-trigger\").getBoundingClientRect(); const h = document.querySelector(\".host-row\").getBoundingClientRect(); [u.left - h.left, u.right - h.right].map(Math.round).join()","equals":"0,0"},{"hover":"role:button:Subscription usage"},{"shot":"usage-row"}]'
yv drive 'new' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":"role:button:Subscription usage"},{"wait":400},{"eval":"const u = document.querySelector(\".usage-trigger\").getBoundingClientRect(); const h = document.querySelector(\".host-row\").getBoundingClientRect(); [u.left - h.left, u.right - h.right].map(Math.round).join()","equals":"0,0"},{"hover":"role:button:Subscription usage"},{"shot":"usage-row"}]'
```

### subscriptions-page
Viewports: both
Accounts appear once, with reset times and source host badges. Reload preserves the route.

```sh
yv drive 'subscriptions' --steps '[{"expect":"role:heading:Subscriptions"},{"expect":"role:region:Claude · TEST-Work","text":"91% used"},{"expect":"role:region:Claude · Default","count":1},{"expect":"role:region:Codex · Default","text":"34% used"},{"expect":"css:.subscription-hosts","text":"iso-host"},{"expect":"css:.subscription-row","text":"resets in"},{"eval":"document.documentElement.scrollWidth === innerWidth && document.querySelector(\".main\").scrollWidth === document.querySelector(\".main\").clientWidth","equals":true},{"shot":"subscriptions"},{"reload":true},{"expect":"role:heading:Subscriptions"}]'
```

### account-picker-separate-from-model
Viewports: both
Choosing a named account updates a separate chip, preserves the task draft, and the model menu contains models without segmented account buttons.

```sh
yv drive 'new' --viewport desktop --steps '[{"fill":"label:Message","value":"TEST-keep this draft"},{"click":"css:.account-trigger"},{"expect":"role:dialog:Engine account identity","text":"Choose an account"},{"click":{"role":"menuitemradio","name":"Claude · TEST-Work"}},{"expect":"role:button:Account: Claude, TEST-Work"},{"expect":"label:Message","value":"TEST-keep this draft"},{"click":"role:button:Model Opus 5.5"},{"expect":"css:.picker-accounts","count":0},{"expect":{"css":".picker-model","hasText":"Sonnet 5.5"}},{"press":"Escape"},{"eval":"document.documentElement.scrollWidth === innerWidth && document.querySelector(\".main\").scrollWidth === document.querySelector(\".main\").clientWidth","equals":true},{"shot":"separate-account"}]'
yv drive 'new' --viewport mobile --steps '[{"click":"role:button:Session setup"},{"fill":"label:Message","value":"TEST-keep this draft"},{"click":"css:.account-trigger"},{"expect":"role:dialog:Engine account identity","text":"Choose an account"},{"click":{"role":"menuitemradio","name":"Claude \u00b7 TEST-Work"}},{"expect":"role:button:Account: Claude, TEST-Work"},{"expect":"label:Message","value":"TEST-keep this draft"},{"click":"role:button:Model Opus 5.5"},{"expect":"css:.picker-accounts","count":0},{"expect":{"css":".picker-model","hasText":"Sonnet 5.5"}},{"press":"Escape"},{"eval":"document.documentElement.scrollWidth === innerWidth && document.querySelector(\".main\").scrollWidth === document.querySelector(\".main\").clientWidth","equals":true},{"shot":"separate-account"}]'
```

### other-engine-account-and-fixed-session
Viewports: both
An account on another engine switches both together. Sending uses that account, which stays read-only in the resulting session.

```sh
yv drive 'new' --viewport desktop --steps '[{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Codex · TEST-Codex"}},{"expect":"role:button:Account: Codex, TEST-Codex"},{"expect":"role:button:Model Fake Fast"},{"click":{"role":"button","name":"work","exact":true}},{"fill":"role:textbox:Folder","value":"{{repo}}"},{"press":"Enter"},{"fill":"label:Message","value":"TEST-focused account"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-focused account"},{"click":"role:button:Account: Codex, TEST-Codex"},{"expect":"role:dialog:Engine account identity","text":"This session runs as"},{"expect":"css:.anchored-popover [role=menuitemradio]","count":0},{"shot":"fixed-account"},{"press":"Escape"}]'
yv drive 'new' --viewport mobile --steps '[{"click":"role:button:Session setup"},{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Codex \u00b7 TEST-Codex"}},{"expect":"role:button:Account: Codex, TEST-Codex"},{"expect":"role:button:Model Fake Fast"},{"press":"Escape"},{"click":{"role":"button","name":"work","exact":true}},{"fill":"role:textbox:Folder","value":"{{repo}}"},{"press":"Enter"},{"fill":"label:Message","value":"TEST-focused account"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-focused account"},{"click":"role:button:Session setup"},{"click":"role:button:Account: Codex, TEST-Codex"},{"expect":"role:dialog:Engine account identity","text":"This session runs as"},{"expect":"css:.anchored-popover [role=menuitemradio]","count":0},{"shot":"fixed-account"},{"press":"Escape"}]'
```

### sessions-search-and-archive
Viewports: both
The session page searches active conversations, opens a result, and finds an archived conversation through its filter.

```sh
yv api '{"op":"session.update","sessionId":"{{sid_think}}","archived":true}'
yv drive 'sessions' --steps '[{"expect":"role:heading:Sessions"},{"fill":"label:Search sessions","value":"TEST-say hello"},{"expect":"css:.focused-session-row","count":1},{"click":"css:.focused-session-row"},{"expect":"css:.thread","text":"Echo: TEST-say hello"},{"goto":"sessions"},{"click":{"role":"button","name":"Archived","exact":true}},{"expect":{"css":".focused-session-row","hasText":"TEST-plan the refactor"}},{"fill":"label:Search sessions","value":"TEST-no-match"},{"expect":"css:.focused-session-row","count":0},{"expect":"css:.focused-empty"},{"shot":"empty-search"}]'
yv api '{"op":"session.update","sessionId":"{{sid_think}}","archived":false}'
```

### sessions-visibility-tabs
Viewports: both
The Sessions page opens on Active, which leaves archived sessions out. All
sessions lists active and archived together, the archived row labelled
Archived. Archived lists only the archived ones. The tab in use reads as
pressed. Read back: the daemon still has the session archived until the
recipe restores it.

```sh
yv api '{"op":"session.update","sessionId":"{{sid_think}}","archived":true}'
yv state --expect '{"sessions":[{"id":"{{sid_think}}","archived":true}]}'
yv drive 'sessions' --steps '[{"expect":"role:heading:Sessions"},{"expect":{"role":"button","name":"Active","exact":true}},{"eval":"document.querySelector(\".sessions-toolbar button[aria-pressed=true]\").textContent","equals":"Active"},{"expect":{"css":".focused-session-row","hasText":"TEST-say hello"}},{"expect":{"css":".focused-session-row","hasText":"TEST-plan the refactor"},"count":0},{"click":{"role":"button","name":"All sessions","exact":true}},{"eval":"document.querySelector(\".sessions-toolbar button[aria-pressed=true]\").textContent","equals":"All sessions"},{"expect":{"css":".focused-session-row","hasText":"TEST-say hello"}},{"expect":{"css":".focused-session-row","hasText":"TEST-plan the refactor"},"text":"Archived"},{"shot":"all-sessions"},{"click":{"role":"button","name":"Archived","exact":true}},{"expect":{"css":".focused-session-row","hasText":"TEST-plan the refactor"},"text":"Archived"},{"expect":{"css":".focused-session-row","hasText":"TEST-say hello"},"count":0},{"eval":"[...document.querySelectorAll(\".focused-session-row .focused-session-state\")].every((el) => el.textContent.trim() === \"Archived\")","equals":true},{"shot":"archived-only"}]'
yv api '{"op":"session.update","sessionId":"{{sid_think}}","archived":false}'
yv state --expect '{"sessions":[{"id":"{{sid_think}}","archived":false}]}'
```

### sessions-search-no-match-says-so
Viewports: both
A search that matches nothing says so and offers to clear it. It used to say
"No sessions to show. Start a session whenever you're ready." with a New
session button, as if there were no sessions at all.

```sh
yv drive 'sessions' --steps '[{"expect":"role:heading:Sessions"},{"expect":{"css":".focused-session-row","hasText":"TEST-say hello"}},{"fill":"label:Search sessions","value":"TEST-matches-nothing"},{"expect":"css:.focused-session-row","count":0},{"expect":"css:.focused-empty","text":"No sessions match your search."},{"expect":{"css":".focused-empty button","hasText":"New session"},"count":0},{"shot":"no-match"},{"click":{"css":".focused-empty button","hasText":"Clear search"}},{"expect":"label:Search sessions","value":""},{"expect":{"css":".focused-session-row","hasText":"TEST-say hello"}}]'
```

### sessions-search-finds-the-account
Viewports: both
Each row names the account its session runs on, so searching for that name
lists the session. Until 2026-11-24 the search skipped the account and said
"No sessions match your search." (#394).

```sh
yv api '{"op":"session.create","engine":"codex","account":"{{codex_acct}}","model":"fake-fast","effort":"low","cwd":"{{repo}}","text":"TEST-on the codex account"}' --wait '{"op":"created"}' --save acct_search_sid=sessionId
yv drive 'sessions' --steps '[{"expect":{"css":".focused-session-row","hasText":"TEST-on the codex account"},"text":"TEST-Codex"},{"fill":"label:Search sessions","value":"test-codex"},{"expect":{"css":".focused-session-row","hasText":"TEST-on the codex account"}},{"expect":{"css":".focused-session-row","hasText":"TEST-say hello"},"count":0},{"expect":"css:.focused-empty","count":0},{"shot":"search-by-account"}]'
```

### host-rows-line-up-with-the-page
Viewports: both
Host rows and the "Run sessions on another machine" tile span the same column
as the host details under them. They used to start 8 px left of it (and the
rows end 8 px short), from the quiet button's negative margin.

```sh
yv drive hosts --steps '[{"expect":"role:button:Select iso-host"},{"expect":"css:.host-empty"},{"eval":"const col = document.querySelector(\".focused-hosts\").getBoundingClientRect(); [\".host-summary\", \".host-empty\"].every((s) => { const r = document.querySelector(s).getBoundingClientRect(); return Math.abs(r.left - col.left) <= 1 && Math.abs(r.right - col.right) <= 1; })","equals":true},{"shot":"hosts"}]'
```

### session-rows-line-up-with-the-page
Viewports: both
Session rows on the Sessions page span the same column as the tabs and the
search box above them. They used to start 8 px left of it (and their dividers
end 8 px short), from the quiet button's negative margin (#246).

```sh
yv drive sessions --steps '[{"expect":{"css":".focused-session-row","hasText":"TEST-say hello"}},{"eval":"const col = document.querySelector(\".focused-session-list\").getBoundingClientRect(); const rows = [...document.querySelectorAll(\".focused-session-row\")]; rows.length > 0 && rows.every((e) => { const r = e.getBoundingClientRect(); return Math.abs(r.left - col.left) <= 1 && Math.abs(r.right - col.right) <= 1; })","equals":true},{"shot":"session-rows"}]'
```

### session-rows-have-straight-dividers
Viewports: both
The line under each session row on the Sessions page is straight end to end,
like the dividers on Subscriptions and Import. The rows are quiet buttons,
and they kept the button's 8 px corners, so each divider curved up at both
ends.

```sh
yv drive sessions --steps '[{"expect":{"css":".focused-session-row","hasText":"TEST-say hello"}},{"eval":"[...document.querySelectorAll(\".focused-session-row\")].every((e) => { const cs = getComputedStyle(e); return cs.borderBottomLeftRadius === \"0px\" && cs.borderBottomRightRadius === \"0px\"; })","equals":true},{"shot":"session-row-dividers"}]'
```

### offline-host-and-selection-fallback
Viewports: both
An unreachable host shows its problem, retains System actions, and removing it returns selection to the local host.

```sh
yv api '{"op":"host.add","name":"TEST-offline","kind":"ssh","destination":"test@unreachable.invalid"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-offline","status":"error"}]}'
yv drive 'hosts' --viewport desktop --steps '[{"click":"role:button:Select TEST-offline"},{"expect":"css:.host-problem"},{"expect":"css:.host-inspector-panel","text":"reconnects"},{"click":"role:tab:System"},{"eval":"document.documentElement.scrollWidth === innerWidth && document.querySelector(\".main\").scrollWidth === document.querySelector(\".main\").clientWidth","equals":true},{"shot":"offline-system"},{"click":"role:button:Remove host"},{"click":"role:button:Click again to remove"},{"expect":"role:button:Select TEST-offline","count":0},{"expect":"role:button:Select iso-host"},{"expect":"css:.host-inspector-heading","text":"iso-host"}]'
yv api '{"op":"host.add","name":"TEST-offline","kind":"ssh","destination":"test@unreachable.invalid"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-offline","status":"error"}]}'
yv drive 'hosts' --viewport mobile --steps '[{"click":"role:button:Select TEST-offline"},{"expect":"css:.host-problem"},{"expect":"css:.host-inspector-panel","text":"reconnects"},{"click":"role:tab:System"},{"eval":"document.documentElement.scrollWidth === innerWidth && document.querySelector(\".main\").scrollWidth === document.querySelector(\".main\").clientWidth","equals":true},{"shot":"offline-system"},{"click":"role:button:Remove host"},{"click":"role:button:Click again to remove"},{"expect":"role:button:Select TEST-offline","count":0},{"expect":"role:button:Select iso-host"},{"expect":"css:.host-inspector-heading","text":"iso-host"}]'
```

### workspace-widths-and-views
Viewports: desktop
Across eight pane widths the header stays one row and every control stays inside it. Terminal and Processes switch without discarding the shell.

```sh
yv drive 'session:{{sid_edit}}' --viewport desktop --steps '[{"eval":"(async () => { const pane=document.querySelector(\".pane\"); for (const width of [300,350,400,440,441,460,520,600]) { pane.style.setProperty(\"--pane-w\",width+\"px\"); await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))); const bar=pane.querySelector(\".tabs\").getBoundingClientRect(); const controls=[...pane.querySelectorAll(\".tabs button,.tabs select\")].filter(b=>b.getClientRects().length); if (Math.round(bar.height)!==52 || controls.some(b=>{ const r=b.getBoundingClientRect(); return r.left<bar.left || r.right>bar.right+.5 || r.bottom>bar.bottom; })) return {badWidth:width}; } return true; })()","equals":true},{"eval":"document.querySelector(\".pane\").style.setProperty(\"--pane-w\",\"300px\"); true","equals":true},{"click":"role:button:Terminal"},{"expect":"label:Terminal input"},{"click":"role:button:Processes"},{"expect":"css:.proc-table"},{"click":"role:button:Terminal"},{"expect":"label:Terminal input"},{"shot":"workspace-tabs"}]'
```

### manage-accounts-opens-logins
Viewports: both
Manage accounts opens Engine logins on the selected host and keeps that destination after a reload. The composer has no standalone login email. On a phone the account picker sits in the Session setup chip.

```sh
yv drive new --viewport desktop --steps '[{"expect":"css:.composer-identity","count":0},{"click":"css:.account-trigger"},{"click":"role:button:Manage accounts"},{"expect":"role:heading:Hosts"},{"expect":"css:[role=tab][aria-selected=true]","text":"Engine logins"},{"expect":"css:.engine-row","text":"Claude Code"},{"reload":true},{"expect":"css:[role=tab][aria-selected=true]","text":"Engine logins"},{"shot":"manage-logins"}]'
yv drive new --viewport mobile --steps '[{"expect":"css:.composer-identity","count":0},{"click":"role:button:Session setup"},{"click":"css:.account-trigger"},{"click":"role:button:Manage accounts"},{"expect":"role:heading:Hosts"},{"expect":"css:[role=tab][aria-selected=true]","text":"Engine logins"},{"expect":"css:.engine-row","text":"Claude Code"},{"reload":true},{"expect":"css:[role=tab][aria-selected=true]","text":"Engine logins"},{"shot":"manage-logins"}]'
```

### sidebar-skips-duplicate-page-links
Viewports: desktop
Sessions and Subscriptions stay accessible from page navigation. The sidebar
keeps its session list and Usage entry.

```sh
yv drive new --viewport desktop --steps '[{"expect":{"css":".sidebar-top button","hasText":"Sessions","exact":true},"count":0},{"expect":{"css":".sidebar-top button","hasText":"Subscriptions","exact":true},"count":0},{"click":"role:button:Go to session or page"},{"fill":"role:combobox","value":"subscriptions"},{"press":"Enter"},{"expect":"role:heading:Subscriptions"}]'
```

### subscriptions-page-signed-out-account
Viewports: both
An account that needs sign-in shows Sign in, not its last usage, as the
sidebar usage popover does (#396). Sign in opens the Engine logins tab.

```sh
yv api '{"op":"account.add","engine":"claude","name":"TEST-Away"}' --wait '{"op":"accounts"}'
yv drive 'subscriptions' --steps '[{"expect":"role:heading:Subscriptions"},{"expect":"role:region:Claude · TEST-Work","text":"91% used"},{"expect":"role:region:Claude · TEST-Away","text":"Needs sign-in"},{"expect":"role:region:Claude · TEST-Away","text":"Sign in to see usage"},{"wait":1500},{"eval":"[...document.querySelectorAll(\".subscription-row\")].find((r) => r.getAttribute(\"aria-label\") === \"Claude · TEST-Away\").querySelectorAll(\".usage-percent, [role=progressbar]\").length","equals":0},{"eval":"document.documentElement.scrollWidth === innerWidth","equals":true},{"shot":"signed-out-account"},{"click":"role:button:Sign in — Claude · TEST-Away"},{"expect":"css:[role=tab][aria-selected=true]","text":"Engine logins"},{"expect":{"css":".account-row","hasText":"TEST-Away"},"text":"not logged in"}]'
```

## Gotchas
- On New session the popover follows the draft's account by design
  (`compact-usage-popover` expects Default there); elsewhere, with no account
  selected, it opens on the account the trigger reads (`leadRow` in
  `ui/src/UsageMeters.tsx`, #323). Until 2026-11-21 it led with Default, and
  `compact-usage-popover` expected TEST-Work on New session with TEST-Work
  signed out (no percent since #358), so it failed for two reasons.
- Below 700 px the new-session composer folds the account, model, effort and
  permission pickers into one `Session setup` chip (#290), so a recipe that
  opens the account picker on a phone clicks `role:button:Session setup`
  first. `manage-accounts-opens-logins` ran one step list at both viewports
  and timed out on `.account-trigger` at mobile until 2026-11-20.
