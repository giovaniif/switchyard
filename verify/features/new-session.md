# New session

The page a user lands on: a centered prompt box with the model, effort and
permission pickers, the host and folder buttons under it, and a note that says
whether the session gets a worktree. Sending creates a session, opens it, and
the first turn runs. Send is disabled while the prompt is empty.

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

### page-renders
Expect at both viewports: the heading "What should we work on?", the Message
box, Send disabled, the host button "iso-host". No console, page or API errors.

```sh
yv drive new --steps '[{"expect":"role:heading:What should we work on?"},{"expect":"label:Message"},{"expect":{"role":"button","name":"Send"},"enabled":false},{"expect":"role:button:iso-host"},{"shot":"new-session"}]'
```

### send-creates-session
Pick a Codex model, type a prompt in the seeded repository and send. Expect the
session page with the prompt as the heading and the fake's reply "Echo: …".
Read back: the daemon lists the session as idle, with a `yard/` branch.

```sh
yv drive new --viewport desktop --steps '[{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Codex · Default"}},{"press":"Escape"},{"click":"role:button:work"},{"fill":"role:textbox:Folder","value":"{{repo}}"},{"press":"Enter"},{"fill":"label:Message","value":"TEST-new from the page"},{"click":"role:button:Send"},{"expect":"role:heading:TEST-new from the page"},{"expect":"css:.thread","text":"Echo: TEST-new from the page"}]'
yv state --expect '{"sessions":[{"title":"TEST-new from the page","status":"idle","engine":"codex","branch":"~yard/"}]}'
```

### long-prompt-titled-at-a-word
A first line longer than 60 characters becomes a title cut at a word and
marked with "…": the desktop header reads "TEST-a very long first prompt
that becomes the title and…", not "…the title and kee". (It used to stop
mid-word with no mark, so it read like a typo.)

```sh
for vp in desktop mobile; do
  yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"TEST-a very long first prompt that becomes the title and keeps going '$vp'"}' --wait '{"op":"created"}' --save long_sid=sessionId
  yv drive 'session:{{long_sid}}' --viewport $vp --steps '[{"expect":"css:.thread","text":"Echo: TEST-a very long"},{"expect":"role:heading:TEST-a very long first prompt"},{"shot":"long-title-'$vp'"},{"eval":"document.querySelector(\"h1\").textContent","equals":"TEST-a very long first prompt that becomes the title and…"}]'
done
```

### long-second-word-is-cut-not-dropped
A short word followed by one that runs past 60 characters (a pasted URL) is
cut through that word and marked with "…": the header reads "TEST-fix
https://github.com/example-org/example-repo/issues…", not just "TEST-fix…".
(Backing up to the last whole word used to throw the URL away.)

```sh
for vp in desktop mobile; do
  yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"permission":"ask","text":"TEST-fix https://github.com/example-org/example-repo/issues/12345#issuecomment-987654321-'$vp'"}' --wait '{"op":"created"}' --save url_sid=sessionId
  yv drive 'session:{{url_sid}}' --viewport $vp --steps '[{"expect":"css:.thread","text":"Echo: TEST-fix https://github.com"},{"expect":"role:heading:TEST-fix https://github.com"},{"shot":"url-title-'$vp'"},{"eval":"document.querySelector(\"h1\").textContent","equals":"TEST-fix https://github.com/example-org/example-repo/issues…"}]'
done
```

### empty-prompt-cannot-send
Expect Send disabled with nothing typed, enabled after typing, disabled again
after clearing.

```sh
yv drive new --steps '[{"expect":{"role":"button","name":"Send"},"enabled":false},{"fill":"label:Message","value":"x"},{"expect":{"role":"button","name":"Send"},"enabled":true},{"fill":"label:Message","value":""},{"expect":{"role":"button","name":"Send"},"enabled":false}]'
```

