# Import

Claude Code sessions run outside Switchyard (in a terminal, an IDE) can be
imported: "Import" in the sidebar lists the host's Claude Code sessions (the
Agent SDK's `listSessions`, newest first) and imports one as a Switchyard
session in its own folder, in place, with its history; the next message
resumes the same Claude Code conversation. In isolated environments the fake
sidecar offers three TEST- sessions: two in the iso's work folder, one whose
folder no longer exists.

## Setup

```sh
yv iso up
yv iso seed --profile empty
```

## Cleanup

```sh
yv iso down --name map-import-remote
yv iso down
```

## Recipes

### dialog-lists-sessions
Expect at both viewports: the Import dialog with the three TEST- sessions,
newest first, each with its folder, and the dialog inside the viewport. At
390 px Import is in the sidebar drawer.

```sh
yv drive new --viewport desktop --steps '[{"click":{"role":"button","name":"Import","exact":true}},{"expect":"role:dialog:Import from Claude Code","inViewport":true},{"expect":"role:dialog","text":"TEST-fix the login redirect"},{"expect":"role:dialog","text":"TEST-write the release notes"},{"expect":"role:dialog","text":"TEST-old refactor of the parser"},{"shot":"import-dialog"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":{"role":"button","name":"Import","exact":true}},{"expect":"role:dialog:Import from Claude Code","inViewport":true},{"expect":"role:dialog","text":"TEST-fix the login redirect"},{"expect":"role:dialog","text":"TEST-write the release notes"},{"expect":"role:dialog","text":"TEST-old refactor of the parser"},{"shot":"import-dialog"}]'
```

### filter-narrows-the-list
Typing in the filter keeps only matching sessions.

```sh
yv drive new --viewport desktop --steps '[{"click":{"role":"button","name":"Import","exact":true}},{"fill":"label:Filter sessions","value":"parser"},{"expect":"role:dialog","text":"TEST-old refactor of the parser"},{"expect":"role:button:Import TEST-fix the login redirect","visible":false}]'
```

### recently-active-warns
Viewports: desktop
A session used in the last 10 minutes may still be open in Claude Code: its
row warns before importing (two writers would interleave one conversation).

```sh
yv drive new --viewport desktop --steps '[{"click":{"role":"button","name":"Import","exact":true}},{"expect":"role:dialog","text":"Used in the last 10 minutes"},{"expect":{"css":".import-row","hasText":"TEST-fix the login redirect"},"text":"close it there first"},{"expect":{"css":".import-row","hasText":"TEST-old refactor of the parser"},"text":"3 d ago"}]'
```

### age-stays-visible
Each row's second line ends with when the session was last used ("3 d ago"),
and it shows in full at both viewports: a long folder or branch shrinks with
an ellipsis instead. At 390 px the folder used to push the age out of sight.

```sh
yv drive new --viewport desktop --steps '[{"click":{"role":"button","name":"Import","exact":true}},{"expect":"role:dialog","text":"TEST-old refactor of the parser"},{"eval":"[...document.querySelectorAll(\".import-meta\")].map((m) => { const w = document.createTreeWalker(m, NodeFilter.SHOW_TEXT); let n, last = null; while ((n = w.nextNode())) if (n.textContent.trim()) last = n; const r = document.createRange(); r.setStart(last, last.length - 1); r.setEnd(last, last.length); return last.textContent.trim().endsWith(\"ago\") && r.getBoundingClientRect().right <= m.getBoundingClientRect().right + 1; }).join(\",\")","equals":"true,true,true"},{"shot":"import-ages"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":{"role":"button","name":"Import","exact":true}},{"expect":"role:dialog","text":"TEST-old refactor of the parser"},{"eval":"[...document.querySelectorAll(\".import-meta\")].map((m) => { const w = document.createTreeWalker(m, NodeFilter.SHOW_TEXT); let n, last = null; while ((n = w.nextNode())) if (n.textContent.trim()) last = n; const r = document.createRange(); r.setStart(last, last.length - 1); r.setEnd(last, last.length); return last.textContent.trim().endsWith(\"ago\") && r.getBoundingClientRect().right <= m.getBoundingClientRect().right + 1; }).join(\",\")","equals":"true,true,true"},{"shot":"import-ages"}]'
```

### import-opens-the-session
Viewports: desktop
Importing opens a Claude session in that folder with its history and a note;
listing again offers "Open" instead.

