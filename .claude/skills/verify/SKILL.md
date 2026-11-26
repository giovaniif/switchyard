---
name: verify
description: Prove a Switchyard change works in the running app. Starts an isolated yardd with scripted engines, drives the UI in headless Chromium and the daemon over its WebSocket, runs the feature map, and records an evidence bundle (base repro, head fix at both viewports, failing test first, design check). Use before claiming any user-visible change works, before opening a PR, and when asked "does X work".
---

# Verify a Switchyard change

Tests alone are not verification. `verify/yv` is the one way agents touch the
running app: every command prints one JSON object (`ok`, `problems` with
`hint`s), exit 0 ok, 1 failed, 2 usage. Read `problems[].hint` before guessing.
Full help: `verify/yv help`.

## Ground rules

- Never use the live yardd on port 47717 (the user's sessions). `yv` only
  drives isolated daemons it started (`iso`), on free ports, with a temp data
  dir, a temp HOME, the scripted engines (`crates/yardd/tests/fake-codex`), a
  fake `gh` (one canned PR) and a fake `ssh` (always unreachable). Your session
  inherits the live daemon's `YARD_*` variables; `yv` scrubs them.
- Data is synthetic: repos, sessions and PRs are named `TEST-…`.
- Real engines only with `iso up --engines real`, and then only Codex
  `gpt-5.6-luna` effort `low` (yv refuses other turns without `--allow-spend`).
  No Claude turns.
- Kill only what you started: `yv iso down` (or `--all` for this checkout's).

## Start the app

```sh
verify/yv doctor            # tools, Chromium, build freshness, load, checkout vs origin/main
verify/yv iso up            # builds yardd + ui/dist if stale, starts yardd; --name n for a second one
verify/yv iso seed          # TEST-demo-app repo + one session per scripted behaviour
verify/yv state             # sessions as the daemon sees them; seeded ids are variables
```

Seeded variables (use as `{{name}}` in any yv argument, `$(v name)` in map
shell blocks, `verify/yv var get name` elsewhere): `repo`, `plain`, `work`,
`fake`, `port`, `sid_echo` (plain reply), `sid_edit` (edit + a real diff +
PR #4242 from the fake gh), `sid_think` (thinking), `sid_fail` (engine error),
`sid_claude` (scripted Claude sidecar), `sid_cmd` (waiting for approval).

Prompt markers for the fake Codex: `[cmd]` approval, `[edit]` file change,
`[slow]` runs until Stop, `[stuck]` ignores Stop, `[fail]` error, `[think]`
reasoning; anything else echoes `Echo: <prompt>`. The fake Claude sidecar
knows `[die]`, `[stuck]`, `[steps]` and `[subagent]` (a Task call with a nested
subagent, and a `sleep 30` left running under the sidecar). `iso up --logged-out` starts both engines logged
out; `--pressure` makes the queue hold new sessions. `[chart]` on either engine calls the `chart` MCP server (`verify/fakes/mcp-chart`) and shows its widget; the Connectors page (route `connectors`) lists the fakes' MCP servers (see `verify/features/connectors.md`).

A dev server that does not watch files serves stale code: after changing
`crates/` or `ui/`, `verify/yv iso restart` (rebuilds when stale). Every shot
and drive records the build it ran against (`build.sha`, `build.dirty`).
A restart ends every turn in flight, like a user's yardd restart: the seeded
`sid_cmd` comes back idle ("yardd restarted during this turn."), not waiting
for approval. A proof that needs a live approval or a running turn after a
restart starts a new one first (`api session.send` with `[cmd]` or `[slow]`,
`--wait` for `approval_request`) and checks it with `state --expect`, at base
and at head alike. Otherwise the base "repro" shows a quiet session and proves nothing.

## Drive it

```sh
verify/yv shot new                       # both viewports: mobile 390x844, desktop 1440x900, 2x
verify/yv shot 'session:{{sid_edit}}' --viewport desktop
verify/yv drive 'session:{{sid_cmd}}' --steps '[{"click":"role:button:Allow once"},{"expect":"css:.thread","text":"Ran it."},{"shot":"after"}]'
verify/yv api '{"op":"session.send","sessionId":"{{sid_echo}}","text":"hi"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'
verify/yv state --expect '{"sessions":[{"id":"{{sid_echo}}","status":"idle"}]}'
```

- Routes: `new`, `hosts`, `settings`, `connectors`, `session:<id or title text>`.
- At mobile (below 700 px) the sidebar is a closed drawer. Routes open it
  themselves; steps that click or read something in it open it first with
  `{"click":"role:button:Open sessions"}` (hidden on desktop, so a recipe
  that needs it runs one drive per viewport).
- Steps: `goto`, `click`, `hover`, `tap` (mobile only), `fill`+`value`, `type` (xterm), `press`, `drag`+`to`
  (HTML5 drag and drop; `hold` ms waits over the target before releasing,
  `during` names a screenshot taken while held there), `wait`
  (ms or target), `expect` (+`text`, `value`, `enabled`, `count`, `visible:false`,
  `inViewport`), `upload`+`files`, `shot`, `aria` (accessibility snapshot),
  `eval`(+`equals`), `reload`. Targets: `role:button:Name`, `label:Message`,
  `text:…`, `placeholder:…`, `testid:…`, `css:.x`, or `{"css":".session-row","hasText":"…","nth":0,"exact":true}`.
  The first *visible* match is used.
- Every run collects console errors, page errors, failed API calls (yardd
  `error` replies) and HTTP failures; any of them fails the run unless you pass
  `--allow-errors` (do that only when the error is the thing you are proving).
- Each `drive` (and each viewport in it) starts a fresh browser profile:
  localStorage set in one drive is gone in the next. Seed browser state with
  an `eval` step and check its effect in the same drive.
- On a failed step you get a screenshot, an aria snapshot and the reason.
  `--video` records the drive (mp4 with ffmpeg).
- Learn the UI's names with `{"aria":true}` before guessing selectors.

## The feature map

`verify/features/<area>.md` holds what "working" means, as runnable recipes.
`verify/yv map status` lists areas and last results; `verify/yv map run <area>`
runs one area in its own isolated yardd; `map run all` runs everything (~10 min).
Known failing recipes are product bugs listed under each area's Gotchas.
Rules: never weaken a recipe to make it pass. Map drift: fix the recipe.
Product bug: keep it failing, add a Gotcha, queue a finding. A bug you fix
gets a recipe with its repro.

## The proof bar (evidence bundle)

For any change you claim fixes something:

```sh
D=.verify/evidence/<slug>
verify/yv evidence init $D --claim "What the user sees, why it is wrong" --head <fix sha>
# base: the merge-base, or the commit with only the new test
git checkout <base sha> && verify/yv iso restart       # or iso up
verify/yv shot <route> --allow-errors --out $D-base    # or drive with the repro steps
verify/yv evidence add $D --ref base --kind repro --file $D-base/<label>.json
verify/yv evidence add $D --ref base --kind test --cmd "<the new test command>"   # must fail
git checkout <fix sha> && verify/yv iso restart
verify/yv drive <route> --steps '…' --out $D-head      # both viewports for UI changes
verify/yv evidence add $D --ref head --kind fix --file $D-head/drive.json
verify/yv evidence add $D --ref head --kind test --cmd "CARGO_BUILD_JOBS=4 cargo test -p yardd && (cd ui && bun test)"
verify/yv evidence add $D --ref head --kind design                 # UI changes: runs design-check
verify/yv evidence add $D --ref head --kind regression --area <area>   # optional: map run
verify/yv evidence verify $D
```

`evidence verify` passes only with: a base repro (or a `no-repro: <why>`
note), a head fix (both viewports when `ui/` changed), no failed API call only
on head (unless the fix is that refusal: one `--kind note --text "expected-api:
<op>: <why>"` per op, e.g. `term.open`, which the verifier judges), a passing
head test run, a test that failed on base (or a
`no-failing-test: <why>` note), a design check for UI changes (deletions and
renames count), no recipe failing only on head, unchanged artifact hashes, and
the checkout still at the recorded head. `--cmd` commands are run by yv, so
exit codes are observed, not reported. Head proofs need a clean tree.

**Failing test first:** commit the regression test alone, show it fails for
the bug's reason (`--ref base --kind test --cmd …` at that commit), then
commit the fix. A test that passes before the fix proves nothing. A feature
map recipe that reproduces the bug counts as the test:
`--cmd "verify/yv map run <area> --recipe <label>"`.

Publish for the PR: `verify/yv evidence publish $D` (pushes the bundle and
images to the `verify-assets` branch, returns `bundleUrl` and a Markdown
Verification section). Never commit screenshots to the PR branch
(`verify/yv assets check`).

## UI changes: design validation

Before writing code: read `ui/src/styles.css` (colour tokens in `:root`,
`.btn`, `.btn-text`, `.btn-icon`, `.btn-primary`, `.btn-quiet`, the font-size
scale) and find the screen that already solves the same pattern. Use its
components and tokens; no hardcoded colours, no magic numbers, no raw HTML
(agent output is untrusted: React elements only). After: `verify/yv
design-check` clean on added lines; screenshot the reference screen next to
your after-shots; cover empty, loading, error and long-text states at both
viewports. `verify/yv annotate --before a.png --after b.png --box x,y,w,h`
makes before/after crops for the PR body.

## Cleanup

`verify/yv iso down --all`. Isolated daemons live in `/tmp/yv-*`; evidence and
shots in `.verify/` (gitignored). Delete any TEST- data you made outside them.

## When something is off

- `iso-not-running` / `iso-dead`: `verify/yv iso up` (or `down` then `up`).
- `build-stale`: `verify/yv build`. `doctor` says why.
- `needs-var`: run the recipe's Setup or `iso seed`; `verify/yv var ls`.
- A step times out: add `{"aria":true}` before it and read the names.
- `yv selftest` runs the CLI's own tests; `--smoke` adds a real round trip.
