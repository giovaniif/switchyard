# Web version

The same Switchyard as a web page, in one container: yardd, the UI, the compiled Claude sidecar and pinned `codex`, `claude`, `gh` and `git`. Run it on a server and open it from any browser.

> **There is no sign-in.** Anyone who can reach the page can run any command in the container, as you, with your Claude, Codex and GitHub logins. Serve it only on a private network, such as a Tailscale tailnet, a VPN or a LAN you trust. Never put it on a public domain or open port.

## On Coolify

1. New resource > **Docker Compose**: either from the public repository `https://github.com/giovaniif/better-than-claude-code` (it picks up `docker-compose.yml` and can build the image itself), or **Empty Docker Compose** with the contents of [docker-compose.yml](docker-compose.yml) pasted (that pulls `ghcr.io/giovaniif/switchyard:latest`).
2. Set the service's domain to `https://<name>:47717`, for example `https://yard.example.com:47717`. The `:47717` tells Coolify's proxy which container port to route to; the page itself is served on 443.
3. Set `YARD_ALLOW_ORIGIN` to the URL you will open, without the port: `https://yard.example.com`. The WebSocket refuses every other browser origin, so the page shows "disconnected" until this matches. Several URLs can be listed, comma separated.
4. Set `PROJECTS` to the folder on the server that holds your projects, such as `/home/me/code`. It is mounted at the same path in the container, and the folder picker opens on it. Files there must be writable by uid 1000, the container's user.
5. Optional: `YARD_HOST_NAME` (the name the UI shows for the machine, default `switchyard`), `GH_TOKEN` (for the PR pane and `git` over HTTPS) and `CLAUDE_CODE_OAUTH_TOKEN` (from `claude setup-token`, instead of logging in from the app).
6. Deploy and open the URL. The new-session page says when an engine isn't logged in; click **Log in** (or type `/login` in a session). Codex shows a link and a code to enter there; Claude Code shows a sign-in link whose page gives back a code to paste. The logins are saved in the home volume. The Hosts page shows each engine's state too.
7. For the PR pane and pushing: `gh auth login` in a session's terminal, unless `GH_TOKEN` is set, and `git config --global user.name ...` and `user.email ...` for commits.

## With Docker Compose

```bash
YARD_ALLOW_ORIGIN=http://localhost:47717 docker compose up -d
```

`docker-compose.yml` publishes the port on `127.0.0.1` only; reach it from elsewhere through an SSH forward or a private interface, never `0.0.0.0` on a public machine. `docker compose build` builds the image from this checkout instead of pulling it.

## The same server in the desktop app

The compose file publishes container port 47717 on the server's `127.0.0.1`, which is where the desktop app looks for Switchyard on an SSH host. Add the server on the app's Hosts page (for example `me@server`) and it connects to this container: the app and the web page show the same sessions. Without that port the app finds nothing on the server and offers to install a second, separate Switchyard there. If you added the web version before this port existed, add `127.0.0.1:47717:47717` under `ports:` and redeploy.

Update the container by redeploying it; the app's **Update Switchyard** button refuses, since it only manages the Switchyard it installed.

## iPhone app

The iOS app connects to this same private HTTPS origin and uses the existing
WebSocket Origin check. Open **Hosts → Switchyard for iPhone** for setup. It offers
Install with TestFlight for the public beta; `YARD_TESTFLIGHT_URL` replaces that
link (`none` removes it). The phone must have network access to the origin (for example,
through Tailscale); the desktop's SSH forward cannot be used from the phone.
See [mobile/README.md](../mobile/README.md) for the native app and signing setup.

## Projects

The container sees only its own disk. Projects get in one of three ways:

- `PROJECTS`: a folder on the server, bind-mounted at the same path. The usual choice, and what the folder picker lists first.
- The `/workspace` volume, when `PROJECTS` is unset: `git clone` into it from a session's terminal.
- Anywhere under `/home/yard`, which is also a volume.

Worktrees and sessions live in `/home/yard/.yard`. Both volumes survive redeploys; deleting them loses the logins and sessions.

## Notes

- The image is built by `.github/workflows/docker.yml` on every push to `main` (`latest` and the short commit). A new GitHub package is private: make it public in the package settings, or log Coolify in to `ghcr.io`, before pasting the compose file.
- The engine versions are build arguments: `docker build --build-arg CODEX_VERSION=... --build-arg CLAUDE_VERSION=... .`. They are pinned on purpose, since `codex app-server` is experimental.
- In the container yardd listens on `0.0.0.0` (`YARD_BIND`). Everywhere else it stays on loopback.
- The container runs as its own user, `yard`: its terminals, `gh` and git see only what you log in or mount there. To use a server as your own user from a browser, see "Also in a browser" in [remote-hosts.md](remote-hosts.md).
- Codex runs with no sandbox of its own in the container (`YARD_CODEX_SANDBOX=danger-full-access`): its Linux sandbox needs namespaces that an unprivileged container does not get, so every command would fail. The container is the boundary, and Codex still asks before what it thinks needs approval. Set `YARD_CODEX_SANDBOX=workspace-write` to go back to the desktop default, for example on a container you run with relaxed seccomp and AppArmor.