```sh
yv drive new --viewport desktop --steps '[{"click":{"role":"button","name":"Import","exact":true}},{"click":"role:button:Import TEST-fix the login redirect"},{"expect":"role:heading:TEST-fix the login redirect"},{"expect":"css:.thread","text":"TEST-added login.test.ts."},{"expect":"css:.thread","text":"Imported from Claude Code"},{"shot":"imported"},{"click":{"role":"button","name":"Import","exact":true}},{"expect":"role:button:Open TEST-fix the login redirect"}]'
yv state --expect '{"sessions":[{"title":"TEST-fix the login redirect","engine":"claude","cwd":"{{work}}"}]}'
```

### import-on-mobile
Viewports: mobile
At 390 px the dialog's Import button is on screen and importing opens the
session and closes the sidebar drawer.

```sh
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":{"role":"button","name":"Import","exact":true}},{"expect":"role:button:Import TEST-old refactor of the parser","inViewport":true},{"click":"role:button:Import TEST-old refactor of the parser"},{"expect":"css:.thread","text":"TEST-answer to: TEST-old refactor of the parser"},{"expect":"role:navigation:Sessions","visible":false},{"shot":"imported-mobile"}]'
```

### next-message-resumes
The first message after an import resumes the same Claude Code session.

```sh
yv state --save imp_sid='sessions[title=TEST-old refactor of the parser].id' || {
  yv api '{"op":"session.import","claudeSessionId":"33333333-3333-4333-8333-333333333333"}' --wait '{"op":"created"}' --save imp_sid=sessionId
}
yv api '{"op":"session.send","sessionId":"{{imp_sid}}","text":"TEST-carry on"}' --wait '{"op":"event","sessionId":"{{imp_sid}}","event":{"type":"turn_completed"}}'
grep -q "resume=33333333-3333-4333-8333-333333333333" "$(v fake)/sidecar.log"
```

### missing-folder-is-refused
Viewports: desktop
A session whose folder is gone cannot be imported; the dialog says why.

```sh
yv drive new --viewport desktop --allow-errors --steps '[{"click":{"role":"button","name":"Import","exact":true}},{"click":"role:button:Import TEST-write the release notes"},{"expect":"role:dialog","text":"no longer exists"}]'
```

### import-from-a-remote-host
Viewports: desktop
With a remote host connected, the dialog's host picker lists its Claude Code
sessions and imports one there; the session runs on the remote.

```sh
yv iso down --name map-import-remote >/dev/null
P=$(yv iso up --name map-import-remote --host-name TEST-remote | jq -er .iso.port)
yv api '{"op":"host.add","name":"TEST-remote","kind":"direct","url":"ws://127.0.0.1:'"$P"'/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-remote","status":"connected"}]}' --save rhost='hosts[name=TEST-remote].id'
yv api '{"op":"claude.sessions","host":"{{rhost}}"}' --wait '{"op":"claude.sessions","hostId":"{{rhost}}"}' --expect '{"sessions":[{"title":"TEST-write the release notes"}]}'
yv drive new --viewport desktop --steps '[{"click":{"role":"button","name":"Import","exact":true}},{"click":{"within":"role:dialog","css":".chip"}},{"click":{"within":"role:dialog:Hosts","role":"button","name":"TEST-remote"}},{"expect":"role:dialog","text":"on TEST-remote"},{"click":"role:button:Import TEST-old refactor of the parser"},{"expect":"css:.thread","text":"TEST-answer to: TEST-old refactor of the parser"}]'
yv state --iso map-import-remote --expect '{"sessions":[{"title":"TEST-old refactor of the parser","engine":"claude","cwd":"~yv-map-import-remote"}]}'
yv iso down --name map-import-remote
```

## Gotchas
- Imported sessions run in place (no worktree): Claude Code keeps a session's transcript by folder, and resuming needs the same folder.
- The map and the daemon suite use the fake sidecar, which only logs the resume id it was given. A real resume was checked by hand on 2026-11-15: a throwaway `claude -p --model claude-haiku-5-5` session in a temp folder told to remember PELICAN, imported into a `--engines real` isolated yardd (it kept Haiku 5.5), answered "PELICAN" to "What word did I ask you to remember?". Repeat that by hand after changing the resume path; never import the owner's real sessions to test.
- `claude -p` and SDK sessions are not listed (the SDK's `includeProgrammatic: false`, like Claude Code's own `/resume`); `session.import` still takes their id.
- Titles come from the SDK: the custom title (`/rename`), else the summary, else the first prompt.
- The dialog's host chip opens its menu as a separate `Hosts` popover (portalled, #297), not inside the Import dialog: target its rows `within` `role:dialog:Hosts`.
