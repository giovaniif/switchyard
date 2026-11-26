# Automations

Scheduled work: an automation is a prompt, a folder on a host, an engine,
model, effort and permission, a base branch and a schedule in the host's local
time. Each run is an ordinary session in its own worktree with `automation`
set in its metadata. Runs stay out of the sidebar; a finished run archives
itself; the Automations page is a grid of small cards (side by side on a wide
window, stacked below 700 px), each with the schedule in words and the next
run, an enabled switch, one status line (a working run, else how the last one
went), a strip of marks for the recent runs (each opens its session), and
Run now, Runs (every run, with a pin) and Edit. Pinning a run brings it back
to the sidebar. Scheduled
runs skip while the previous one is still going and, when set, while the
engine's usage is above a limit.

## Setup

```sh
yv iso up
yv iso seed
yv api '{"op":"automation.save","automation":{"name":"TEST-Patrol","prompt":"TEST-say hello","cwd":"{{repo}}","engine":"codex","model":"fake-fast","effort":"low","permission":"ask","schedule":{"kind":"daily","time":"09:30"}}}' --wait '{"op":"automation.saved"}' --save auto_id=automation.id
```

## Cleanup

```sh
yv iso down
```

## Recipes

### list-shows-schedule-next-and-last
The seeded automation is listed with its schedule in words, a next run, "No runs yet" as its status, and the enabled switch on. At 390 px Automations is in the sidebar drawer, and the heading is on screen.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"expect":"role:heading:Automations"},{"expect":{"css":".automation","hasText":"TEST-Patrol"}},{"expect":{"css":".automation","hasText":"Daily at 09:30 · next "}},{"expect":{"css":".automation-status","hasText":"No runs yet"}},{"expect":"role:switch:Enabled: TEST-Patrol"},{"shot":"list"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"expect":"role:heading:Automations","inViewport":true},{"expect":{"css":".automation","hasText":"Daily at 09:30 · next "}},{"expect":{"css":".automation-status","hasText":"No runs yet"}},{"shot":"list"}]'
```

### create-from-the-form
Fill the form and save: the new automation appears in the list and in the daemon. Each viewport creates its own.

Viewports: both, one automation each.

```sh
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"click":"role:button:New automation"},{"fill":"label:Automation name","value":"TEST-mobile one"},{"fill":"label:Prompt","value":"TEST-mobile prompt"},{"click":{"css":".chip","hasText":"work"}},{"fill":"css:.menu-search","value":"{{repo}}"},{"press":"Enter"},{"fill":"label:Time","value":"10:15"},{"shot":"form"},{"click":"role:button:Save automation"},{"expect":{"css":".automation","hasText":"TEST-mobile one"}},{"expect":{"css":".automation","hasText":"Daily at 10:15"}}]'
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:New automation"},{"fill":"label:Automation name","value":"TEST-desktop one"},{"fill":"label:Prompt","value":"TEST-desktop prompt"},{"click":{"css":".chip","hasText":"work"}},{"fill":"css:.menu-search","value":"{{repo}}"},{"press":"Enter"},{"fill":"label:Time","value":"10:15"},{"shot":"form"},{"click":"role:button:Save automation"},{"expect":{"css":".automation","hasText":"TEST-desktop one"}},{"expect":{"css":".automation","hasText":"Daily at 10:15"}}]'
yv api '{"op":"automation.list"}' --wait '{"op":"automations"}' --expect '{"op":"automations","automations":[{"name":"TEST-mobile one"},{"name":"TEST-desktop one"}]}'
```

### run-now-archives-and-lists-the-run
Run now starts a session; when it ends it is archived, so it is not in the sidebar, but it is a mark in its card's run strip (named by its time, not the automation's name again, with "Done" in its tooltip), the status line reads "Last:", and both the mark and the Runs list open it.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:Run now: TEST-Patrol"},{"expect":"css:.automation[aria-label=\"TEST-Patrol\"] .run-mark"},{"expect":"css:.automation[aria-label=\"TEST-Patrol\"] .run-mark[title$=\" · Done\"]","timeout":90000},{"expect":{"css":".automation[aria-label=\"TEST-Patrol\"] .automation-status","hasText":"Last: "}},{"eval":"document.querySelector(\".automation[aria-label=\\\"TEST-Patrol\\\"] .run-mark\").title.replace(/^\\d\\d:\\d\\d · \\d+s · Done$/, \"time · duration · Done\")","equals":"time · duration · Done"},{"expect":{"css":".session-row","hasText":"TEST-Patrol ·"},"visible":false},{"shot":"runs"}]'
yv api '{"op":"automation.list"}' --wait '{"op":"automations"}' --expect '{"op":"automations","automations":[{"id":"{{auto_id}}","lastRun":{"outcome":"started"}}]}' --save run_sid='automations[id={{auto_id}}].lastRun.sessionId'
yv state --expect '{"sessions":[{"id":"{{run_sid}}","title":"~TEST-Patrol · ","archived":true,"status":"idle"}]}'
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"css:.automation[aria-label=\"TEST-Patrol\"] .run-mark"},{"expect":"css:.thread","text":"Echo: TEST-say hello"}]'
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:Runs: TEST-Patrol"},{"expect":{"css":".automation-run","hasText":"Done"}},{"expect":{"css":".automation-run","hasText":"TEST-Patrol"},"visible":false},{"click":"css:.automation-run"},{"expect":"css:.thread","text":"Echo: TEST-say hello"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"expect":"css:.automation[aria-label=\"TEST-Patrol\"] .run-mark"},{"expect":{"css":".automation[aria-label=\"TEST-Patrol\"] .automation-status","hasText":"Last: "}},{"click":"role:button:Runs: TEST-Patrol"},{"expect":{"css":".automation-run","hasText":"Done"}},{"shot":"runs"}]'
```

