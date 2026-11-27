# Pressure

When the host is overloaded (1-minute load above its cores, free memory
under `YARD_MIN_FREE_GB`, or swap at least 90% full) yardd records an
episode: its peaks and the top sessions and stray processes at its worst
sample. While it lasts the sidebar (a line over the page on phones and with
the sidebar collapsed) says so and names the worst contributor, with Open and,
while its turn runs, the session's own Stop. A running turn quiet for 5
minutes meanwhile gets a note in its thread. Closed episodes are listed on
the host's System tab. Nothing is stopped automatically.

The isolated daemon reads its load, swap and per-session or per-process use
from its readings file (var `readings`, see `pressure::Injected`); these
recipes write it. Calm for 1 s closes an episode here (60 s for real) and 2 s
of quiet counts as stalled (5 minutes for real).

## Setup

```sh
yv iso up --env YARD_TEST_PRESSURE_CALM_SECS=1,YARD_TEST_STALL_SECS=2
yv iso seed
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","text":"[slow] TEST-busy build"}' --wait '{"op":"event","event":{"type":"assistant_delta"}}' --save busy=sessionId
```

## Cleanup

```sh
kill "$(v stray)" 2>/dev/null || true
# A recipe that fails stops before its own teardown.
yv iso down --name map-pressure-remote
yv iso down
```

## Recipes

### quiet-when-calm
Viewports: both
No banner while the host has room, even with a turn running.

```sh
yv drive 'session:{{sid_echo}}' --steps '[{"expect":"css:.thread","text":"Echo:"},{"wait":3500},{"expect":"css:.pressure-banner","count":0},{"shot":"calm"}]'
```

### full-swap-on-a-calm-host
Viewports: both
Linux leaves stale pages in swap long after the pressure is over. Swap full
on a host with a low load and memory to spare (the readings of the owner's
own server on a calm day) is no overload: no banner, the host reads Healthy,
and nothing is recorded.

```sh
printf '{"load":1.3,"swapUsed":4276183040,"swapTotal":4294963200}' > "$(v readings)"
yv drive hosts --steps '[{"click":"role:tab:System"},{"expect":"css:.host-system","text":"4.0 of 4.0 GB used"},{"wait":3500},{"expect":"css:.pressure-banner","count":0},{"expect":"css:.pressure-now","count":0},{"expect":"css:.pressure-episode","count":0},{"expect":"css:.host-system","text":"Healthy"},{"shot":"swap-full-calm"}]'
test ! -e "$(v data)/pressure.json"
printf '{"load":0,"swapUsed":0,"swapTotal":0}' > "$(v readings)"
```

### banner-names-the-worst-session
Viewports: both
Load 140 on the host: the banner names the host, the load and the session
using the most CPU, with Open and Stop. Open goes to that conversation.

```sh
printf '{"load":140,"swapUsed":0,"swapTotal":0,"sessions":{"%s":{"cpu":800,"mem":9663676416}}}' "$(v busy)" > "$(v readings)"
yv drive 'session:{{sid_echo}}' --steps '[{"expect":"css:.pressure-banner","text":"iso-host is overloaded · load 140 · mostly [slow] TEST-busy build (8.0 cores, 9.0 GB)"},{"expect":"role:button:Stop [slow] TEST-busy build"},{"shot":"banner"},{"click":"role:button:Open [slow] TEST-busy build"},{"expect":"css:.thread","text":"[slow] TEST-busy build"}]'
```

### stalled-turn-note
Viewports: both
The running turn has said nothing since its first words while the host is
overloaded: its thread says why, below the turn, and the turn keeps running.

```sh
yv drive 'session:{{busy}}' --steps '[{"expect":"css:.stall-note","text":"the host is overloaded (load 140)."},{"shot":"stalled"},{"click":"role:button:See Hosts"},{"expect":"role:tab:System","text":"System"}]'
yv state --expect '{"sessions":[{"id":"{{busy}}","status":"running"}]}'
```

