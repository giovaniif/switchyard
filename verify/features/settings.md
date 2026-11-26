# Settings

The Settings page (`#/settings`, the gear row in the sidebar's top nav, after
Import) holds what this browser keeps for itself: the **Notifications** switch
(see notifications) and, under **Sidebar**, a "Show in sidebar" switch per host,
This Mac included. A hidden host loses its row at the foot of the sidebar and
lends nothing to the plan-usage strip above the rows; the choice is kept per
browser (`localStorage` `yard.sidebar.hiddenHosts`, a list of host ids). The
Hosts page, the pickers and the session list still show every host.

The strip shows each engine and account once, from the shown hosts: an added
account gets a line with its name ("Claude · TEST-Work") above its meters, and
when two shown hosts report the same engine and account the one that reported
last wins (later reset time, then more used), else the first listed. Setup
gives the remote a Claude account TEST-Work and runs a scripted Claude session
under it, so only the remote has Claude TEST-Work limits (the fake reports 91%
for an added account) while This Mac has the seeded Default Claude (47%). The
remote does not read its limits in the background
(`YARD_CLAUDE_USAGE_EVERY_MS=0`), or it would report Default's 47% too.
Setup logs TEST-Work in on the remote (the fake takes `good-code`): an added
account reads logged out until then, and the popover shows a logged-out
account as "Sign in" instead of its limits.

## Setup

```sh
yv iso up
yv iso seed
yv var set remote_port "$(yv iso up --name map-settings-remote --host-name TEST-server --env YARD_CLAUDE_USAGE_EVERY_MS=0 | jq -r .iso.port)"
yv var set remote_work "$(yv var get work --iso map-settings-remote | jq -er .value)"
yv api '{"op":"host.add","name":"TEST-server","kind":"direct","url":"ws://127.0.0.1:{{remote_port}}/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-server","status":"connected"}]}' --save remote_id='hosts[name=TEST-server].id'
yv api '{"op":"account.add","engine":"claude","name":"TEST-Work","host":"{{remote_id}}"}' --wait '{"op":"accounts","hostId":"{{remote_id}}"}' --save work_acct='accounts[name=TEST-Work].id'
yv api '{"op":"login.start","engine":"claude","account":"{{work_acct}}","host":"{{remote_id}}"}' --wait '{"op":"login"}'
yv api '{"op":"login.code","engine":"claude","account":"{{work_acct}}","code":"good-code","host":"{{remote_id}}"}' --wait '{"op":"engines","hostId":"{{remote_id}}","accounts":{"{{work_acct}}":{"loggedIn":true}}}' --timeout 20000
yv api '{"op":"session.create","host":"{{remote_id}}","engine":"claude","account":"{{work_acct}}","model":"claude-haiku-5-5","effort":"medium","cwd":"{{remote_work}}","text":"TEST-work on the server"}' --wait '{"op":"usage","hostId":"{{remote_id}}","usage":{"accounts":{"{{work_acct}}":{"status":"allowed"}}}}' --timeout 30000
```

## Cleanup

```sh
yv iso down --name map-settings-remote
yv iso down
```

## Recipes

### settings-page
The gear row opens the page (marked active): the Notifications switch, and a
Show in sidebar switch per host, all hosts shown at first. The sidebar itself
has no switch. On a phone the drawer closes once the page opens.

```sh
yv drive settings --viewport desktop --steps '[{"expect":"role:heading:Settings"},{"expect":{"css":".nav-row.active","hasText":"Settings"}},{"expect":"role:switch:Notifications"},{"expect":{"css":"[role=switch][aria-checked=true][aria-label=\"Show iso-host in sidebar\"]"}},{"expect":{"css":"[role=switch][aria-checked=true][aria-label=\"Show TEST-server in sidebar\"]"}},{"expect":{"css":".sidebar [role=switch]"},"count":0},{"expect":{"css":".host-row","hasText":"iso-host"}},{"expect":{"css":".host-row","hasText":"TEST-server"}},{"shot":"settings"}]'
yv drive settings --viewport mobile --steps '[{"expect":"role:heading:Settings"},{"expect":{"css":".app.drawer-open"},"count":0},{"expect":"role:switch:Notifications"},{"expect":{"css":"[role=switch][aria-checked=true][aria-label=\"Show iso-host in sidebar\"]"}},{"expect":{"css":"[role=switch][aria-checked=true][aria-label=\"Show TEST-server in sidebar\"]"}},{"shot":"settings"},{"click":"role:button:Open sessions"},{"expect":{"css":".nav-row.active","hasText":"Settings"}},{"expect":{"css":".sidebar [role=switch]"},"count":0}]'
```

### hide-this-mac
Turning This Mac (here `iso-host`) off removes its row from the sidebar and
keeps the remote's; after a reload it is still off and still hidden. The Hosts
page (Manage hosts) still lists it; turning it back on brings the row back.

```sh
yv drive settings --viewport desktop --steps '[{"click":"role:switch:Show iso-host in sidebar"},{"expect":{"css":".host-row","hasText":"iso-host"},"count":0},{"expect":{"css":".host-row","hasText":"TEST-server"}},{"eval":"localStorage.getItem(\"yard.sidebar.hiddenHosts\")","equals":"[\"local\"]"},{"reload":true},{"expect":"role:heading:Settings"},{"expect":{"css":"[role=switch][aria-checked=false][aria-label=\"Show iso-host in sidebar\"]"}},{"expect":{"css":".host-row","hasText":"iso-host"},"count":0},{"expect":{"css":".host-row","hasText":"TEST-server"}},{"shot":"this-mac-hidden"},{"click":"role:button:Manage hosts"},{"expect":"role:heading:Hosts"},{"expect":"role:button:Select iso-host"},{"eval":"(history.back(),1)"},{"click":"role:switch:Show iso-host in sidebar"},{"expect":{"css":".host-row","hasText":"iso-host"}}]'
yv drive settings --viewport mobile --steps '[{"click":"role:switch:Show iso-host in sidebar"},{"reload":true},{"expect":{"css":"[role=switch][aria-checked=false][aria-label=\"Show iso-host in sidebar\"]"}},{"click":"role:button:Open sessions"},{"wait":400},{"expect":{"css":".host-row","hasText":"TEST-server"},"inViewport":true},{"expect":{"css":".host-row","hasText":"iso-host"},"count":0},{"shot":"this-mac-hidden"}]'
```

