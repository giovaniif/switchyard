# Sidebar

Pinned sessions, then ungrouped ones newest first, then one collapsible
section per group, each row with title, model and status. A row's … menu (or
right-click) marks read or unread, renames, pins, moves to a group, archives or restores, and deletes after one
confirmation; a worktree session also offers to remove its worktree. On a
desktop-width window a row can also be dragged onto a group, the ungrouped
Sessions rows or Pinned; on the phone drawer the menu is the only way. A group
exists only while a session is in it; its header menu renames or ungroups all
of them. Pinned, archived and group live in yardd, so every window agrees;
which groups are collapsed is per browser. The Archived header is pinned to the
bottom of the session area and never scrolls away.

A session starts titled with its task's first line; as soon as it starts yardd
names it from the task, then again after its first and third completed turns
from the conversation (a session never named yet, after any turn), with a
small model (Codex `gpt-5.6-luna`, else Haiku). Scripted environments have naming
off; Setup turns it on with the fake Codex model, which answers a title only when
the conversation has `[name]`. A title typed through Rename is never replaced.

## Setup

```sh
yv iso up --env YARD_SESSION_NAMES=on,YARD_NAMING_CODEX_MODEL=fake-fast
yv iso seed
```

## Cleanup

```sh
yv iso down
```

## Recipes

### rows-show-status
Expect at both viewports: every seeded session has a row; the approval one
says "Needs approval". At 390 px the sidebar is a drawer, opened first.

```sh
yv drive new --viewport desktop --steps '[{"expect":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":{"css":".session-row","hasText":"[cmd] TEST-run the migration"},"text":"Needs approval"},{"expect":{"role":"navigation","name":"Sessions"},"text":"TEST-hello from Claude"},{"shot":"sidebar"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":{"css":".session-row","hasText":"[cmd] TEST-run the migration"},"text":"Needs approval"},{"expect":{"role":"navigation","name":"Sessions"},"text":"TEST-hello from Claude"},{"shot":"sidebar"}]'
```

### rows-mark-what-needs-you
Each row's mark and label say its state: a turning ring while running, an amber
"!" while it waits for approval, a blue dot and "Answered" for a finished turn
not opened yet (bold title), red "Failed". Opening an answered session clears
it, and it stays cleared after a reload; the others stay answered. Which answers
were seen is per browser; turns that ended before it first kept track count as
seen, so the recipe moves that point back to the start.

```sh
yv drive new --viewport desktop --steps '[{"eval":"(()=>{localStorage.setItem(\"yard.seen.since\",\"1\");localStorage.removeItem(\"yard.seen\");return 1})()"},{"reload":true},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"text":"Answered"},{"expect":{"css":".session-row.unread .mark-answered"}},{"expect":{"css":".session-row","hasText":"[cmd] TEST-run the migration"},"text":"Needs approval"},{"expect":{"css":".mark-approval"}},{"expect":{"css":".session-row","hasText":"[fail] TEST-failing turn"},"text":"Failed"},{"shot":"marks"},{"click":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":"css:.thread","text":"TEST-say hello"},{"wait":300},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"text":"Idle"},{"reload":true},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"text":"Idle"},{"expect":{"css":".session-row","hasText":"[edit] TEST-fix the greeting"},"text":"Answered"}]'
yv drive new --viewport mobile --steps '[{"eval":"(()=>{localStorage.setItem(\"yard.seen.since\",\"1\");localStorage.removeItem(\"yard.seen\");return 1})()"},{"reload":true},{"click":"role:button:Open sessions"},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"text":"Answered"},{"expect":{"css":".session-row.unread .mark-answered"}},{"expect":{"css":".session-row","hasText":"[cmd] TEST-run the migration"},"text":"Needs approval"},{"expect":{"css":".mark-approval"}},{"expect":{"css":".session-row","hasText":"[fail] TEST-failing turn"},"text":"Failed"},{"shot":"marks"},{"click":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":"css:.thread","text":"TEST-say hello"},{"wait":300},{"click":"role:button:Open sessions"},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"text":"Idle"},{"reload":true},{"click":"role:button:Open sessions"},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"text":"Idle"},{"expect":{"css":".session-row","hasText":"[edit] TEST-fix the greeting"},"text":"Answered"}]'
```