### pinning-a-run-brings-it-back
Pinning a run from its card's Runs list puts it under Pinned in the sidebar, and its pin then says it is pinned.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:Runs: TEST-Patrol"},{"click":"css:.automation[aria-label=\"TEST-Patrol\"] .automation-pin"},{"expect":"css:.automation-pin[aria-label$=\" is pinned\"]"},{"expect":"css:.list-head","text":"Pinned"},{"expect":{"css":".session-row","hasText":"TEST-Patrol ·"}},{"shot":"pinned"}]'
yv state --expect '{"sessions":[{"id":"{{run_sid}}","pinned":true}]}'
```

### run-time-is-how-long-it-ran
Viewports: both
A run's time on the Automations page is how long it ran, not how long since
it last changed: moving the run into a group a few seconds later (a pin, a
rename, a reply also count) leaves the fake's under-a-second run at 0-2 s, in
the Runs list, in each strip mark's tooltip and on the "Last:" line.

```sh
yv drive new --viewport desktop --steps '[{"wait":4000}]'
yv api '{"op":"session.update","sessionId":"{{run_sid}}","group":"TEST-later"}' --wait '{"op":"session","session":{"id":"{{run_sid}}","group":"TEST-later"}}'
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:Runs: TEST-Patrol"},{"expect":"css:.automation-run"},{"eval":"[...document.querySelectorAll(\".automation-run-time\")].map((e) => e.textContent).concat([...document.querySelectorAll(\".run-mark\")].map((e) => e.title.split(\" · \")[1])).filter((t) => !/^[0-2]s$/.test(t)).join(\", \") || \"all short\"","equals":"all short"},{"expect":{"css":".automation[aria-label=\"TEST-Patrol\"] .automation-status","hasText":"Last: "}},{"eval":"/^Last: [0-2]s · /.test(document.querySelector(\".automation[aria-label=\\\"TEST-Patrol\\\"] .automation-status\").textContent)","equals":true},{"shot":"run-times"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"click":"role:button:Runs: TEST-Patrol"},{"expect":"css:.automation-run"},{"eval":"[...document.querySelectorAll(\".automation-run-time\")].map((e) => e.textContent).concat([...document.querySelectorAll(\".run-mark\")].map((e) => e.title.split(\" · \")[1])).filter((t) => !/^[0-2]s$/.test(t)).join(\", \") || \"all short\"","equals":"all short"},{"shot":"run-times"}]'
```

### grid-of-cards-and-the-run-strip
Viewports: both
The page is a grid of small cards: on a wide window (1440 px) two cards sit
side by side on the first row; at 390 px they stack in one column, each as
wide as the page. A card holds no run titles (a run is named by its time).
The newest mark is the last one in the strip and opens that run.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"expect":{"css":".automation","hasText":"TEST-desktop one"}},{"eval":"(() => { const [a, b] = [...document.querySelectorAll(\".automation\")].map((e) => e.getBoundingClientRect()); return a.top === b.top && b.left > a.right && a.width >= 280 && a.width < 600 })()","equals":true},{"expect":{"css":".automation","hasText":"TEST-Patrol ·"},"visible":false},{"shot":"grid"},{"click":"css:.automation[aria-label=\"TEST-Patrol\"] .run-mark:last-child"},{"expect":"css:.thread","text":"Echo: TEST-say hello"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"expect":{"css":".automation","hasText":"TEST-desktop one"}},{"eval":"(() => { const [a, b] = [...document.querySelectorAll(\".automation\")].map((e) => e.getBoundingClientRect()); return b.top >= a.bottom && a.left === b.left && a.width > 300 && a.right <= window.innerWidth })()","equals":true},{"shot":"grid"},{"click":"css:.automation[aria-label=\"TEST-Patrol\"] .run-mark:last-child"},{"expect":"css:.thread","text":"Echo: TEST-say hello"}]'
```

