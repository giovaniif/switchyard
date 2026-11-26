# Hosts

Compact host rows show CPU, memory, disk and connection state. Selecting a host opens one inspector below the list: Sessions, Engine logins, and System. Login notes never resize the host summaries. Remotes use SSH or a direct WebSocket URL; their sessions join the same sidebar.

## Setup

```sh
yv iso up
yv iso seed
yv var set remote_port "$(yv iso up --name map-hosts-remote --host-name TEST-remote | jq -r .iso.port)"
yv var set remote_work "$(yv var get work --iso map-hosts-remote | jq -er .value)"
```

## Cleanup

```sh
yv iso down --name map-hosts-remote
# Recipes that fail stop before their own teardown; a remote left up breaks the next run.
yv iso down --name map-hosts-signin
yv iso down
```

## Recipes

### hosts-page
Viewports: both
The host summary shows its state and resources. The selected inspector separates logins from sessions.

```sh
yv drive 'hosts' --steps '[{"expect":"role:heading:Hosts"},{"expect":"role:button:Select iso-host","text":"CPU"},{"click":"role:tab:Engine logins"},{"expect":{"css":".engine-row","hasText":"Claude Code"},"text":"logged in as me@example.com"},{"expect":{"css":".engine-row","hasText":"Codex"},"text":"logged in as me@example.com"},{"shot":"hosts-logins"}]'
```

### empty-state-tile
With no remote host, the dashed invitation opens Add host. Cancel restores it.

```sh
yv drive 'hosts' --steps '[{"click":"role:button:Run sessions on another machine"},{"expect":"role:region:Add host"},{"expect":"role:button:Run sessions on another machine","count":0},{"shot":"add-host"},{"click":"role:button:Cancel"},{"expect":"role:button:Run sessions on another machine"}]'
```

### sessions-list-collapsed
Viewports: both
The Sessions tab lists activity with clickable titles; logins and queue policy live in their own tabs.

```sh
yv drive 'hosts' --steps '[{"expect":{"css":".focused-session-row","hasText":"TEST-hello from Claude"}},{"expect":"css:.engines","count":0},{"click":"role:tab:System"},{"expect":"css:.host-inspector-panel","text":"New sessions are queued while load is above"},{"click":"role:tab:Sessions"},{"click":{"css":".focused-session-row","hasText":"TEST-hello from Claude"}},{"expect":"css:.thread","text":"hello from Claude"}]'
```

### add-direct-host
A direct host connects; its card and sidebar row appear.

```sh
yv api '{"op":"host.add","name":"TEST-remote","kind":"direct","url":"ws://127.0.0.1:{{remote_port}}/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-remote","status":"connected"}]}' --save remote_id='hosts[name=TEST-remote].id'
yv drive hosts --viewport desktop --steps '[{"expect":"role:button:Select TEST-remote"},{"expect":{"css":".host-row","hasText":"TEST-remote"}}]'
```

### remote-session
A session created on the remote runs there and shows in the hub's sidebar.

```sh
yv api '{"op":"session.create","host":"{{remote_id}}","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{remote_work}}","text":"TEST-on the remote"}' --wait '{"op":"created"}' --save rsid=sessionId
yv drive 'session:{{rsid}}' --steps '[{"expect":"css:.thread","text":"Echo: TEST-on the remote"}]'
yv state --iso map-hosts-remote --expect '{"sessions":[{"title":"TEST-on the remote","status":"idle"}]}'
```

### hosts-grid
Viewports: both
Compact host rows share one full width. Selecting another host replaces the single inspector without growing either summary.

