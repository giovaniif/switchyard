# Launch defaults

Settings saves model/effort and host defaults for New sessions. Explicit
defaults beat last-used choices and survive reloads. Reset restores last-used
behavior. Configured offline session hosts stay selected; removed hosts fall
back. Switchy's defaults are gone: agents keep their own model (see agents).

## Setup

```sh
yv iso up
yv iso seed
yv var set remote_port "$(yv iso up --name map-launch-defaults-remote --host-name TEST-default-host | jq -r .iso.port)"
yv api '{"op":"host.add","name":"TEST-default-host","kind":"direct","url":"ws://127.0.0.1:{{remote_port}}/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-default-host","status":"connected"}]}' --save default_host='hosts[name=TEST-default-host].id'
```

## Cleanup

```sh
yv iso down
yv iso down --name map-launch-defaults-remote
```

## Recipes

### independent-defaults
Viewports: both
Save a model/host default, reload Settings and New session: both show it.
There is no Switchy defaults row any more.

```sh
M_DESKTOP='[{"expect":"role:button:Model Fake Deep"}]'
M_MOBILE='[{"click":"role:button:Session setup"},{"expect":"role:button:Model Fake Deep"},{"press":"Escape"}]'
A='[{"eval":"localStorage.removeItem(\"yard.defaults.sessions\"); true"},{"reload":true},{"click":"css:section[aria-labelledby=\"defaults-sessions\"] .chip"},{"expect":"css:.chip-popover","inViewport":true},{"click":{"css":".chip-popover .menu-item","hasText":"TEST-default-host"}},{"click":"css:section[aria-labelledby=\"defaults-sessions\"] button[aria-label^=\"Model \"]"},{"click":{"css":".picker-fold","hasText":"Codex"}},{"click":"role:button:Fake Deep"},{"press":"Escape"},{"click":{"css":"section[aria-labelledby=\"defaults-sessions\"] .btn-primary","hasText":"Save defaults"}},{"expect":"css:section[aria-labelledby=\"defaults-sessions\"] [role=status]","text":"Defaults saved."},{"reload":true},{"expect":"css:section[aria-labelledby=\"defaults-sessions\"] .chip","text":"TEST-default-host"},{"expect":"css:section[aria-labelledby=\"defaults-sessions\"] button[aria-label^=\"Model \"]","text":"Fake Deep"},{"expect":"css:section[aria-labelledby=\"defaults-switchy\"]","count":0},{"shot":"saved-defaults"},{"goto":"new"}]'
B='[{"expect":"role:button:TEST-default-host"},{"reload":true}]'
C='[{"expect":"role:button:TEST-default-host"},{"shot":"session-defaults"}]'
yv drive settings --viewport desktop --steps "$(jq -nc --argjson a "$A" --argjson b "$B" --argjson c "$C" --argjson m "$M_DESKTOP" '$a + $m + $b + $m + $c')"
yv drive settings --viewport mobile --steps "$(jq -nc --argjson a "$A" --argjson b "$B" --argjson c "$C" --argjson m "$M_MOBILE" '$a + $m + $b + $m + $c')"
```

### effort-menu-stays-on-screen
Viewports: both
The effort menu opens fully on screen in the defaults row. (On a 390 px
phone the effort chip comes last in the row, and its menu hung from the chip's
left edge to x ≈ 393-398, past the screen's right edge, cutting off the menu's
right side.)

```sh
yv drive settings --steps '[{"click":"css:section[aria-labelledby=\"defaults-sessions\"] button[aria-label^=\"Effort \"]"},{"expect":"css:.effort-menu"},{"shot":"sessions-effort-menu"},{"eval":"(() => { const r = document.querySelector(\".effort-menu\").getBoundingClientRect(); return r.left >= 0 && r.right <= innerWidth; })()","equals":true}]'
```

### reset-defaults
Viewports: both
Explicit defaults can be cleared.

```sh
yv drive settings --steps '[{"eval":"localStorage.setItem(\"yard.defaults.sessions\",JSON.stringify({host:\"local\",choice:{engine:\"codex\",model:\"fake-fast\",effort:\"low\"}})); true"},{"reload":true},{"click":{"css":"section[aria-labelledby=defaults-sessions] button","hasText":"Use last used"}},{"eval":"localStorage.getItem(\"yard.defaults.sessions\")","equals":null},{"shot":"reset-defaults"}]'
```

### removed-default-host-keeps-the-model
Viewports: both
The saved default host was removed: New session falls back to this computer
but still starts on the saved default model and effort, as Settings shows.

```sh
yv api '{"op":"host.add","name":"TEST-removed-default","kind":"ssh","destination":"test@gone.invalid"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-removed-default"}]}' --save removed_host='hosts[name=TEST-removed-default].id'
yv api '{"op":"host.remove","hostId":"{{removed_host}}"}'
PICKS='[{"expect":"role:button:Model Fake Deep"},{"expect":"role:button:Effort Xhigh"}]'
PRE='[{"eval":"localStorage.setItem(\"yard.defaults.sessions\",JSON.stringify({host:\"{{removed_host}}\",choice:{engine:\"codex\",model:\"fake-deep\",effort:\"xhigh\"}})); true"},{"reload":true},{"expect":{"css":".new-chips button","hasText":"iso-host"}}]'
POST='[{"shot":"new-session"},{"goto":"settings"},{"expect":"css:section[aria-labelledby=\"defaults-sessions\"] .chip","text":"iso-host"},{"expect":"css:section[aria-labelledby=\"defaults-sessions\"] button[aria-label^=\"Model \"]","text":"Fake Deep"},{"shot":"settings"}]'
yv drive new --viewport desktop --steps "$(jq -nc --argjson pre "$PRE" --argjson picks "$PICKS" --argjson post "$POST" '$pre + $picks + $post')"
yv drive new --viewport mobile --steps "$(jq -nc --argjson pre "$PRE" --argjson picks "$PICKS" --argjson post "$POST" '$pre + [{"click":"role:button:Session setup"}] + $picks + [{"press":"Escape"}] + $post')"
```

### offline-host-skips-branch-loading
Viewports: both
A saved offline host and folder stay selected. Opening the composer waits for
the host before looking up branches and keeps the task draft available.

```sh
yv api '{"op":"host.add","name":"TEST-offline-default","kind":"ssh","destination":"test@offline.invalid"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-offline-default","status":"error"}]}' --save offline_default='hosts[name=TEST-offline-default].id'
yv drive new --steps '[{"eval":"localStorage.setItem(\"yard.host\",\"{{offline_default}}\"); localStorage.setItem(\"yard.cwd.{{offline_default}}\",\"/tmp/TEST-remembered-repo\"); true"},{"reload":true},{"expect":"role:button:TEST-offline-default"},{"expect":"role:button:TEST-remembered-repo"},{"fill":"label:Message","value":"TEST-draft for offline host"},{"expect":"role:button:Send","enabled":false},{"wait":350},{"expect":"css:.banner-error","count":0},{"shot":"new-session-offline"}]'
```

## Gotchas

- Since #290 every picker menu (host, model, effort) is portaled to `<body>`
  through `AnchoredPopover`, so it is never inside the Settings
  `section[aria-labelledby=…]` whose chip opened it. Select an open menu by
  its own class (`.chip-popover`, `.effort-menu`, `.picker-fold`); only one is
  open at a time. Scoping it to the section made three recipes time out at both
  viewports until this was fixed.
- On a phone, New session keeps model and effort in the `Session setup` chip
  (#290). Recipes that read them there run one drive per viewport and open the
  chip on mobile.