### mark-chat-unread
Marking a chat unread persists across reloads, including the open chat. Opening
it deliberately or choosing Mark as read clears it.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Mark as unread"},{"expect":{"css":".session-row.unread","hasText":"TEST-say hello"},"text":"Unread"},{"reload":true},{"expect":{"css":".session-row.unread","hasText":"TEST-say hello"},"text":"Unread"},{"shot":"marked-unread"},{"click":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":"css:.thread","text":"Echo: TEST-say hello"},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"text":"Idle"},{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Mark as unread"},{"expect":{"css":".session-row.unread","hasText":"TEST-say hello"},"text":"Unread"},{"reload":true},{"expect":{"css":".session-row.unread","hasText":"TEST-say hello"},"text":"Unread"},{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Mark as read"},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"text":"Idle"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Mark as unread"},{"expect":{"css":".session-row.unread","hasText":"TEST-say hello"},"text":"Unread"},{"reload":true},{"click":"role:button:Open sessions"},{"expect":{"css":".session-row.unread","hasText":"TEST-say hello"},"text":"Unread"},{"shot":"marked-unread"},{"click":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":"css:.thread","text":"Echo: TEST-say hello"},{"click":"role:button:Open sessions"},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"text":"Idle"},{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Mark as unread"},{"expect":{"css":".session-row.unread","hasText":"TEST-say hello"},"text":"Unread"},{"reload":true},{"click":"role:button:Open sessions"},{"expect":{"css":".session-row.unread","hasText":"TEST-say hello"},"text":"Unread"},{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Mark as read"},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"text":"Idle"}]'
```



### rename-session
Viewports: both
The session menu starts with the current name selected. Save updates the row
and conversation, and keeps the renamed session after reload. Cancel keeps
the old name. The seeded names come back at the end: later
recipes find these sessions by them.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Rename"},{"expect":"label:Session name","value":"TEST-say hello"},{"eval":"document.activeElement.value === document.activeElement.value.slice(document.activeElement.selectionStart, document.activeElement.selectionEnd)","equals":true},{"fill":"label:Session name","value":"   "},{"expect":"role:button:Save name","enabled":false},{"fill":"label:Session name","value":"  TEST-Release notes  "},{"shot":"rename-dialog"},{"press":"Enter"},{"expect":"role:button:Actions for TEST-Release notes"},{"expect":"role:heading:TEST-Release notes"},{"expect":"css:.thread","text":"Echo: TEST-say hello"},{"reload":true},{"expect":"role:heading:TEST-Release notes"},{"shot":"renamed-session"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Actions for [think] TEST-plan"},{"click":"role:menuitem:Rename"},{"expect":"role:dialog:Rename session","inViewport":true},{"fill":"label:Session name","value":"TEST-Canceled"},{"click":"role:button:Cancel"},{"expect":"role:button:Actions for [think] TEST-plan"},{"click":"role:button:Actions for [think] TEST-plan"},{"click":"role:menuitem:Rename"},{"fill":"label:Session name","value":"TEST-Plan for release"},{"shot":"rename-dialog"},{"click":"role:button:Save name"},{"expect":"role:button:Actions for TEST-Plan for release"},{"click":{"css":".session-row","hasText":"TEST-Plan for release"}},{"expect":"role:heading:TEST-Plan for release"},{"shot":"renamed-session"}]'
yv api '{"op":"session.rename","sessionId":"{{sid_echo}}","title":"TEST-say hello"}' --wait '{"op":"session","session":{"id":"{{sid_echo}}","title":"TEST-say hello"}}'
yv api '{"op":"session.rename","sessionId":"{{sid_think}}","title":"[think] TEST-plan the refactor"}' --wait '{"op":"session","session":{"id":"{{sid_think}}","title":"[think] TEST-plan the refactor"}}'
```

### pin-and-unpin
Pin moves the row under "Pinned"; the daemon stores it.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Pin"},{"expect":"css:.list-head","text":"Pinned"}]'
yv state --expect '{"sessions":[{"id":"{{sid_echo}}","pinned":true}]}'
yv drive new --viewport desktop --steps '[{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Unpin"},{"expect":"text:Pinned","visible":false}]'
yv state --expect '{"sessions":[{"id":"{{sid_echo}}","pinned":false}]}'
```

### archive-and-restore
Archive moves the row into the collapsed Archived section; Restore brings it back.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Actions for [think] TEST-plan"},{"click":"role:menuitem:Archive"},{"expect":"role:button:Archived"}]'
yv state --expect '{"sessions":[{"id":"{{sid_think}}","archived":true}]}'
yv drive new --viewport desktop --steps '[{"click":"role:button:Archived"},{"click":"role:button:Actions for [think] TEST-plan"},{"click":"role:menuitem:Restore"},{"expect":"role:button:Archived","visible":false}]'
yv state --expect '{"sessions":[{"id":"{{sid_think}}","archived":false}]}'
```