### stop-from-the-banner
Viewports: desktop
Stop in the banner is the session's own Stop: the turn ends (interrupted, not
failed), the Stop goes, Open stays, and the thread's note is gone.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Stop [slow] TEST-busy build"},{"expect":"role:button:Stop [slow] TEST-busy build","count":0},{"expect":"role:button:Open [slow] TEST-busy build"}]'
yv state --expect '{"sessions":[{"id":"{{busy}}","status":"idle"}]}'
yv drive 'session:{{busy}}' --viewport desktop --steps '[{"expect":"css:.thread","text":"[slow] TEST-busy build"},{"wait":3500},{"expect":"css:.stall-note","count":0}]'
```

### history-on-the-system-tab
Viewports: both
Calm again: the banner goes and the episode is listed on the System tab with
its peaks and top use, the session linking to its conversation.

```sh
printf '{"load":0,"swapUsed":0,"swapTotal":0}' > "$(v readings)"
yv drive hosts --steps '[{"click":"role:tab:System"},{"expect":{"css":".pressure-episode","hasText":"Load 140 on"}},{"expect":"css:.pressure-now","count":0},{"expect":"css:.pressure-banner","count":0},{"expect":{"css":".pressure-tops","hasText":"8.0 cores, 9.0 GB"}},{"eval":"(() => { document.querySelector(\".pressure-history\").scrollIntoView({ block: \"start\" }); return true; })()","equals":true},{"shot":"history"},{"click":{"css":".pressure-session","hasText":"[slow] TEST-busy build"}},{"expect":"css:.thread","text":"[slow] TEST-busy build"}]'
```

### a-stray-process-is-named
Viewports: both
The worst user belongs to no session (here a `sleep` the recipe started): the
banner names it with its pid and offers no Open or Stop; the history lists it
as not a session.

```sh
sleep 600 >/dev/null 2>&1 &
yv var set stray "$!"
printf '{"load":140,"swapUsed":0,"swapTotal":0,"procs":{"%s":{"cpu":2400,"mem":4294967296}}}' "$(v stray)" > "$(v readings)"
yv drive hosts --steps '[{"expect":"css:.pressure-banner","text":"mostly sleep, pid"},{"expect":{"css":".pressure-banner button","hasText":"Open"},"count":0},{"click":"role:tab:System"},{"expect":{"css":".pressure-episode","hasText":"Now"},"text":"not a session"},{"eval":"(() => { document.querySelector(\".pressure-history\").scrollIntoView({ block: \"start\" }); return true; })()","equals":true},{"shot":"stray"}]'
printf '{"load":0,"swapUsed":0,"swapTotal":0}' > "$(v readings)"
```

### a-remote-hosts-own-episodes
Viewports: both
Each yardd records its own. A remote host overloaded: the hub's banner names
it and its session, and that host's System tab lists the episode with the
session linking to its conversation through the hub.

```sh
yv var set remote_port "$(yv iso up --name map-pressure-remote --host-name TEST-remote --env YARD_TEST_PRESSURE_CALM_SECS=1 | jq -r .iso.port)"
yv var set remote_readings "$(yv var get readings --iso map-pressure-remote | jq -er .value)"
yv var set remote_work "$(yv var get work --iso map-pressure-remote | jq -er .value)"
yv api '{"op":"host.add","name":"TEST-remote","kind":"direct","url":"ws://127.0.0.1:{{remote_port}}/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-remote","status":"connected"}]}' --save remote_id='hosts[name=TEST-remote].id'
yv api '{"op":"session.create","host":"{{remote_id}}","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{remote_work}}","text":"[slow] TEST-remote hog"}' --wait '{"op":"created"}' --save rsid=sessionId
printf '{"load":99,"swapUsed":0,"swapTotal":0,"sessions":{"%s":{"cpu":600,"mem":5368709120}}}' "$(v rsid | sed 's/.*~//')" > "$(v remote_readings)"
yv drive hosts --steps '[{"expect":"css:.pressure-banner","text":"TEST-remote is overloaded · load 99 · mostly [slow] TEST-remote hog (6.0 cores, 5.0 GB)"},{"click":"role:button:Select TEST-remote"},{"click":"role:tab:System"},{"expect":{"css":".pressure-episode","hasText":"Now"},"text":"[slow] TEST-remote hog"},{"eval":"(() => { document.querySelector(\".pressure-history\").scrollIntoView({ block: \"start\" }); return true; })()","equals":true},{"shot":"remote"},{"click":{"css":".pressure-session","hasText":"[slow] TEST-remote hog"}},{"expect":"css:.thread","text":"[slow] TEST-remote hog"}]'
printf '{"load":0,"swapUsed":0,"swapTotal":0}' > "$(v remote_readings)"
yv api '{"op":"host.remove","hostId":"{{remote_id}}"}' --wait '{"op":"hosts"}'
yv iso down --name map-pressure-remote
```

## Gotchas

- Stop shows only while the worst session's turn runs: it is that session's
  own Stop (`session.interrupt`). Memory held by an idle session's processes
  is stopped from its Processes tab.