### edit-changes-the-schedule
Edit opens the form with the saved values; saving a new time updates the words in the list.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:Edit: TEST-Patrol"},{"expect":{"css":"input","value":"TEST-Patrol"}},{"fill":"label:Time","value":"07:45"},{"click":"role:button:Save automation"},{"expect":{"css":".automation","hasText":"Daily at 07:45"}}]'
yv api '{"op":"automation.list"}' --wait '{"op":"automations"}' --expect '{"op":"automations","automations":[{"id":"{{auto_id}}","schedule":{"kind":"daily","time":"07:45"}}]}'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"click":"role:button:Edit: TEST-Patrol"},{"shot":"edit"}]'
```

### edit-keeps-an-unlisted-model
Edit an automation whose saved model is not in the host's list (retired or renamed): the form keeps its engine and model, named by id, and saving a new name does not switch it to another engine. Both viewports, one save each.

```sh
yv api '{"op":"automation.save","automation":{"name":"TEST-Retired model","prompt":"TEST-retired","cwd":"{{repo}}","engine":"codex","model":"gpt-retired","effort":"low","permission":"ask","schedule":{"kind":"daily","time":"09:30"}}}' --wait '{"op":"automation.saved"}' --save retired_id=automation.id
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:Edit: TEST-Retired model"},{"wait":1000},{"expect":{"css":".automation-pickers","hasText":"gpt-retired"}},{"fill":"label:Automation name","value":"TEST-Retired desktop"},{"shot":"edit"},{"click":"role:button:Save automation"},{"expect":{"css":".automation","hasText":"TEST-Retired desktop"}}]'
yv api '{"op":"automation.list"}' --wait '{"op":"automations"}' --expect '{"op":"automations","automations":[{"id":"{{retired_id}}","name":"TEST-Retired desktop","engine":"codex","model":"gpt-retired","effort":"low"}]}'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"click":"role:button:Edit: TEST-Retired desktop"},{"wait":1000},{"expect":{"css":".automation-pickers","hasText":"gpt-retired"}},{"fill":"label:Automation name","value":"TEST-Retired mobile"},{"shot":"edit"},{"click":"role:button:Save automation"},{"expect":{"css":".automation","hasText":"TEST-Retired mobile"}}]'
yv api '{"op":"automation.list"}' --wait '{"op":"automations"}' --expect '{"op":"automations","automations":[{"id":"{{retired_id}}","name":"TEST-Retired mobile","engine":"codex","model":"gpt-retired"}]}'
```

### disable-stops-the-schedule
Turning the switch off shows "off" as the next run and stores `enabled: false`; turning it back on schedules again.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:switch:Enabled: TEST-Patrol"},{"expect":{"css":".automation[aria-label=\"TEST-Patrol\"] .automation-when","hasText":" · off"}}]'
yv api '{"op":"automation.list"}' --wait '{"op":"automations"}' --expect '{"op":"automations","automations":[{"id":"{{auto_id}}","enabled":false,"nextRun":null}]}'
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:switch:Enabled: TEST-Patrol"},{"expect":{"css":".automation-when","hasText":" · off"},"visible":false},{"expect":{"css":".automation[aria-label=\"TEST-Patrol\"] .automation-when","hasText":" · next "}}]'
```