### archiving-the-open-session-keeps-the-list-closed
Viewports: desktop
Archiving the session you are looking at moves its row into Archived and
leaves that list collapsed; it used to spring open on every archive. Going to
an archived session (here by its URL) still opens the list, so its row is in
view. Restore brings it back.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Archive"},{"expect":"role:button:Archived"},{"wait":600},{"expect":{"css":"button.list-toggle[aria-expanded=\"false\"]","hasText":"Archived"}},{"expect":"css:.archived-list","count":0},{"shot":"archived-open-session"},{"eval":"(location.hash = \"#/\", true)","equals":true},{"eval":"(location.hash = \"#/s/{{sid_echo}}\", true)","equals":true},{"expect":{"css":"button.list-toggle[aria-expanded=\"true\"]","hasText":"Archived"}},{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Restore"},{"expect":"role:button:Archived","visible":false}]'
yv state --expect '{"sessions":[{"id":"{{sid_echo}}","archived":false}]}'
```

### delete-keeps-worktree
"Delete…" then "Delete, keep the worktree" removes the session and leaves its folder.

```sh
yv state --save fail_cwd='sessions[id={{sid_fail}}].cwd'
yv drive new --viewport desktop --steps '[{"click":"role:button:Actions for [fail] TEST-failing turn"},{"click":"role:menuitem:Delete…"},{"click":"role:menuitem:Delete, keep the worktree"},{"expect":{"css":".session-row","hasText":"[fail] TEST-failing turn"},"visible":false}]'
test -d "$(v fail_cwd)"
```

### group-new
"Move to group…", "New group…" and a name put the session under a header with the name and a count.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Move to group…"},{"click":"role:menuitem:New group…"},{"fill":"label:Group name","value":"Release"},{"press":"Enter"},{"expect":{"css":".group-head","hasText":"Release"},"text":"1"}]'
yv state --expect '{"sessions":[{"id":"{{sid_echo}}","group":"Release"}]}'
```

### group-move-to-existing
The menu lists the existing group; choosing it joins the section.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Actions for [think] TEST-plan"},{"click":"role:menuitem:Move to group…"},{"click":"role:menuitem:Release"},{"expect":{"css":".group-head","hasText":"Release"},"text":"2"}]'
yv state --expect '{"sessions":[{"id":"{{sid_think}}","group":"Release"}]}'
```

### group-drag-out-to-ungrouped
Viewports: desktop. Dragging a grouped row onto the ungrouped rows takes it out
of its group (`session.update { group: null }`); the dragged row dims and the
Sessions area lights up while it is held there. Phones (the drawer below
700 px) have no HTML5 drag on touch, so rows there are not draggable and the
row menu is the way; the last step checks that.

```sh
yv drive new --viewport desktop --steps '[{"drag":{"css":".session-item","hasText":"TEST-say hello"},"to":{"css":".session-item","hasText":"[edit] TEST-fix the greeting"},"during":"held-over-sessions"},{"expect":{"css":".group-head","hasText":"Release"},"text":"1"},{"expect":{"css":".group-zone .session-row","hasText":"TEST-say hello"},"visible":false}]'
yv state --expect '{"sessions":[{"id":"{{sid_echo}}","group":null},{"id":"{{sid_think}}","group":"Release"}]}'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":{"css":".session-row","hasText":"TEST-say hello"}},{"eval":"document.querySelectorAll(\".session-item[draggable=true]\").length","equals":0}]'
```

### group-drag-into-group
Viewports: desktop. Dropping a row on a group's header moves it into the group;
on a collapsed group, resting there for a moment opens it first.

```sh
yv drive new --viewport desktop --steps '[{"click":{"css":".group-toggle","hasText":"Release"}},{"expect":{"css":".session-row","hasText":"[think] TEST-plan"},"visible":false},{"drag":{"css":".session-item","hasText":"TEST-say hello"},"to":{"css":".group-head","hasText":"Release"},"hold":900,"during":"held-over-group"},{"expect":{"css":".group-head","hasText":"Release"},"text":"2"},{"expect":{"css":".group-zone .session-row","hasText":"TEST-say hello"}},{"expect":{"css":".group-zone .session-row","hasText":"[think] TEST-plan"}}]'
yv state --expect '{"sessions":[{"id":"{{sid_echo}}","group":"Release"}]}'
```

### group-drag-noop
Viewports: desktop. Dropping a row on the group it is already in sends nothing:
the group refuses the drop, so the session's `updatedAt` does not move.

```sh
yv state --save think_at='sessions[id={{sid_think}}].updatedAt'
yv drive new --viewport desktop --steps '[{"drag":{"css":".session-item","hasText":"[think] TEST-plan"},"to":{"css":".group-head","hasText":"Release"},"during":"held-over-own-group"},{"expect":{"css":".group-head","hasText":"Release"},"text":"2"},{"expect":"css:.drop-over","visible":false}]'
yv state --expect '{"sessions":[{"id":"{{sid_think}}","group":"Release","updatedAt":{{think_at}}}]}'
```

### group-collapse-persists
Clicking the header hides the group's rows; the choice survives a reload (kept in this browser).

```sh
yv drive new --viewport desktop --steps '[{"click":{"css":".group-toggle","hasText":"Release"}},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"visible":false},{"reload":true},{"goto":"new"},{"expect":{"css":".group-head","hasText":"Release"}},{"expect":{"css":".session-row","hasText":"TEST-say hello"},"visible":false},{"click":{"css":".group-toggle","hasText":"Release"}},{"expect":{"css":".session-row","hasText":"TEST-say hello"}}]'
```

### group-rename
Rename in the header menu changes the group of every session in it.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Actions for group Release"},{"click":"role:menuitem:Rename"},{"fill":"label:Group name","value":"Shipping"},{"press":"Enter"},{"expect":{"css":".group-head","hasText":"Shipping"},"text":"2"},{"expect":{"css":".group-head","hasText":"Release"},"visible":false}]'
yv state --expect '{"sessions":[{"id":"{{sid_echo}}","group":"Shipping"},{"id":"{{sid_think}}","group":"Shipping"}]}'
```

