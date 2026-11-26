# Worktrees

Every new session in a git repository with a commit gets its own branch
(`yard/<words>-<hex>`) and worktree under `<data>/wt/<repo>/`. A folder outside
a repository runs in place, with a note. One writer per folder: a second
in-place session is refused while another runs there. Removing a worktree is
never forced: uncommitted or untracked files keep it.

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

### session-gets-worktree
Seeded sessions run on their own branches under the data dir.

```sh
yv state --expect '{"sessions":[{"id":"{{sid_echo}}","branch":"~yard/","cwd":"~/data/wt/TEST-demo-app/"}]}'
```

### plain-folder-in-place
A folder that is not a repository runs in place and says so.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","worktree":true,"text":"TEST-in place"}' --wait '{"op":"created"}' --save place_sid=sessionId
yv state --expect '{"sessions":[{"id":"{{place_sid}}","cwd":"{{plain}}"}]}'
yv drive 'session:{{place_sid}}' --viewport desktop --steps '[{"expect":"css:.thread","text":"Not a git repository with commits"}]'
```

### one-writer-per-folder
A second in-place session in a folder with a running one is refused.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"[slow] TEST-writer one"}' --wait '{"op":"created"}' --save w1_sid=sessionId
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"TEST-writer two"}' --wait '{"op":"error"}' --expect '{"op":"error","for":"session.create"}'
yv api '{"op":"session.interrupt","sessionId":"{{w1_sid}}"}'
```

### delete-removes-clean-worktree
"Delete and remove worktree" on a clean session removes the folder.

```sh
yv state --save think_cwd='sessions[id={{sid_think}}].cwd'
yv drive 'session:{{sid_think}}' --viewport desktop --steps '[{"click":{"role":"button","name":"Delete","exact":true}},{"click":"role:button:Delete and remove worktree"},{"expect":"role:heading:What should we work on?"}]'
sleep 1
test ! -d "$(v think_cwd)"
```

### sidebar-delete-refusal-names-the-session
Viewports: both
Deleting the edit session (uncommitted change) from the sidebar, with another
session open, is refused, and the banner over the open session names the one
it refused. (Before: "The worktree has uncommitted…", read as the open
session's.) On a phone the refusal closes the drawer, so the banner is not
left behind its scrim (#483; before, only "Dismiss" peeked out and the drawer
said nothing). The refusal is an API error, hence `--allow-errors`.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --allow-errors --steps '[{"click":"role:button:Actions for [edit] TEST-fix the greeting"},{"click":"role:menuitem:Delete…"},{"click":"role:menuitem:Delete and remove the worktree"},{"expect":"css:.banner-error","text":"\"[edit] TEST-fix the greeting\" was not deleted: the worktree has uncommitted"},{"shot":"refused"}]'
yv drive 'session:{{sid_echo}}' --viewport mobile --allow-errors --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Actions for [edit] TEST-fix the greeting"},{"click":"role:menuitem:Delete…"},{"click":"role:menuitem:Delete and remove the worktree"},{"expect":"css:.banner-error","text":"\"[edit] TEST-fix the greeting\" was not deleted: the worktree has uncommitted"},{"expect":"role:button:Close sessions","visible":false},{"shot":"refused"},{"click":"role:button:Dismiss"},{"expect":"css:.banner-error","visible":false}]'
yv state --expect '{"sessions":[{"id":"{{sid_edit}}"}]}'
```

### dirty-worktree-is-kept
Removing the edit session's worktree (it has an uncommitted change) is refused.

```sh
yv api '{"op":"session.delete","sessionId":"{{sid_edit}}","removeWorktree":true}' --wait '{"op":"error"}' --expect '{"op":"error","for":"session.delete"}'
yv state --expect '{"sessions":[{"id":"{{sid_edit}}"}]}'
```

## Gotchas
- Files a setup script creates count as untracked unless gitignored, so they block removal.
- `plain-folder-in-place` failed on main (#309): the session page polls
  `pr.get` every 60 s, and for a folder that is not a repository yardd
  answered `{ "pr": { "error": "not a git repository" } }`, which `yv drive`
  counts as a failed API call. Nothing showed (the chip renders only a PR with
  a number). yardd now answers `{ "pr": { "repo": false } }`, and the recipe
  passes.
- Deleting a session on desktop showed a "no such session" banner (#307):
  closing its terminals told the open terminal pane to reload a list yardd
  could no longer give. `delete-removes-clean-worktree` catches it as a failed
  `term.list` call. Fixed by #308; the recipe passes on 2026-11-20.