### worktree-note
The note under the prompt says what happens to the folder: in a repository a
new worktree, in a plain folder "Works in the folder itself".

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:work"},{"fill":"role:textbox:Folder","value":"{{plain}}"},{"press":"Enter"},{"expect":"text:Works in the folder itself"}]'
```

### base-branch-picker
Viewports: desktop
In a repository the folder row gets a branch chip, labelled with the current
branch. Its menu lists the branches with "current" beside the checked-out
one and filters as you type; picking another branch cuts the new worktree from
it. Read back: the session's worktree holds the file only that branch has.

```sh
R="$(v repo)"
git -C "$R" branch -D TEST-base >/dev/null 2>&1 || true
git -C "$R" worktree add -q -b TEST-base "$R-TEST-base-wt" >/dev/null
echo base > "$R-TEST-base-wt/TEST-base-only.txt"
git -C "$R-TEST-base-wt" add TEST-base-only.txt
git -C "$R-TEST-base-wt" -c user.name=TEST -c user.email=test@example.com commit -q -m "TEST-base only"
git -C "$R" worktree remove --force "$R-TEST-base-wt"
CUR="$(git -C "$R" branch --show-current)"
yv drive new --viewport desktop --steps '[{"click":"role:button:work"},{"fill":"role:textbox:Folder","value":"{{repo}}"},{"press":"Enter"},{"expect":"css:button[title=\"Branch the worktree starts from\"]","text":"'"$CUR"'"},{"click":"css:button[title=\"Branch the worktree starts from\"]"},{"expect":{"css":".menu-item","hasText":"'"$CUR"'"},"text":"current"},{"fill":"role:textbox:Base branch","value":"TEST-ba"},{"expect":"css:.menu-item","count":1},{"click":{"css":".menu-item","hasText":"TEST-base"}},{"expect":"css:button[title=\"Branch the worktree starts from\"]","text":"TEST-base"},{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Codex · Default"}},{"press":"Escape"},{"fill":"label:Message","value":"TEST-from the base branch"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-from the base branch"}]'
yv state --expect '{"sessions":[{"title":"TEST-from the base branch","status":"idle","branch":"~yard/"}]}'
WT="$(yv state | jq -er '.sessions[] | select(.title=="TEST-from the base branch") | .cwd')"
test -f "$WT/TEST-base-only.txt"
```

### typed-path-with-no-match-says-so
Viewports: both
A typed path whose last part matches no folder (a typo, or a folder that does
not exist yet) says so under the field instead of leaving the list blank, and
Enter does not take it: the chip keeps the folder it had.

```sh
yv drive new --steps '[{"click":"role:button:work"},{"fill":"role:textbox:Folder","value":"{{work}}/TEST-no-such-folder"},{"expect":"css:.picker-empty","text":"No folder in"},{"expect":"css:.picker-empty","text":"TEST-no-such-folder"},{"shot":"no-match"},{"press":"Enter"},{"expect":"css:.picker-empty","text":"TEST-no-such-folder"},{"press":"Escape"},{"expect":{"role":"button","name":"work","exact":true}}]'
```

### typed-path-under-a-missing-folder-names-it
Viewports: both
A typed path whose parent folder does not exist (a typo higher up, or a
folder deleted since) names the first folder that is not there, in words,
and offers no "Use" row for it: Enter keeps the folder the chip had.
(Before #462 the list showed yardd's raw "cannot read …: No such file or
directory (os error 2)" and offered "Use <the missing folder>".)

```sh
yv drive new --steps '[{"click":"role:button:work"},{"fill":"role:textbox:Folder","value":"{{work}}/TEST-gone/TEST-deeper/b"},{"expect":"css:.picker-empty","text":"TEST-gone doesn’t exist."},{"expect":"text:os error","count":0},{"shot":"missing-parent"},{"fill":"role:textbox:Folder","value":"{{work}}/TEST-gone/"},{"expect":"css:.picker-empty","text":"TEST-gone doesn’t exist."},{"expect":"role:option","count":0},{"press":"Enter"},{"press":"Escape"},{"expect":{"role":"button","name":"work","exact":true}}]'
```

### remembered-folder-that-is-gone-says-so
Viewports: both
The page reopens on the folder last used. When that folder was deleted since,
the note under the chips says it no longer exists (not "Works in the folder
itself") and Send stays disabled with a task typed, instead of failing only
after Send. Found as patrol finding #171.

```sh
yv drive new --steps '[{"eval":"(()=>{localStorage.setItem(\"yard.cwd\",\"{{work}}/TEST-gone-later\");return 1})()","equals":1},{"reload":true},{"goto":"new"},{"expect":"css:.new-chips .bad","text":"no longer exists"},{"expect":"text:Works in the folder itself","visible":false},{"fill":"label:Message","value":"TEST-into a gone folder"},{"expect":{"role":"button","name":"Send","exact":true},"enabled":false},{"shot":"gone-folder"}]'
```

### long-folder-keeps-its-chip-icon
Viewports: both
A folder name long enough to be cut with "…" keeps its chip's icon at full
size (and its chevron), as on a short name, instead of the label squeezing
them (#170: the 14 px folder icon rendered 8 px wide).

```sh
mkdir -p "$(v work)/TEST-a-really-long-project-folder-name-for-the-mobile-chips"
yv drive new --steps '[{"eval":"(()=>{localStorage.setItem(\"yard.cwd\",\"{{work}}/TEST-a-really-long-project-folder-name-for-the-mobile-chips\");return 1})()","equals":1},{"reload":true},{"goto":"new"},{"expect":{"css":".new-chips .chip","hasText":"TEST-a-really-long"}},{"eval":"(() => { const icons = [...document.querySelectorAll(\".new-chips .chip svg\")]; return icons.length >= 2 && icons.every((i) => i.getBoundingClientRect().width === Number(i.getAttribute(\"width\"))); })()","equals":true},{"shot":"long-folder-chip"}]'
```

### folder-menu-stays-on-screen
Viewports: both
The folder menu opens fully on screen and the page stays put: the heading
still reads "What should we work on?" with nothing scrolled sideways. At
desktop it opens under the folder chip. (On a 390 px phone it used to hang
from the folder chip, the second in the row, so it ran ~100 px past the right
edge and focusing its search box scrolled the whole page left: "at should we
work on?".)

```sh
yv drive new --steps '[{"click":{"role":"button","name":"work","exact":true}},{"expect":"placeholder:Search folders, or type a path"},{"wait":300},{"shot":"folder-menu"},{"eval":"(() => { const pop = document.querySelector(\".chip-popover-wide\").getBoundingClientRect(); const chip = document.querySelector(\".new-chips .chip[aria-expanded=true]\").getBoundingClientRect(); const scrolled = [...document.querySelectorAll(\"*\")].filter((e) => e.scrollLeft > 0).length; const underChip = innerWidth <= 700 || Math.abs(pop.left - chip.left) < 1; return pop.left >= 0 && pop.right <= innerWidth && scrolled === 0 && underChip; })()","equals":true}]'
```

### recent-folder-opens-with-tab
Viewports: both
A Recent folder is a folder like the ones listed under it: it has its open
(›) button, a repository wears the branch icon, and Tab on it (the key the ›
button names) lists the folders inside while the search box keeps focus.
(Recent rows had no › and no git flag, so Tab on the highlighted one did
nothing and moved focus to another row's › button: typing stopped filtering.
Patrol finding #369.)

```sh
yv drive new --steps '[{"click":{"role":"button","name":"work","exact":true}},{"fill":"placeholder:Search folders, or type a path","value":"TEST-demo"},{"expect":"css:.picker-head","text":"Recent"},{"expect":"css:.folder-row.cursor","text":"TEST-demo-app"},{"expect":"role:button:Open TEST-demo-app"},{"eval":"document.querySelector(\".folder-row.cursor svg path\").getAttribute(\"d\").startsWith(\"M6 3v12\")","equals":true},{"press":"Tab"},{"expect":"placeholder:Search folders, or type a path","value":"{{repo}}/"},{"eval":"document.activeElement === document.querySelector(\".menu-search\")","equals":true},{"expect":"css:.folder-row","text":"Use "},{"shot":"recent-tab"}]'
```

### refused-send-names-the-session-in-the-way
Viewports: both
A folder that cannot get a worktree (here a plain folder) takes one working
session at a time. Sending another there is refused, and the banner names the
session in the way and says to stop it or let it finish. The task stays in the
box. (It used to say "Another session is working in this folder. Stop it
first, or tick New worktree.", a box the page has not had since every
repository session got a worktree.)

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"[slow] TEST-first writer"}' --wait '{"op":"created"}' --save hold_sid=sessionId
yv drive new --allow-errors --steps '[{"click":{"role":"button","name":"work","exact":true}},{"fill":"placeholder:Search folders, or type a path","value":"{{plain}}"},{"press":"Enter"},{"fill":"label:Message","value":"TEST-second writer"},{"click":{"role":"button","name":"Send","exact":true}},{"expect":"css:.banner-error","text":"\"[slow] TEST-first writer\" is working in this folder. Stop it or let it finish first."},{"expect":"label:Message","value":"TEST-second writer"},{"shot":"refused"}]'
yv api '{"op":"session.interrupt","sessionId":"{{hold_sid}}"}'
```

