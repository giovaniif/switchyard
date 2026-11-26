# Switchyard agent guide

One UI for Claude Code, Codex, OpenCode and Grok sessions. Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before changing anything: it holds the decisions and what is already built.

- `crates/yardd`: Rust daemon. Owns agent processes, the event log, terminals, host stats. Serves the UI and a WebSocket.
- `sidecar/yard-claude`: Bun/TypeScript bridge to the Claude Agent SDK, one process per Claude session.
- `ui`: React + Vite. Talks only to yardd over the WebSocket.
- `app/src-tauri`: Tauri window that starts yardd and loads its page.

## Build and run

- iOS/TestFlight: read [docs/IOS_BUILDS.md](docs/IOS_BUILDS.md) before building or declaring credentials missing. The server's Expo/EAS account holds reusable iOS signing credentials; `~/signing.p12` is for the desktop app. Never print or commit secret values.

- Rust is not on `PATH` on the Mac: `export PATH=/opt/homebrew/opt/rustup/bin:$PATH` before any `cargo`. The Linux server (where the agent loops run) has it on `PATH`.
- `CARGO_BUILD_JOBS=2 cargo build -p yardd`. Keep the job cap: the machine has 8 cores, 16 GB and other work running.
- UI: `cd ui && bun install && bunx vite build`. Sidecar: `cd sidecar/yard-claude && bun install`.
- Use Bun. `pnpm` is broken here. Never run `tsc` in any form.
- Test daemon, from the repo root: `YARD_PORT=<port> YARD_DATA_DIR=<temp folder> ./target/debug/yardd`. It serves `ui/dist` and finds the sidecar relative to the current directory (`YARD_ROOT` overrides). Without `YARD_DATA_DIR` it would open the user's real database in `~/.yard`.
- `/bin/bash` is 3.2. Put multi-step shell logic in a script file, not a long one-liner.

## Hard rules

- A yardd is usually running on port 47717 with the user's live sessions, started by the desktop app. Never stop it. No `pkill yardd`: kill only the PIDs you started. Never test on port 47717.
- Tests must not write under `~/.yard`. Point any data directory setting at a temp folder.
- Ports 4510-4559 belong to LocalStack. Stay out of them.
- Live agent turns spend the user's subscriptions. Tests and verification use the scripted engines; a real-engine check uses Codex on `gpt-5.6-luna`, effort `low`, with tiny prompts. The agent loops run on Claude (`verify/loops.json`), capped by usage; no other Claude turns unless the task says so.
- You own this repo end to end: commit and push your work, including CI changes. Leave other repositories alone.
- Releases are numbered: write the CHANGELOG.md section, then `scripts/release.sh X.Y.Z` (see CONTRIBUTING.md): it opens the version PR with auto-merge, and its merge publishes. Other pushes to `main` build but don't publish.
- yardd binds loopback by default and always checks the WebSocket `Origin`. Keep both. Leaving loopback is opt-in through `YARD_BIND` (the container image uses `0.0.0.0`; a server behind a proxy lists loopback plus one proxy-facing address), always with `YARD_ALLOW_ORIGIN` naming the page's URL. Never match `Origin` against the `Host` header: that reopens DNS rebinding.

## Code

- Message and event shapes live in `ui/src/protocol.ts` and in yardd. Change both together.
- Both engines are normalized to the same events. Add an event type only when no existing one fits.
- Agent output is untrusted: render it as React elements, never as HTML.
- Comments say why, not what. Match the surrounding style. Add a dependency only when it earns its place.

## Verifying

- Compiling is not proof. Use the verification CLI, `verify/yv` (skill: `.claude/skills/verify/SKILL.md`): `yv iso up && yv iso seed` starts an isolated yardd with scripted engines on a free port, `yv shot`/`yv drive` exercise the UI in headless Chromium at mobile and desktop sizes, `yv api`/`yv state` drive and read the daemon over its WebSocket. Every command prints one JSON object.
- The feature map `verify/features/*.md` says what "working" means, area by area; `verify/yv map run <area>` runs it. Never weaken a recipe to make it pass.
- A fix is proven with an evidence bundle (`verify/yv evidence …`): base repro, the regression test failing first, head fix at both viewports, tests, design check.
- Run the tests: `CARGO_BUILD_JOBS=2 cargo test -p yardd` (daemon suite on a fake Codex, no real turns), `cd ui && bun test`, `cd sidecar/yard-claude && bun test`, `verify/yv selftest`. CI (`.github/workflows/ci.yml`) runs all of them on every PR.
- Say plainly what you did not verify.

## Agent PRs

- Agent fixes go through PRs, not straight to `main`: the patrol loop finds, fixes and proves one issue per run; an independent verifier posts a verdict; the land gate (`verify/yv land`, settings in `verify/land.json`) decides whether it could merge. It is live: a PR it finds landable (verifier PASS or PASS+NOTES at head, green checks, no open threads, nothing irreversible or self-or-CI) merges without the owner.
- Skills: `verify`, `verifier`, `babysit`, `patrol`, `map-maintenance`, `weekly-review` in `.claude/skills/`. The loops run as Switchyard sessions on the live yardd, started by systemd user timers (`verify/loops/README.md`).
- Agents post on GitHub as the owner, so every agent comment carries `<!-- yv:agent -->` and the owner is reached with `verify/yv notify`, which shows an OS notification in his open Switchyard windows (no ntfy). An older yardd that does not know the op makes it exit 1 with that reason; nothing falls back.
- Overview for humans: [docs/VERIFICATION.md](docs/VERIFICATION.md).
