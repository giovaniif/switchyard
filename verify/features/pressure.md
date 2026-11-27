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
yv iso down
```

## Recipes

### quiet-when-calm
Viewports: both
No banner while the host has room, even with a turn running.

```sh
yv drive 'session:{{sid_echo}}' --steps '[{"expect":"css:.thread","text":"Echo:"},{"wait":3500},{"expect":"css:.pressure-banner","count":0},{"shot":"calm"}]'
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
overloaded: its thread says why, under Working, and the turn keeps running.

```sh
yv drive 'session:{{busy}}' --steps '[{"expect":"css:.stall-note","text":"the host is overloaded (load 140)."},{"expect":"css:.working","text":"Working"},{"shot":"stalled"},{"click":"role:button:See Hosts"},{"expect":"role:tab:System","text":"System"}]'
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
yv drive hosts --steps '[{"click":"role:tab:System"},{"expect":{"css":".pressure-episode","hasText":"Load 140 on"}},{"expect":"css:.pressure-now","count":0},{"expect":"css:.pressure-banner","count":0},{"expect":{"css":".pressure-tops","hasText":"8.0 cores, 9.0 GB"}},{"shot":"history"},{"click":{"css":".pressure-session","hasText":"[slow] TEST-busy build"}},{"expect":"css:.thread","text":"[slow] TEST-busy build"}]'
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
yv drive hosts --steps '[{"expect":"css:.pressure-banner","text":"mostly sleep, pid"},{"expect":{"css":".pressure-banner button","hasText":"Open"},"count":0},{"click":"role:tab:System"},{"expect":{"css":".pressure-episode","hasText":"Now"},"text":"not a session"},{"shot":"stray"}]'
printf '{"load":0,"swapUsed":0,"swapTotal":0}' > "$(v readings)"
```

## Gotchas

- Stop shows only while the worst session's turn runs: it is that session's
  own Stop (`session.interrupt`). Memory held by an idle session's processes
  is stopped from its Processes tab.
