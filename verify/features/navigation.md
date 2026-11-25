# Navigation

Command-K opens a searchable session and page palette. On macOS, Settings and
Import live in the Switchyard app menu; browsers retain their sidebar rows.

## Setup

```sh
yv iso up
yv iso seed
```

## Cleanup

```sh
yv iso down
```

## Recipes

### command-k-navigation
Viewports: both
Open from the composer, search sessions and pages, select with arrows and Enter,
and dismiss an empty search with Escape while restoring focus.

```sh
yv drive 'session:{{sid_echo}}' --steps '[{"click":"label:Message"},{"fill":"label:Message","value":"TEST-unsent navigation draft"},{"press":"Meta+k"},{"expect":"role:dialog:Go to session or page"},{"expect":"role:combobox:Search sessions and pages"},{"eval":"window.__firstOption = document.querySelector(\"[role=option][aria-selected=true]\").id; true","equals":true},{"press":"ArrowDown"},{"eval":"document.querySelector(\"[role=option][aria-selected=true]\").id !== window.__firstOption","equals":true},{"press":"ArrowUp"},{"press":"Tab"},{"eval":"document.activeElement.getAttribute(\"aria-label\")","equals":"Close"},{"press":"Tab"},{"eval":"document.activeElement.getAttribute(\"role\")","equals":"combobox"},{"press":"Escape"},{"eval":"document.activeElement.getAttribute(\"aria-label\")","equals":"Message"},{"expect":"label:Message","value":"TEST-unsent navigation draft"},{"press":"Meta+k"},{"shot":"palette"},{"fill":"role:combobox:Search sessions and pages","value":"TEST-hello from Claude"},{"expect":"role:option","count":1},{"press":"Enter"},{"expect":"css:.thread","text":"Echo: TEST-hello from Claude"},{"press":"Meta+k"},{"fill":"role:combobox:Search sessions and pages","value":"settings"},{"press":"Enter"},{"expect":"role:heading:Settings"},{"press":"Meta+k"},{"fill":"role:combobox:Search sessions and pages","value":"connectors"},{"press":"Enter"},{"expect":"role:heading:Connectors"},{"press":"Meta+k"},{"fill":"role:combobox:Search sessions and pages","value":"automations"},{"press":"Enter"},{"expect":"role:heading:Automations"},{"press":"Meta+k"},{"fill":"role:combobox:Search sessions and pages","value":"no-such-destination-xyz"},{"expect":"role:status","text":"No sessions or pages match."},{"shot":"empty"},{"press":"Escape"},{"expect":"role:dialog:Go to session or page","visible":false}]'
```

### command-k-finds-agents
Viewports: both
Agents is a sidebar page, so the palette lists it too, and Switchy's old name
still finds it (Switchy's page became the Agents page).

```sh
yv drive new --steps '[{"press":"Meta+k"},{"expect":"role:dialog:Go to session or page"},{"fill":"role:combobox:Search sessions and pages","value":"Switchy"},{"expect":{"css":".navigation-option","hasText":"Agents"}},{"shot":"agents-search"},{"press":"Enter"},{"expect":"role:heading:Agents"}]'
```

### command-k-rows-name-repo-and-branch
Viewports: both
A worktree session's row in the palette names its repository and branch
("Codex · iso-host · TEST-demo-app · yard/edit-…"), not the full worktree path
under yardd's data dir, which cut off before the branch (#212). Its path still
matches a search.

```sh
yv drive new --steps '[{"press":"Meta+k"},{"expect":"role:dialog:Go to session or page"},{"fill":"role:combobox:Search sessions and pages","value":"TEST-fix the greeting"},{"expect":{"css":".navigation-option","hasText":"TEST-fix the greeting"}},{"eval":"/^Codex · iso-host · TEST-demo-app · yard\\/edit-test-fix-the-greeting-[0-9a-f]+$/.test(document.querySelector(\".navigation-option .navigation-detail\").textContent)","equals":true},{"shot":"repo-and-branch"},{"fill":"role:combobox:Search sessions and pages","value":"wt/TEST-demo-app/yard-edit"},{"expect":{"css":".navigation-option","hasText":"TEST-fix the greeting"}}]'
```

### recent-list-skips-automation-runs
Viewports: both
Finished automation runs are archived and live on the Automations page. Nine
of them, newer than every conversation, must not push the sidebar's sessions
out of the empty palette; searching still finds a run.

```sh
yv api '{"op":"automation.save","automation":{"name":"TEST-Nav runs","prompt":"TEST-say hello","cwd":"{{repo}}","engine":"codex","model":"fake-fast","effort":"low","permission":"ask","schedule":{"kind":"daily","time":"09:30"}}}' --wait '{"op":"automation.saved"}' --save nav_auto=automation.id
for i in 1 2 3 4 5 6 7 8 9; do yv api '{"op":"automation.run","id":"{{nav_auto}}","force":true}' --wait '{"op":"event","event":{"type":"turn_completed"}}'; done
yv state --expect '{"sessions":[{"title":"~TEST-Nav runs · ","archived":true}]}'
yv drive new --steps '[{"press":"Meta+k"},{"expect":"role:dialog:Go to session or page"},{"expect":{"css":".navigation-option","hasText":"TEST-say hello"}},{"expect":{"css":".navigation-option","hasText":"TEST-hello from Claude"}},{"expect":{"css":".navigation-option","hasText":"TEST-Nav runs"},"visible":false},{"shot":"recent"},{"fill":"role:combobox:Search sessions and pages","value":"TEST-Nav runs"},{"expect":{"css":".navigation-option","hasText":"TEST-Nav runs"}},{"shot":"search-finds-runs"}]'
```

### native-app-menu
Viewports: both
Model the Mac shell capability and its DOM events. The Settings and Import
sidebar rows disappear (Agents, which replaced Switchy, Automations and
Connectors stay; Hosts, Sessions and Subscriptions left the sidebar in #290 and
open from Go to), while Settings, Import and Go to remain accessible through
the menu bridge.

```sh
yv drive new --steps '[{"eval":"window.__SWITCHYARD_APP_MENU__ = true; true","equals":true},{"eval":"window.dispatchEvent(new CustomEvent(\"switchyard:navigate\", {detail: \"settings\"})); true","equals":true},{"expect":"role:heading:Settings"},{"eval":"[...document.querySelectorAll(\".sidebar-top button\")].map(e => e.textContent.trim())","equals":["New session","Agents","Automations","Connectors"]},{"shot":"mac-sidebar"},{"eval":"window.dispatchEvent(new CustomEvent(\"switchyard:navigate\", {detail: \"import\"})); true","equals":true},{"expect":"role:dialog:Import from Claude Code"},{"shot":"menu-import"},{"press":"Escape"},{"eval":"window.dispatchEvent(new CustomEvent(\"switchyard:navigate\", {detail: \"navigation\"})); true","equals":true},{"expect":"role:dialog:Go to session or page"},{"fill":"role:combobox:Search sessions and pages","value":"import"},{"press":"Enter"},{"expect":"role:dialog:Import from Claude Code"}]'
```