### task-draft-survives-opening-a-session
A half-typed task stays in the box while you look at a session and come back
through New session. Once that task has started a session, the box is empty
again.

```sh
yv drive new --viewport desktop --steps '[{"fill":"label:Message","value":"TEST-half a task"},{"click":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":"css:.thread","text":"Echo:"},{"click":{"css":".nav-row","hasText":"New session"}},{"expect":"label:Message","value":"TEST-half a task"},{"shot":"task-kept"},{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Codex · Default"}},{"press":"Escape"},{"click":"role:button:work"},{"fill":"role:textbox:Folder","value":"{{repo}}"},{"press":"Enter"},{"click":"role:button:Send"},{"expect":"role:heading:TEST-half a task"},{"click":{"css":".nav-row","hasText":"New session"}},{"expect":"label:Message","value":""}]'
yv drive new --viewport mobile --steps '[{"press":"Escape"},{"fill":"label:Message","value":"TEST-task on the phone"},{"click":"role:button:Open sessions"},{"click":{"css":".session-row","hasText":"TEST-say hello"}},{"expect":"css:.thread","text":"Echo:"},{"click":"role:button:Open sessions"},{"click":{"css":".nav-row","hasText":"New session"}},{"expect":"label:Message","value":"TEST-task on the phone"},{"shot":"task-kept-mobile"}]'
```

### folder-picker-lists-folders (manual: the Finder dialog only exists in the desktop app)
"Choose in Finder…" opens the macOS dialog through Tauri. The typed-path
browsing is covered by worktree-note.

## Gotchas
- The default model is Claude Opus 5.5 even when only Codex is logged in; the fakes log both in.
- The folder field browses as you type (`fs.dirs`); press Enter to pick the typed path. A path that names no folder is not taken; the list says so.
- `role:button:Send` is not exact either: a sidebar row titled "TEST-… send" matches it first on desktop. Use `{"role":"button","name":"Send","exact":true}` once sessions exist.
- `label:Folder` is not exact: once a session whose title contains "folder" exists (as in an iso you have been exploring), the sidebar row's button matches first and `fill` fails with "Element is not an <input>". Target the field by `placeholder:Search folders, or type a path` there.
