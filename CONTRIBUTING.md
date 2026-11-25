# Contributing

Thanks for looking. Switchyard is small and young, so issues and pull requests are both welcome. For anything bigger than a fix, open an issue first so we can agree on the shape. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) has the decisions and how each part works; read it before changing behaviour.

## Layout

| Path | What it is |
|---|---|
| `crates/yardd` | The Rust daemon. Owns agent processes, the event log (SQLite), terminals, worktrees, host stats and remote hosts. Serves the UI and a WebSocket. |
| `sidecar/yard-claude` | Bun/TypeScript bridge to the Claude Agent SDK, one process per Claude session, JSON lines over stdio. |
| `ui` | React + Vite. Talks only to yardd's WebSocket. |
| `app/src-tauri` | The Tauri window: starts yardd if needed and loads its page. |
| `scripts` | Host installers and the screenshot generator. |

Codex is driven directly from Rust through `codex app-server`. Both engines are normalized to one event list (`ui/src/protocol.ts` and yardd change together).

## Run from source

Needs Rust, Bun and git, plus `codex` and `claude` installed and logged in if you want real sessions.

```bash
(cd sidecar/yard-claude && bun install)
(cd ui && bun install && bunx vite build)
YARD_PORT=47790 YARD_DATA_DIR=/tmp/yard-dev cargo run -p yardd
```

Then open http://127.0.0.1:47790. Without `YARD_DATA_DIR` it uses your real `~/.yard`, and without `YARD_PORT` it takes 47717, where the desktop app's daemon usually runs. For UI work with hot reload, start yardd with `YARD_ALLOW_ORIGIN=http://localhost:5173` as well, then run `cd ui && VITE_YARD_WS=ws://127.0.0.1:47790/ws bunx vite`.

Settings, all optional: `YARD_MIN_FREE_GB` (default 2) is the free memory below which new sessions queue.

## Desktop app

```bash
app/build.sh
open target/release/bundle/macos/Switchyard.app
```

`app/build.sh` builds the UI, compiles the Claude sidecar with `bun build --compile`, builds yardd in release and bundles all three into `Switchyard.app/Contents/Resources/yard`, so the app runs without the repo, Bun or Rust. It caps Cargo at `CARGO_BUILD_JOBS=2` unless you set it.

- Closing the app leaves yardd and its sessions running; `pkill -x yardd` stops them. yardd logs to `~/.yard/yardd.log` when the app starts it.
- If a yardd from another build is already running, the app uses it and says so in the title rather than stopping it.
- `SWITCHYARD_PORT=47728 YARD_DATA_DIR=/tmp/yard-test Switchyard.app/Contents/MacOS/switchyard` runs a second copy next to the real one.
- `cargo run -p switchyard` falls back to the repo's builds of yardd, the UI and the sidecar.

CI builds a signed, notarized DMG on every push to `main` and keeps it in the run's artifacts (`.github/workflows/macos-app.yml`). Signing reads the repository secrets listed in `.github/scripts/macos-sign.sh`; without them the DMG is ad-hoc signed. `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` sign the in-app update package (its public key is in `app/src-tauri/tauri.conf.json`); losing the private key means users must download the DMG again to get a build with a new key.

## Tests

```bash
cargo test -p yardd                       # unit tests and the daemon integration suite
(cd ui && bun install && bun test)        # reducers and attachment helpers
(cd sidecar/yard-claude && bun test)      # the Claude message builder
```

The integration tests start the real yardd on free ports with temporary data folders and drive it over its WebSocket. A scripted `codex app-server` (`crates/yardd/tests/fake-codex`) stands in for Codex, and a fake `ssh` runs the remote installer locally, so nothing needs a login, the network or your subscriptions, and the suite is safe next to a running yardd.

Compiling isn't proof: for anything user-visible, drive the feature through the WebSocket or the UI and check what comes back. `verify/yv` does that for you: `verify/yv iso up && verify/yv iso seed` starts an isolated daemon with scripted engines, `verify/yv shot new` screenshots it, and `verify/yv map run <area>` runs the feature map in `verify/features`. See [docs/VERIFICATION.md](docs/VERIFICATION.md).

## Screenshots

`scripts/screenshots/run.sh` regenerates `docs/screenshots` with scripted engines (`scripts/screenshots/bin`): two throwaway daemons, demo repositories and Playwright, with no account or model calls. Build `target/debug/yardd` first.

## Releasing

1. Add a `## X.Y.Z (date)` section to [CHANGELOG.md](CHANGELOG.md): what changed, for people who use the app. It becomes the release notes, on GitHub and in the app's update window. Committing it is optional.
2. Run `scripts/release.sh X.Y.Z`. That's all.

The script never switches your checkout. It builds the release commit from `origin/main` in a temporary worktree: your changelog section, and every version (yardd, the app, `tauri.conf.json`, `Cargo.lock`) set to `X.Y.Z`. It pushes that as `release/X.Y.Z`, opens the PR `release: X.Y.Z` and turns on auto-merge (squash), so it merges once the required checks pass. `--dry-run` makes the commit and shows it without pushing anything. It refuses a version without a changelog section, one that is already tagged or released, and one not newer than `main`'s. If `main` moves before the checks finish, the protected branch wants the PR updated: use Update branch on the PR.

The merge starts `.github/workflows/release.yml` (on any push to `main` that changes a version file). It checks that the versions agree, that `X.Y.Z` has a changelog section and is not released yet (otherwise it stops with nothing to do), then runs CI on the merged commit, builds the signed DMG, the signed update package (`Switchyard.app.tar.gz` and `latest.json`, which installed apps poll; its `notes` are the changelog section) and both Linux packages, and publishes the release, which creates the tag `vX.Y.Z` on that commit. Then it tags the Docker image `X.Y.Z`. A tag made by the workflow starts no other workflow, which is why all of this lives in one. If a publish fails, use Re-run failed jobs on its run, or Actions → Release → Run workflow on `main`: it publishes the version on `main` if that version has no release, and never publishes one twice.

Asset names never change, so `releases/latest/download/Switchyard.dmg` and the server installer always get the newest release.

## Conventions

- Agent output is untrusted: render it as React elements, never as HTML.
- yardd binds loopback by default and always checks the WebSocket `Origin`. Never match `Origin` against `Host`.
- Add an event type only when no existing one fits. Add a dependency only when it earns its place.
- Comments say why, not what.