### reload-and-back-keep-the-page
A reload keeps the Automations page, and the Edit form with its saved values (not a blank "New automation" that would save a copy); Back from a run opened there returns to the page.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"expect":"role:heading:Automations"},{"reload":true},{"expect":"role:heading:Automations"},{"click":"role:button:Edit: TEST-Patrol"},{"reload":true},{"expect":"role:heading:Edit automation"},{"expect":{"css":"input","value":"TEST-Patrol"}},{"click":"role:button:Cancel"},{"click":"role:button:Run now: TEST-Patrol"},{"expect":"css:.automation[aria-label=\"TEST-Patrol\"] .run-mark[title$=\" · Done\"]","count":2,"timeout":90000},{"click":"css:.automation[aria-label=\"TEST-Patrol\"] button.automation-status"},{"expect":"css:.thread","text":"Echo: TEST-say hello"},{"eval":"history.back()"},{"expect":"role:heading:Automations"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"reload":true},{"expect":{"css":".automation","hasText":"TEST-Patrol"}},{"click":"role:button:Edit: TEST-Patrol"},{"reload":true},{"expect":{"css":"input","value":"TEST-Patrol"}},{"shot":"edit-after-reload"}]'
```

### delete-removes-the-automation-and-frees-its-runs
Deleting asks once; the automation goes and its runs become ordinary (archived) sessions.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:Edit: TEST-Patrol"},{"click":"role:button:Delete automation"},{"click":"role:button:Confirm delete"},{"expect":{"css":".automation","hasText":"TEST-Patrol"},"visible":false}]'
yv api '{"op":"automation.list"}' --wait '{"op":"automations"}' --expect '{"op":"automations","automations":[{"name":"TEST-mobile one"},{"name":"TEST-desktop one"}]}'
```

### claude-usage-limit-skips
A scheduled-style run (not Run now) of a Claude automation whose 5-hour limit is below Claude's usage (47% after seed) skips with the reason, like a Codex one does. Daemon only.

```sh
yv api '{"op":"automation.save","automation":{"name":"TEST-Claude capped","prompt":"TEST-capped","cwd":"{{repo}}","engine":"claude","model":"claude-haiku-5-5","effort":"low","permission":"ask","enabled":false,"maxFiveHour":10,"schedule":{"kind":"daily","time":"09:30"}}}' --wait '{"op":"automation.saved"}' --save claude_capped=automation.id
yv api '{"op":"automation.run","id":"{{claude_capped}}","force":false}' --wait '{"op":"automation.ran"}' --expect '{"op":"automation.ran","outcome":"skipped","reason":"~Claude 5-hour usage 47% is above 10%"}'
```

### run-now-failure-says-why-once
Run now on an automation whose folder has gone: the card's status line says "Failed" and gives the reason once, not a second time as "Failed: …" under it. Both viewports, one Run now each.

```sh
mkdir -p "$(v work)/TEST-moved"
yv api '{"op":"automation.save","automation":{"name":"TEST-Moved","prompt":"TEST-moved","cwd":"{{work}}/TEST-moved","engine":"codex","model":"fake-fast","effort":"low","permission":"ask","enabled":false,"schedule":{"kind":"daily","time":"09:30"}}}' --wait '{"op":"automation.saved"}'
rmdir "$(v work)/TEST-moved"
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:Run now: TEST-Moved"},{"expect":{"css":".automation[aria-label=\"TEST-Moved\"] .automation-status","hasText":"Failed · "}},{"wait":1000},{"expect":{"css":".automation-reason","hasText":"TEST-moved is not a folder"},"count":1},{"shot":"failed"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"click":"role:button:Run now: TEST-Moved"},{"wait":1000},{"expect":{"css":".automation[aria-label=\"TEST-Moved\"] .automation-status","hasText":"Failed · "}},{"expect":{"css":".automation-reason","hasText":"TEST-moved is not a folder"},"count":1},{"shot":"failed"}]'
```

### interval-and-weekly-schedules-from-the-form
The form's other two schedule types: "Every few minutes or hours" shows Repeat every and its unit (no Time), "Weekly" shows the day and a Time. The hint under the schedule reads the schedule in words before saving, the card reads the same after, and the daemon stores `interval` (every 2, hours) and `weekly` (day 5, 08:00). A 5-hour usage limit typed in the form is stored as `maxFiveHour`. Each viewport creates one (interval at 390 px, weekly at 1440 px).

Viewports: both, one automation each.