```sh
yv api '{"op":"host.add","name":"TEST-grid","kind":"direct","url":"ws://127.0.0.1:{{remote_port}}/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-grid","status":"connected"}]}' --save grid_id='hosts[name=TEST-grid].id'
yv drive 'hosts' --steps '[{"expect":"role:button:Select TEST-grid"},{"eval":"(() => { const rows=[...document.querySelectorAll(\".host-summary\")]; const a=rows[0].getBoundingClientRect(), b=rows.at(-1).getBoundingClientRect(); return b.top>=a.bottom && Math.round(a.width)===Math.round(b.width); })()","equals":true},{"click":"role:button:Select TEST-grid"},{"expect":"css:.host-inspector","count":1},{"expect":"css:.host-inspector-heading","text":"TEST-grid"},{"shot":"selected-remote"},{"click":"role:tab:Engine logins"},{"expect":"css:.engine-row","text":"Claude Code"},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true}]'
yv api '{"op":"host.remove","hostId":"{{grid_id}}"}' --wait '{"op":"hosts"}'
```

### selecting-a-host-shows-its-details
Viewports: both
On a phone the iPhone setup card and the host rows fill the screen, so the
inspector starts below the fold. Selecting a host brings its details into view;
otherwise the tap only moves the row's border. When the details are already in
view (desktop), the page stays where it is. The click goes through
`element.click()` so Playwright's own scroll-into-view doesn't hide either case.

```sh
yv api '{"op":"host.add","name":"TEST-reveal","kind":"direct","url":"ws://127.0.0.1:{{remote_port}}/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-reveal","status":"connected"}]}' --save reveal_id='hosts[name=TEST-reveal].id'
yv drive 'hosts' --steps '[{"expect":"role:button:Select TEST-reveal"},{"eval":"(() => { const h = document.querySelector(\".host-inspector-heading\").getBoundingClientRect(); window.__wasVisible = h.top >= 0 && h.bottom <= innerHeight; window.__top = document.querySelector(\".hosts .scroll\").scrollTop; document.querySelector(\"[aria-label=\\\"Select TEST-reveal\\\"]\").click(); return true; })()","equals":true},{"expect":"css:.host-inspector-heading","text":"TEST-reveal"},{"wait":800},{"eval":"(() => { const r = document.querySelector(\".host-inspector-heading\").getBoundingClientRect(); const still = !window.__wasVisible || document.querySelector(\".hosts .scroll\").scrollTop === window.__top; return r.top >= 0 && r.bottom <= innerHeight && still; })()","equals":true},{"shot":"selected-in-view"}]'
yv api '{"op":"host.remove","hostId":"{{reveal_id}}"}' --wait '{"op":"hosts"}'
```

### ssh-host-unreachable
An SSH host that cannot be reached reports an error instead of hanging.

```sh
yv api '{"op":"host.add","name":"TEST-ssh","kind":"ssh","destination":"test@unreachable.invalid"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-ssh","status":"error"}]}' --timeout 30000
yv drive hosts --viewport desktop --steps '[{"expect":"role:button:Select TEST-ssh"}]'
```

### remove-host
Viewports: desktop
System asks for a second click to remove a host, which disappears across reloads.

```sh
yv api '{"op":"host.add","name":"TEST-gone","kind":"ssh","destination":"test@gone.invalid"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-gone"}]}'
yv drive 'hosts' --viewport desktop --steps '[{"click":"role:button:Select TEST-gone"},{"click":"role:tab:System"},{"click":"role:button:Remove host"},{"expect":"css:.host-foot","text":"Its sessions keep running there."},{"click":"role:button:Click again to remove"},{"expect":"role:button:Select TEST-gone","count":0},{"reload":true},{"expect":"role:button:Select TEST-gone","count":0}]'
```

### refused-add-keeps-the-form
Viewports: desktop
A value yardd refuses (remote port 99999) shows the error and keeps the form
with what was typed; fixing the field and adding again closes it once the host
appears.