### group-remove-from-group
"Remove from group" takes one session out; the group keeps the rest.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Actions for [think] TEST-plan"},{"click":"role:menuitem:Remove from group"},{"expect":{"css":".group-head","hasText":"Shipping"},"text":"1"}]'
yv state --expect '{"sessions":[{"id":"{{sid_think}}","group":null},{"id":"{{sid_echo}}","group":"Shipping"}]}'
```

### group-ungroup
Ungroup empties the group: the header is gone and the sessions are ungrouped.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Actions for group Shipping"},{"click":"role:menuitem:Ungroup"},{"expect":"css:.group-head","visible":false},{"expect":{"css":".session-row","hasText":"TEST-say hello"}}]'
yv state --expect '{"sessions":[{"id":"{{sid_echo}}","group":null}]}'
```

### archived-header-stays-at-bottom
With many sessions the list scrolls, but the Archived header stays at the bottom of the session list, just above the sidebar's footer (usage meters, host rows), and opening it expands upward with its own scroll.

```sh
for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14; do yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"TEST-many '$i'"}' > /dev/null; done
yv api '{"op":"session.update","sessionId":"{{sid_think}}","archived":true}'
yv drive new --viewport desktop --steps '[{"eval":"(document.querySelector(\".session-list\").scrollTop = 9999, document.querySelector(\".session-list\").scrollHeight > document.querySelector(\".session-list\").clientHeight)","equals":true},{"expect":"role:button:Archived","inViewport":true},{"eval":"Math.abs(document.querySelector(\".archived-dock\").getBoundingClientRect().bottom - document.querySelector(\".archived-dock\").nextElementSibling.getBoundingClientRect().top) < 2","equals":true},{"click":"role:button:Archived"},{"expect":{"css":".session-row","hasText":"[think] TEST-plan"},"inViewport":true},{"eval":"Math.abs(document.querySelector(\".archived-dock\").getBoundingClientRect().bottom - document.querySelector(\".archived-dock\").nextElementSibling.getBoundingClientRect().top) < 2","equals":true}]'
```

### hand-off-keeps-the-group
A hand-off from a grouped session puts the new session in the same group.
It uses the Claude session: earlier recipes delete `sid_fail` and archive `sid_think`.

```sh
yv api '{"op":"session.update","sessionId":"{{sid_claude}}","group":"TEST-handoff"}' --wait '{"op":"session","session":{"id":"{{sid_claude}}","group":"TEST-handoff"}}'
yv api '{"op":"session.handoff","sessionId":"{{sid_claude}}","engine":"codex","model":"fake-fast","effort":"low"}' --wait '{"op":"created"}'
yv state --expect '{"sessions":[{"engine":"codex","title":"TEST-hello from Claude","group":"TEST-handoff"}]}'
yv drive new --viewport desktop --steps '[{"expect":{"css":".group-head","hasText":"TEST-handoff"},"text":"2"}]'
```

### hand-off-from-a-pinned-session-stays-in-its-group
Viewports: both
A hand-off from a pinned, grouped session is not pinned. Its row is listed in
the group it belongs to, not nested under the pinned parent, so the group's
count matches the rows under it.