```sh
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"click":"role:button:New automation"},{"fill":"label:Automation name","value":"TEST-every two hours"},{"fill":"label:Prompt","value":"TEST-interval prompt"},{"click":{"css":".chip","hasText":"work"}},{"fill":"css:.menu-search","value":"{{repo}}"},{"press":"Enter"},{"click":"role:button:Schedule type"},{"click":"role:menuitemradio:Every few minutes or hours"},{"expect":"label:Time","visible":false},{"fill":"label:Repeat every","value":"2"},{"click":"role:button:Repeat unit"},{"click":"role:menuitemradio:hours"},{"expect":{"css":".automation-field","hasText":"Every 2 hours, in"}},{"fill":"label:5-hour usage limit percent","value":"80"},{"shot":"interval-form"},{"click":"role:button:Save automation"},{"expect":{"css":".automation","hasText":"TEST-every two hours"}},{"expect":{"css":".automation","hasText":"Every 2 hours"}},{"shot":"interval-list"}]'
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:New automation"},{"fill":"label:Automation name","value":"TEST-weekly friday"},{"fill":"label:Prompt","value":"TEST-weekly prompt"},{"click":{"css":".chip","hasText":"work"}},{"fill":"css:.menu-search","value":"{{repo}}"},{"press":"Enter"},{"click":"role:button:Schedule type"},{"click":"role:menuitemradio:Weekly"},{"expect":"label:Repeat every","visible":false},{"click":"role:button:Day of the week"},{"click":"role:menuitemradio:Friday"},{"fill":"label:Time","value":"08:00"},{"expect":{"css":".automation-field","hasText":"Fridays at 08:00, in"}},{"shot":"weekly-form"},{"click":"role:button:Save automation"},{"expect":{"css":".automation","hasText":"TEST-weekly friday"}},{"expect":{"css":".automation","hasText":"Fridays at 08:00"}},{"shot":"weekly-list"}]'
yv api '{"op":"automation.list"}' --wait '{"op":"automations"}' --expect '{"op":"automations","automations":[{"name":"TEST-every two hours","schedule":{"kind":"interval","every":2,"unit":"hours"},"maxFiveHour":80},{"name":"TEST-weekly friday","schedule":{"kind":"weekly","day":5,"time":"08:00"}}]}'
```

### a-working-run-dots-the-automations-button
While a run is working, the sidebar's Automations button carries a dot named "A run is working" (the run itself stays out of the sidebar), and its card leads with "Running · <elapsed>" behind a turning mark that opens the run, its strip mark saying "Running"; Stop ends the run and the dot goes. At 390 px the button is in the drawer.

```sh
yv api '{"op":"automation.save","automation":{"name":"TEST-Long run","prompt":"[slow] TEST-long run","cwd":"{{repo}}","engine":"codex","model":"fake-fast","effort":"low","permission":"ask","enabled":false,"schedule":{"kind":"daily","time":"09:30"}}}' --wait '{"op":"automation.saved"}' --save long_id=automation.id
yv api '{"op":"automation.run","id":"{{long_id}}","force":true}' --wait '{"op":"automation.ran"}' --expect '{"op":"automation.ran","outcome":"started"}' --save long_sid=sessionId
yv state --expect '{"sessions":[{"id":"{{long_sid}}","status":"running"}]}'
yv drive new --viewport desktop --steps '[{"expect":{"css":".nav-row","hasText":"Automations"}},{"expect":"role:img:A run is working"},{"expect":{"css":".session-row","hasText":"TEST-Long run ·"},"visible":false},{"shot":"working"},{"click":"role:button:Automations"},{"expect":{"css":".automation[aria-label=\"TEST-Long run\"] .automation-status.status-live","hasText":"Running · "}},{"expect":"css:.automation[aria-label=\"TEST-Long run\"] .automation-status .mark-working"},{"expect":"css:.automation[aria-label=\"TEST-Long run\"] .run-mark[title$=\" · Running\"]"},{"shot":"working-card"},{"click":"css:.automation[aria-label=\"TEST-Long run\"] button.automation-status"},{"expect":"css:.thread","text":"TEST-long run"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":"role:img:A run is working"},{"shot":"working"},{"click":"role:button:Automations"},{"expect":{"css":".automation[aria-label=\"TEST-Long run\"] .automation-status","hasText":"Running · "}},{"shot":"working-card"}]'
yv api '{"op":"session.interrupt","sessionId":"{{long_sid}}"}'
yv state --expect '{"sessions":[{"id":"{{long_sid}}","status":"idle"}]}'
yv drive new --viewport desktop --steps '[{"expect":{"css":".nav-row","hasText":"Automations"}},{"expect":"role:img:A run is working","visible":false}]'
```

### manual-run-with-its-own-prompt
A manual automation never fires on its own: no next run, and the card's schedule line reads "Manual · runs when started". `automation.run` with a `prompt` and a `title` starts a run titled `<name> <title> · <date>` that runs that prompt (not the saved one), shows on its card's run strip and in Runs labelled with its time and tag (`09:28 #4242`), and stays out of the sidebar. This is how `verify/yv loop run verify --pr N` starts a verifier (`Loop verify #N · …`). The form shows the schedule as "Manual: only when started", with no Time.