```sh
yv drive hosts --viewport desktop --allow-errors --steps '[{"click":"role:button:Add host"},{"fill":"label:SSH destination","value":"test@TEST-devbox"},{"click":"role:button:Options"},{"fill":"label:Remote port","value":"99999"},{"click":"role:button:Add TEST-devbox"},{"expect":"css:.banner-error","text":"remote port must be a number"},{"expect":"label:SSH destination","value":"test@TEST-devbox"},{"expect":"label:Remote port","value":"99999"},{"shot":"refused"},{"fill":"label:Remote port","value":"2222"},{"click":"role:button:Add TEST-devbox"},{"expect":"role:button:Select TEST-devbox"},{"expect":"role:region:Add host","visible":false},{"shot":"added"}]'
```

### local-name-right-after-start
Viewports: desktop
Right after yardd starts, before its first stats sample (about 3 s), the
local host already shows its own name (`iso-host`, from YARD_HOST_NAME), not
"This Mac", in the sidebar's host row and the new-session host picker. The
daemon test `the_snapshot_names_this_machine_before_its_first_stats_sample`
pins the snapshot field without the timing.

```sh
yv iso restart
yv drive new --viewport desktop --steps '[{"expect":{"css":".host-row .host-name","hasText":"iso-host"}},{"expect":{"css":".main button","hasText":"iso-host"}},{"expect":{"css":".host-row .host-name","hasText":"This Mac"},"count":0}]'
```

### no-memory-limit-says-load-only
Viewports: both
When the memory limit is disabled System explains the load rule alone.

```sh
yv iso up --name map-hosts-nomem --env YARD_MIN_FREE_GB=0 >/dev/null
yv drive 'hosts' --iso map-hosts-nomem --steps '[{"click":"role:tab:System"},{"expect":"css:.host-inspector-panel","text":"New sessions are queued while load is above"},{"expect":{"css":".host-inspector-panel","hasText":"GB of memory"},"count":0},{"shot":"load-only"}]'
yv iso down --name map-hosts-nomem
```

### ip-destination-keeps-the-whole-address
Viewports: both
An SSH destination that is an IP address names the host by the whole address,
not its first part: `test@100.64.0.7` offers "Add 100.64.0.7" and saves that
name, and a second host on the same tailnet (`test@100.64.0.8`) is added too,
not refused as "a host named 100 already exists". A host name still gives its
first label (`TEST-devbox` in refused-add-keeps-the-form).

```sh
yv drive hosts --viewport mobile --steps '[{"click":"role:button:Add host"},{"fill":"label:SSH destination","value":"test@100.64.0.9"},{"expect":{"css":".add-host .btn-primary"},"text":"Add 100.64.0.9"},{"shot":"ip-label"}]'
yv drive hosts --viewport desktop --steps '[{"click":"role:button:Add host"},{"fill":"label:SSH destination","value":"test@100.64.0.7"},{"click":"role:button:Add 100.64.0.7"},{"expect":"role:button:Select 100.64.0.7"},{"click":"role:button:Add host"},{"fill":"label:SSH destination","value":"test@100.64.0.8"},{"click":"role:button:Add 100.64.0.8"},{"expect":"role:button:Select 100.64.0.8"},{"expect":{"css":".host-row","hasText":"100.64.0.8"}},{"expect":"css:.banner-error","count":0},{"shot":"two-ip-hosts"}]'
```

### long-name-fits-the-add-button
Viewports: both
A long host name keeps the Add button on one line inside the card, with
Cancel beside it: on a phone the label ends in an ellipsis, and the button's
accessible name stays whole. A Tailscale IPv6 destination names the host by
the whole 39-character address; at 390 px that label wrapped inside the 32 px
button, its second line cut off, and Cancel and Options dropped to their own
line. A custom name at the 40-character limit (Options → Name) with no spaces
ran past the card. On desktop the label still shows the whole name.