### strip-from-shown-hosts
Viewports: both
The usage popover reads only shown hosts; the dedicated Subscriptions page keeps all accounts discoverable even when every sidebar host is hidden. The popover leads with one account (on Settings, with nothing selected, the one the trigger reads: TEST-Work since #323) and folds the rest under Other accounts (one meter each), so the recipe opens it and finds each account by its region, whichever leads; the Subscriptions page (page search, route `subscriptions`) has every window.

```sh
yv drive 'settings' --viewport desktop --steps '[{"click":"role:button:Subscription usage"},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude · Default","text":"47%"},{"expect":"role:region:Claude · TEST-Work","text":"91%"},{"press":"Escape"},{"click":"role:switch:Show iso-host in sidebar"},{"click":"role:button:Subscription usage"},{"expect":{"css":".compact-meter","hasText":"47%"},"count":0},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude · TEST-Work","text":"91%"},{"expect":"role:region:Codex · Default","text":"12%"},{"shot":"usage-remote-only"},{"press":"Escape"},{"click":"role:switch:Show TEST-server in sidebar"},{"expect":"css:.host-row","count":0},{"click":"role:button:Subscription usage"},{"expect":"role:dialog:Subscription usage","text":"No subscription limits reported yet."},{"press":"Escape"},{"goto":"subscriptions"},{"expect":"role:heading:Subscriptions"},{"expect":"role:region:Claude · TEST-Work","text":"91% used"},{"expect":"role:region:Codex · Default","text":"34%"}]'
yv drive 'settings' --viewport mobile --steps '[{"click":"role:switch:Show iso-host in sidebar"},{"click":"role:button:Open sessions"},{"click":"role:button:Subscription usage"},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude · TEST-Work","text":"91%"},{"expect":{"css":".compact-meter","hasText":"47%"},"count":0},{"expect":"role:region:Codex · Default","text":"12%"},{"shot":"usage-remote-only"}]'
```

### long-host-name
Viewports: both
A 40-character host name (the most a name allows) in the Sidebar card is cut
with an ellipsis like the sidebar's own host row, and its cloud icon stays
whole inside the row instead of being clipped away with the overflow (#160).
The recipe adds an unreachable host for itself and removes it afterwards.

```sh
yv api '{"op":"host.add","name":"TEST-build-farm-long-name-for-sidebar-01","kind":"direct","url":"ws://127.0.0.1:9/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-build-farm-long-name-for-sidebar-01"}]}' --save long_host='hosts[name=TEST-build-farm-long-name-for-sidebar-01].id'
yv drive settings --viewport desktop --steps '[{"expect":{"css":".settings-host","hasText":"TEST-build-farm-long-name-for-sidebar-01"}},{"eval":"(() => { const row = [...document.querySelectorAll(\".settings-host\")].find((r) => r.textContent.includes(\"TEST-build-farm\")); const box = row.querySelector(\".settings-host-name\").getBoundingClientRect(); const c = row.querySelector(\".cloud\").getBoundingClientRect(); const walk = document.createTreeWalker(row, NodeFilter.SHOW_TEXT); let t; while ((t = walk.nextNode()) && !t.data.includes(\"TEST-build-farm\")); const st = getComputedStyle(t.parentElement); return [c.width > 0 && c.left >= box.left - 0.5 && c.right <= box.right + 0.5, st.textOverflow === \"ellipsis\" && !st.display.includes(\"flex\")].join(); })()","equals":"true,true"},{"shot":"long-host-name"}]'
yv drive settings --viewport mobile --steps '[{"expect":{"css":".settings-host","hasText":"TEST-build-farm-long-name-for-sidebar-01"}},{"expect":"role:switch:Show TEST-build-farm-long-name-for-sidebar-01 in sidebar","inViewport":true},{"eval":"(() => { const row = [...document.querySelectorAll(\".settings-host\")].find((r) => r.textContent.includes(\"TEST-build-farm\")); const box = row.querySelector(\".settings-host-name\").getBoundingClientRect(); const c = row.querySelector(\".cloud\").getBoundingClientRect(); const walk = document.createTreeWalker(row, NodeFilter.SHOW_TEXT); let t; while ((t = walk.nextNode()) && !t.data.includes(\"TEST-build-farm\")); const st = getComputedStyle(t.parentElement); return [c.width > 0 && c.left >= box.left - 0.5 && c.right <= box.right + 0.5, st.textOverflow === \"ellipsis\" && !st.display.includes(\"flex\")].join(); })()","equals":"true,true"},{"shot":"long-host-name"}]'
yv api '{"op":"host.remove","hostId":"{{long_host}}"}' --wait '{"op":"hosts"}'
```

## Gotchas
- Which of two hosts reported last is read from the reports themselves (reset
  times, then use); the fakes always report the same figures, so the tie-breaks
  are covered by `stripGroups` tests in `ui/src/usage.test.ts`, not here.