```sh
yv api '{"op":"automation.save","automation":{"name":"TEST-Loop verify","prompt":"TEST-saved prompt","cwd":"{{repo}}","engine":"codex","model":"fake-fast","effort":"low","permission":"full","base":"main","skipWhileRunning":false,"schedule":{"kind":"manual"}}}' --wait '{"op":"automation.saved"}' --expect '{"op":"automation.saved","automation":{"schedule":{"kind":"manual"},"nextRun":null,"enabled":true}}' --save manual_id=automation.id
yv api '{"op":"automation.run","id":"{{manual_id}}","force":true,"prompt":"TEST-verify pull request 4242","title":"#4242"}' --wait '{"op":"automation.ran"}' --expect '{"op":"automation.ran","outcome":"started"}' --save manual_sid=sessionId
yv state --expect '{"sessions":[{"id":"{{manual_sid}}","title":"~TEST-Loop verify #4242 · "}]}'
yv drive new --viewport desktop --steps '[{"expect":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":{"css":".session-row","hasText":"TEST-Loop verify"},"visible":false},{"click":"role:button:Automations"},{"expect":{"css":".automation[aria-label=\"TEST-Loop verify\"] .automation-when","hasText":"Manual · runs when started"}},{"expect":"css:.automation[aria-label=\"TEST-Loop verify\"] .run-mark[title*=\" #4242 · \"]","timeout":90000},{"shot":"manual-card"},{"click":"role:button:Runs: TEST-Loop verify"},{"expect":{"css":".automation-run","hasText":"#4242"}},{"shot":"manual-runs"},{"click":{"css":".automation-run","hasText":"#4242"}},{"expect":"css:.thread","text":"Echo: TEST-verify pull request 4242"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":{"css":".session-row","hasText":"TEST-Loop verify"},"visible":false},{"click":"role:button:Automations"},{"expect":{"css":".automation[aria-label=\"TEST-Loop verify\"] .automation-when","hasText":"Manual \u00b7 runs when started"}},{"click":"role:button:Runs: TEST-Loop verify"},{"expect":{"css":".automation-run","hasText":"#4242"}},{"shot":"manual-runs"},{"click":"role:button:Edit: TEST-Loop verify"},{"expect":"role:button:Schedule type","text":"Manual: only when started"},{"expect":"label:Time","visible":false},{"shot":"manual-edit"}]'
```

### time-fields-fit-hh-mm
Viewports: both
The HH:MM fields (Window start, Window end, and a daily or weekly Time) are
sized for five characters, like Repeat every, so "between 21:00 and 09:00"
reads on one row at 390 px. (#401: they took a text input's default width,
about 260 px on a phone, and 09:00 wrapped onto a line of its own.)

```sh
yv api '{"op":"automation.save","automation":{"name":"TEST-Window fit","prompt":"TEST-window fit","cwd":"{{repo}}","engine":"codex","model":"fake-fast","effort":"low","enabled":false,"schedule":{"kind":"interval","every":1,"unit":"hours"},"window":{"start":"21:00","end":"09:00"}}}' --wait '{"op":"automation.saved"}'
yv api '{"op":"automation.save","automation":{"name":"TEST-Daily fit","prompt":"TEST-daily fit","cwd":"{{repo}}","engine":"codex","model":"fake-fast","effort":"low","enabled":false,"schedule":{"kind":"daily","time":"09:00"}}}' --wait '{"op":"automation.saved"}'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"click":"role:button:Edit: TEST-Window fit"},{"expect":"label:Window end","value":"09:00"},{"eval":"(() => { const s = document.querySelector(\"[aria-label=\u0027Window start\u0027]\").getBoundingClientRect(), e = document.querySelector(\"[aria-label=\u0027Window end\u0027]\").getBoundingClientRect(), n = document.querySelector(\"[aria-label=\u0027Repeat every\u0027]\").getBoundingClientRect(); return Math.abs(s.top - e.top) < 2 && s.width <= n.width + 0.5 && e.width <= n.width + 0.5 && e.right <= innerWidth; })()","equals":true},{"shot":"window-row"},{"click":"role:button:Cancel"},{"click":"role:button:Edit: TEST-Daily fit"},{"expect":"label:Time","value":"09:00"},{"eval":"document.querySelector(\"[aria-label=\u0027Time\u0027]\").getBoundingClientRect().width <= 80.5","equals":true},{"shot":"daily-time"}]'
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:Edit: TEST-Window fit"},{"expect":"label:Window end","value":"09:00"},{"eval":"(() => { const s = document.querySelector(\"[aria-label=\u0027Window start\u0027]\").getBoundingClientRect(), e = document.querySelector(\"[aria-label=\u0027Window end\u0027]\").getBoundingClientRect(), n = document.querySelector(\"[aria-label=\u0027Repeat every\u0027]\").getBoundingClientRect(); return Math.abs(s.top - e.top) < 2 && s.width <= n.width + 0.5 && e.width <= n.width + 0.5 && e.right <= innerWidth; })()","equals":true},{"shot":"window-row"},{"click":"role:button:Cancel"},{"click":"role:button:Edit: TEST-Daily fit"},{"expect":"label:Time","value":"09:00"},{"eval":"document.querySelector(\"[aria-label=\u0027Time\u0027]\").getBoundingClientRect().width <= 80.5","equals":true},{"shot":"daily-time"}]'
```