```sh
yv drive hosts --viewport mobile --steps '[{"click":"role:button:Add host"},{"fill":"label:SSH destination","value":"test@fd7a:115c:a1e0:ab12:4843:cd96:6263:d71c"},{"expect":"role:button:Add fd7a:115c:a1e0:ab12:4843:cd96:6263:d71c"},{"eval":"(() => { const card = document.querySelector(\".add-host\").getBoundingClientRect(); const b = document.querySelector(\".add-host .btn-primary\"); const r = b.getBoundingClientRect(); const cancel = [...document.querySelectorAll(\".add-host button\")].find(x => x.textContent.trim() === \"Cancel\").getBoundingClientRect(); return b.scrollHeight <= b.clientHeight + 1 && r.right <= card.right && cancel.right <= card.right && Math.abs(cancel.top - r.top) < 1 && document.documentElement.scrollWidth <= document.documentElement.clientWidth; })()","equals":true},{"shot":"ipv6-label"},{"click":"role:button:Options"},{"fill":"css:#host-name","value":"TEST-a-long-host-name-without-any-spaces"},{"eval":"(() => { const card = document.querySelector(\".add-host\").getBoundingClientRect(); const b = document.querySelector(\".add-host .btn-primary\"); const r = b.getBoundingClientRect(); const cancel = [...document.querySelectorAll(\".add-host button\")].find(x => x.textContent.trim() === \"Cancel\").getBoundingClientRect(); return b.scrollHeight <= b.clientHeight + 1 && r.right <= card.right && cancel.right <= card.right && Math.abs(cancel.top - r.top) < 1 && document.documentElement.scrollWidth <= document.documentElement.clientWidth; })()","equals":true},{"shot":"long-name-label"},{"click":"role:button:Add TEST-a-long-host-name-without-any-spaces"},{"expect":"role:button:Select TEST-a-long-host-name-without-any-spaces"}]'
yv drive hosts --viewport desktop --steps '[{"click":"role:button:Add host"},{"fill":"label:SSH destination","value":"test@fd7a:115c:a1e0:ab12:4843:cd96:6263:d71c"},{"eval":"(() => { const b = document.querySelector(\".add-host .btn-primary\"); return b.scrollWidth <= b.clientWidth && b.scrollHeight <= b.clientHeight + 1; })()","equals":true},{"shot":"ipv6-label-whole"},{"click":"role:button:Add fd7a:115c:a1e0:ab12:4843:cd96:6263:d71c"},{"expect":"role:button:Select fd7a:115c:a1e0:ab12:4843:cd96:6263:d71c"}]'
```

### long-derived-name-asks-for-a-shorter-one
Viewports: both
An SSH destination whose first DNS label is over 40 characters (a label may
be up to 63) derives a name yardd refuses. The form says so before sending:
Options opens with the Name field and "Use 40 characters or fewer." under it,
and Add stays disabled (also on Enter), so no "the host name must be at most
40 plain characters" banner pointing at a field the user could not see. A
shorter name under Options adds the host. One drive per viewport: both share
the daemon, so each adds its own name.

```sh
yv drive hosts --viewport mobile --steps '[{"click":"role:button:Add host"},{"fill":"label:SSH destination","value":"test@build-machine-with-a-really-long-hostname-for-testing.tail914724.ts.net"},{"expect":"css:#host-name","visible":true},{"expect":"css:.add-host","text":"Use 40 characters or fewer."},{"expect":{"css":".add-host .btn-primary"},"enabled":false},{"press":"Enter"},{"wait":500},{"expect":"css:.banner-error","count":0},{"shot":"too-long"},{"fill":"css:#host-name","value":"TEST-build-phone"},{"expect":{"css":".add-host","hasText":"Use 40 characters or fewer."},"count":0},{"click":"role:button:Add TEST-build-phone"},{"expect":"role:button:Select TEST-build-phone"},{"expect":"css:.banner-error","count":0},{"shot":"added"}]'
yv drive hosts --viewport desktop --steps '[{"click":"role:button:Add host"},{"fill":"label:SSH destination","value":"test@build-machine-with-a-really-long-hostname-for-testing.tail914724.ts.net"},{"expect":"css:#host-name","visible":true},{"expect":"css:.add-host","text":"Use 40 characters or fewer."},{"expect":{"css":".add-host .btn-primary"},"enabled":false},{"press":"Enter"},{"wait":500},{"expect":"css:.banner-error","count":0},{"shot":"too-long"},{"fill":"css:#host-name","value":"TEST-build-desktop"},{"expect":{"css":".add-host","hasText":"Use 40 characters or fewer."},"count":0},{"click":"role:button:Add TEST-build-desktop"},{"expect":"role:button:Select TEST-build-desktop"},{"expect":"css:.banner-error","count":0},{"shot":"added"}]'
```