```sh
yv api '{"op":"session.update","sessionId":"{{sid_echo}}","pinned":true,"group":"TEST-pinned"}' --wait '{"op":"session","session":{"id":"{{sid_echo}}","pinned":true,"group":"TEST-pinned"}}'
yv api '{"op":"session.handoff","sessionId":"{{sid_echo}}","engine":"codex","model":"fake-deep","effort":"low"}' --wait '{"op":"created"}'
yv state --expect '{"sessions":[{"engine":"codex","title":"TEST-say hello","model":"fake-deep","group":"TEST-pinned","pinned":false}]}'
yv drive new --viewport desktop --steps '[{"expect":{"css":".group-head","hasText":"TEST-pinned"},"text":"1"},{"expect":{"css":".group-zone .session-row","hasText":"Fake Deep"}},{"eval":"[...document.querySelectorAll(\".session-list .drop-zone\")].find(z => z.querySelector(\".list-head\")?.textContent === \"Pinned\").textContent.includes(\"Fake Deep\")","equals":false},{"shot":"pinned-handover"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":{"css":".group-head","hasText":"TEST-pinned"},"text":"1"},{"expect":{"css":".group-zone .session-row","hasText":"Fake Deep"}},{"eval":"[...document.querySelectorAll(\".session-list .drop-zone\")].find(z => z.querySelector(\".list-head\")?.textContent === \"Pinned\").textContent.includes(\"Fake Deep\")","equals":false},{"shot":"pinned-handover"}]'
```

### menu-on-a-row-below-the-fold
With more sessions than the list shows, scrolling down to an older row and
opening its … menu works at both viewports: the menu opens on screen next to
its row and its Rename asks for the name. Scrolling
closes an open row menu on purpose (a fixed menu would drift away from its
row), so the steps scroll the list first and let it settle before the click.

```sh
for i in 1 2 3 4 5 6 7 8; do yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"TEST-filler '$i'"}' --wait '{"op":"event","event":{"type":"turn_completed"}}'; done
yv drive new --viewport desktop --steps '[{"expect":{"css":".session-row","hasText":"TEST-filler 8"}},{"eval":"(()=>{const l=document.querySelector(\".session-list\");const b=document.querySelector(\"[aria-label=\\\"Actions for [edit] TEST-fix the greeting\\\"]\");const below=b.getBoundingClientRect().top>=l.getBoundingClientRect().bottom;b.scrollIntoView({block:\"center\"});return below})()","equals":true},{"wait":400},{"click":"role:button:Actions for [edit] TEST-fix the greeting"},{"expect":"role:menuitem:Rename","inViewport":true},{"shot":"menu-below-fold"},{"click":"role:menuitem:Rename"},{"expect":"label:Session name","value":"[edit] TEST-fix the greeting"},{"press":"Escape"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":{"css":".session-row","hasText":"TEST-filler 8"}},{"eval":"(()=>{const l=document.querySelector(\".session-list\");const b=document.querySelector(\"[aria-label=\\\"Actions for [edit] TEST-fix the greeting\\\"]\");const below=b.getBoundingClientRect().top>=l.getBoundingClientRect().bottom;b.scrollIntoView({block:\"center\"});return below})()","equals":true},{"wait":400},{"click":"role:button:Actions for [edit] TEST-fix the greeting"},{"expect":"role:menuitem:Rename","inViewport":true},{"shot":"menu-below-fold"},{"click":"role:menuitem:Rename"},{"expect":"label:Session name","value":"[edit] TEST-fix the greeting"},{"press":"Escape"}]'
```

### session-named-from-the-conversation
After the first turn the row shows the generated title instead of the task's first line.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{work}}","text":"[name] TEST-hey can you look at this"}' --wait '{"op":"session","session":{"title":"Fake named session"}}'
yv drive new --viewport desktop --steps '[{"expect":{"css":".session-title","hasText":"Fake named session"}},{"expect":{"css":".session-title","hasText":"TEST-hey can you look"},"visible":false}]'
```

## Gotchas
- Row menus render on document.body (a portal), so find menu items by role, not inside the row.
- A click on a row's … button that first has to scroll the row into view
  (`drive` does that by itself) opens the menu and closes it again at once: the
  scroll event lands after the click, and scrolling closes row menus on
  purpose. With more sessions than fit (more than about 9 on a phone), scroll
  `.session-list` and wait before clicking, as menu-on-a-row-below-the-fold does.
  Found by the patrol run of 2026-11-21 01:00.