### interval-active-window
An interval can be restricted to overnight hours and start days. Editing keeps
the saved hours and days, invalid equal boundaries disable Save, the card names
the window, and enabled toggles preserve it. Clearing with null removes it.
Viewports: both.

```sh
yv api '{"op":"automation.save","automation":{"name":"TEST-Overnight","prompt":"TEST-overnight","cwd":"{{repo}}","engine":"codex","model":"fake-fast","effort":"low","enabled":false,"schedule":{"kind":"interval","every":1,"unit":"hours"},"window":{"start":"21:00","end":"09:00"}}}' --wait '{"op":"automation.saved"}' --expect '{"automation":{"window":{"start":"21:00","end":"09:00"},"nextRun":null}}' --save overnight_id=automation.id
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"click":"role:button:Edit: TEST-Overnight"},{"expect":"label:Window start","value":"21:00"},{"expect":"label:Window end","value":"09:00"},{"click":"role:button:Window starts on Monday"},{"click":"role:button:Window starts on Tuesday"},{"click":"role:button:Window starts on Wednesday"},{"click":"role:button:Window starts on Thursday"},{"click":"role:button:Window starts on Friday"},{"shot":"window-form"},{"click":"role:button:Save automation"},{"expect":{"css":".automation[aria-label=\"TEST-Overnight\"] .automation-when","hasText":"Every hour · 21:00–09:00, Mon–Fri · off"}},{"shot":"window-card"}]'
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:Edit: TEST-Overnight"},{"expect":"label:Window start","value":"21:00"},{"expect":"label:Window end","value":"09:00"},{"expect":"css:button[aria-label=\"Window starts on Friday\"][aria-pressed=\"true\"]"},{"fill":"label:Window end","value":"21:00"},{"expect":"role:alert","text":"The window start and end must differ."},{"expect":"role:button:Save automation","enabled":false},{"shot":"window-invalid"},{"fill":"label:Window end","value":"09:00"},{"shot":"window-form"},{"click":"role:button:Save automation"},{"click":"role:switch:Enabled: TEST-Overnight"},{"expect":{"css":".automation[aria-label=\"TEST-Overnight\"] .automation-when","hasText":"Every hour · 21:00–09:00, Mon–Fri · next "}},{"shot":"window-card"}]'
yv api '{"op":"automation.list"}' --wait '{"op":"automations"}' --expect '{"automations":[{"id":"{{overnight_id}}","enabled":true,"window":{"start":"21:00","end":"09:00","days":[1,2,3,4,5]}}]}'
yv api '{"op":"automation.save","automation":{"id":"{{overnight_id}}","window":null}}' --wait '{"op":"automation.saved"}' --expect '{"automation":{"window":null}}'
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"expect":{"css":".automation[aria-label=\"TEST-Overnight\"] .automation-when","hasText":"21:00–09:00"},"visible":false}]'
```

### a-run-waiting-for-approval-marks-the-button-amber
A run that stops for approval puts an amber dot named "A run needs approval" on the sidebar's Automations button, not the green "A run is working" one: runs stay out of the sidebar, so this dot is the only sign there that a run waits for the user. Stopping the run clears it (the fake Codex ignores the interrupt while it asks, so yardd ends the turn after its grace period: wait for the dot to go). At 390 px the button is in the drawer.