### install-on-host (manual: needs a real SSH server)
"Install Switchyard" runs scripts/remote-install.sh over ssh; the daemon suite
covers it with a fake ssh and a file:// release.

### engine-status-keeps-cards-aligned
Viewports: both
Long login details cannot change summary row height or create horizontal overflow.

```sh
yv drive 'hosts' --steps '[{"click":"role:tab:Engine logins"},{"eval":"window.__summaryHeight=document.querySelector(\".host-summary\").getBoundingClientRect().height; document.querySelector(\".engine-row .muted\").textContent=\"logged in as TEST-a-very-long-account-name@a-very-long-domain.example · not logged in\"; true","equals":true},{"eval":"document.querySelector(\".host-summary\").getBoundingClientRect().height === window.__summaryHeight && document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"long-login"}]'
```

### processes-say-the-host-is-offline
Viewports: desktop
A remote session's Processes tab, open when its host goes away, says the list
comes back when the host reconnects. Once, it kept saying "Nothing is running
for this session." (or kept a stale list, with Stop buttons that could only
fail), and a tab first opened offline said "Loading…" forever. Uses its own
remote so the area's TEST-remote stays up. While the host is up the tab lists
the shell of the terminal the workspace opens by default (terminal-first,
#290), so it is a live table, not "Nothing is running", that must go away.

```sh
gone_port="$(yv iso up --name map-hosts-gone --host-name TEST-gone | jq -r .iso.port)"
gone_work="$(yv var get work --iso map-hosts-gone | jq -er .value)"
yv api '{"op":"host.add","name":"TEST-gone","kind":"direct","url":"ws://127.0.0.1:'"$gone_port"'/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-gone","status":"connected"}]}' --save gone_id='hosts[name=TEST-gone].id'
yv api '{"op":"session.create","host":"{{gone_id}}","engine":"codex","model":"fake-fast","effort":"low","cwd":"'"$gone_work"'","text":"TEST-procs on a host that goes"}' --wait '{"op":"created"}' --save gone_sid=sessionId
(sleep 8; yv iso down --name map-hosts-gone >/dev/null) &
yv drive 'session:{{gone_sid}}' --viewport desktop --allow-errors --steps '[{"expect":"css:.thread","text":"Echo: TEST-procs on a host that goes","timeout":15000},{"click":"role:button:Processes"},{"expect":"css:.proc-table","timeout":10000},{"expect":"css:.bar-host","text":"offline","timeout":20000},{"expect":{"role":"region","name":"Workspace"},"text":"Processes are available when this host reconnects.","timeout":5000},{"expect":"css:.proc-table","count":0},{"shot":"procs-offline"}]'
wait
```

### new-session-says-the-remembered-host-is-offline
Viewports: both
New session keeps a remembered remote host selected while it is offline, with
Send disabled, and says why next to the chips. Until 2026-11-21 Send was just
grey: the message could be typed but nothing said the host was offline. Uses
its own remote, taken down before the page opens.

```sh
later_port="$(yv iso up --name map-hosts-later --host-name TEST-later | jq -r .iso.port)"
yv api '{"op":"host.add","name":"TEST-later","kind":"direct","url":"ws://127.0.0.1:'"$later_port"'/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-later","status":"connected"}]}' --save later_id='hosts[name=TEST-later].id'
yv iso down --name map-hosts-later
yv drive new --steps '[{"eval":"(localStorage.setItem(\"yard.host\", \"{{later_id}}\"), true)","equals":true},{"reload":true},{"expect":{"css":".new-chips button","hasText":"TEST-later"}},{"fill":"label:Message","value":"TEST-wait for the host"},{"expect":{"role":"button","name":"Send","exact":true},"enabled":false},{"expect":"css:.new-chips","text":"TEST-later is offline"},{"shot":"remembered-host-offline"}]'
```

### sessions-stay-listed-while-their-host-is-down
Viewports: both
A page opened (a reload, another window) while a remote host is down still
lists that host's sessions, marked Offline, and opening one says the host is
offline. Until 2026-11-21 the sessions vanished from the sidebar until the
host came back: yardd only relayed a remote's sessions over a live link.
Uses its own remote so the area's TEST-remote stays up. Only a hub that saw
the host since it started knows its sessions.

```sh
away_port="$(yv iso up --name map-hosts-away --host-name TEST-away | jq -r .iso.port)"
away_work="$(yv var get work --iso map-hosts-away | jq -er .value)"
yv api '{"op":"host.add","name":"TEST-away","kind":"direct","url":"ws://127.0.0.1:'"$away_port"'/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-away","status":"connected"}]}' --save away_id='hosts[name=TEST-away].id'
yv api '{"op":"session.create","host":"{{away_id}}","engine":"codex","model":"fake-fast","effort":"low","cwd":"'"$away_work"'","text":"TEST-listed while away"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
yv iso down --name map-hosts-away
yv drive new --viewport desktop --steps '[{"expect":{"css":".session-row","hasText":"TEST-listed while away"},"text":"Offline","timeout":20000},{"click":{"css":".session-row","hasText":"TEST-listed while away"}},{"expect":"css:.offline-note","text":"is offline"},{"shot":"listed-while-away"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":{"css":".session-row","hasText":"TEST-listed while away"},"text":"Offline","timeout":20000},{"shot":"listed-while-away"},{"click":{"css":".session-row","hasText":"TEST-listed while away"}},{"expect":"css:.offline-note","text":"is offline"}]'
```

### subscriptions-sign-in-opens-the-accounts-host
Viewports: both
Sign in on a signed-out account's Subscriptions row opens Engine logins on
the host that account lives on, as the sidebar usage popover does. Until
2026-11-25 it opened the host the Hosts page had selected (this machine), so
the account to sign in was not on screen (#430). Uses its own remote so the
account is on that host only.

```sh
signin_port="$(yv iso up --name map-hosts-signin --host-name TEST-signin | jq -r .iso.port)"
yv api '{"op":"host.add","name":"TEST-signin","kind":"direct","url":"ws://127.0.0.1:'"$signin_port"'/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-signin","status":"connected"}]}' --save signin_id='hosts[name=TEST-signin].id'
yv api '{"op":"account.add","host":"{{signin_id}}","engine":"claude","name":"TEST-Faraway"}' --wait '{"op":"accounts"}'
yv drive 'subscriptions' --steps '[{"expect":"role:region:Claude · TEST-Faraway","text":"Sign in to see usage"},{"click":"role:button:Sign in — Claude · TEST-Faraway"},{"expect":"css:.host-inspector-heading","text":"TEST-signin"},{"expect":"css:[role=tab][aria-selected=true]","text":"Engine logins"},{"expect":{"css":".account-row","hasText":"TEST-Faraway"},"text":"not logged in"},{"shot":"signin-on-its-host"}]'
yv api '{"op":"host.remove","hostId":"{{signin_id}}"}' --wait '{"op":"hosts"}'
yv iso down --name map-hosts-signin
```

## Gotchas
- Isolated environments replace `ssh` with `verify/fakes/ssh`, which always fails like an unreachable host.