```sh
yv api '{"op":"automation.save","automation":{"name":"TEST-Asks first","prompt":"[cmd] TEST-asks first","cwd":"{{repo}}","engine":"codex","model":"fake-fast","effort":"low","permission":"ask","schedule":{"kind":"manual"}}}' --wait '{"op":"automation.saved"}' --save ask_id=automation.id
yv api '{"op":"automation.run","id":"{{ask_id}}","force":true}' --wait '{"op":"automation.ran"}' --expect '{"op":"automation.ran","outcome":"started"}' --save ask_sid=sessionId
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"expect":{"css":".automation[aria-label=\"TEST-Asks first\"] .automation-status","hasText":"Needs approval"},"timeout":30000},{"expect":"role:img:A run needs approval"},{"expect":"css:.nav-row .dot.dot-needs_approval"},{"expect":"role:img:A run is working","visible":false},{"shot":"approval"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":"role:img:A run needs approval"},{"expect":"role:img:A run is working","visible":false},{"shot":"approval"}]'
yv api '{"op":"session.interrupt","sessionId":"{{ask_sid}}"}'
yv drive new --viewport desktop --steps '[{"expect":{"css":".nav-row","hasText":"Automations"}},{"expect":"role:img:A run needs approval","visible":false,"timeout":20000},{"expect":"role:img:A run is working","visible":false}]'
yv state --expect '{"sessions":[{"id":"{{ask_sid}}","status":"idle"}]}'
```

### next-run-stays-in-view-with-a-window
A card's schedule line ends with when it runs next, the part that changes. An
interval with a window and start days makes the line longer than a card, so
the schedule words are what gets cut: "next …" stays whole inside the card at
1440 px (cards about 355 px wide) and at 390 px, and the line's tooltip holds
all of it. Viewports: both.

```sh
yv api '{"op":"automation.save","automation":{"name":"TEST-Night shift","prompt":"TEST-night shift","cwd":"{{repo}}","engine":"codex","model":"fake-fast","effort":"low","enabled":true,"schedule":{"kind":"interval","every":5,"unit":"minutes"},"window":{"start":"21:00","end":"09:00","days":[1,3,5]}}}' --wait '{"op":"automation.saved"}' --expect '{"automation":{"enabled":true,"window":{"start":"21:00","end":"09:00","days":[1,3,5]}}}'
yv drive new --viewport desktop --steps '[{"click":"role:button:Automations"},{"expect":{"css":".automation[aria-label=\"TEST-Night shift\"] .automation-when","hasText":"Every 5 minutes · 21:00–09:00, Mon, Wed, Fri · next "}},{"eval":"(() => { const el = document.querySelector(\".automation[aria-label=\\\"TEST-Night shift\\\"] .automation-when\"); const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT); let n; while ((n = w.nextNode())) { const i = n.data.indexOf(\"next \"); if (i >= 0) { const r = document.createRange(); r.setStart(n, i); r.setEnd(n, n.data.length); return r.getBoundingClientRect().right <= el.getBoundingClientRect().right + 1 ? \"next run in view\" : \"next run cut off\"; } } return \"no next run\"; })()","equals":"next run in view"},{"eval":"/^Every 5 minutes · 21:00–09:00, Mon, Wed, Fri · next .+/.test(document.querySelector(\".automation[aria-label=\\\"TEST-Night shift\\\"] .automation-when\").title)","equals":true},{"shot":"night-shift-card"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"expect":{"css":".automation[aria-label=\"TEST-Night shift\"] .automation-when","hasText":"Every 5 minutes · 21:00–09:00, Mon, Wed, Fri · next "}},{"eval":"(() => { const el = document.querySelector(\".automation[aria-label=\\\"TEST-Night shift\\\"] .automation-when\"); const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT); let n; while ((n = w.nextNode())) { const i = n.data.indexOf(\"next \"); if (i >= 0) { const r = document.createRange(); r.setStart(n, i); r.setEnd(n, n.data.length); return r.getBoundingClientRect().right <= el.getBoundingClientRect().right + 1 ? \"next run in view\" : \"next run cut off\"; } } return \"no next run\"; })()","equals":"next run in view"},{"shot":"night-shift-card"}]'
```

## Gotchas
- Recipes share one isolated yardd and run in order: later recipes use the automation and run the earlier ones made.
- A drive at both viewports runs on the same data, so the create recipe uses one drive per viewport.
- `disable-stops-the-schedule` expects no card to read "· off" after its schedule once TEST-Patrol is back on, so a recipe before it must leave its own automations enabled (a daily slot will not fire during a run); recipes after it may add disabled ones.
