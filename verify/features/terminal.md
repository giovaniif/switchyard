# Terminal

The Terminal tab runs the user's shell in the session folder through yardd's
PTY. Several terminals per session ("1", "2"...), 256 KB of scrollback replayed
after a reload, and Attach (terminal output as a card in the reply box). On
desktop it is the Terminal tab of the right pane; on phones it is inside the
Workspace sheet.

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

### runs-command
Viewports: desktop
Typing a command shows its output.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"wait":"label:Terminal input"},{"type":"echo TEST-$((40+2))\n","target":"label:Terminal input"},{"expect":"css:.xterm","text":"TEST-42"},{"shot":"terminal"}]'
```

### keys-arrive-in-order
Viewports: desktop
Keys typed in a quick run reach the shell in the order typed: the echoed
command and its output both match what was typed. Found 2026-11-15: the shell
got `…qrstuvxwyz…` (keys swapped), about half the runs; yardd handled each
keystroke in its own task.

```sh
yv drive 'session:{{sid_edit}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"$"},{"type":"echo TEST-order-abcdefghijklmnopqrstuvwxyz0123456789-abcdefghijklmnopqrstuvwxyz0123456789-abcdefghijklmnopqrstuvwxyz0123456789\n","target":"label:Terminal input"},{"expect":"css:.xterm","text":"TEST-order-"},{"wait":1000},{"eval":"document.querySelector(\".xterm-rows\").innerText.replace(/\\s+/g, \"\").includes(\"$echoTEST-order-abcdefghijklmnopqrstuvwxyz0123456789-abcdefghijklmnopqrstuvwxyz0123456789-abcdefghijklmnopqrstuvwxyz0123456789TEST-order-abcdefghijklmnopqrstuvwxyz0123456789-abcdefghijklmnopqrstuvwxyz0123456789-abcdefghijklmnopqrstuvwxyz0123456789\")","equals":true}]'
```

### second-terminal
Viewports: desktop
"New terminal" adds tab 2 and selects it.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"click":"role:button:New terminal"},{"expect":{"role":"tab","name":"2"}},{"eval":"document.querySelector(\"[role=tab][aria-selected=true]\")?.textContent?.trim()","equals":"2"}]'
```

### scrollback-after-reload
Viewports: desktop
Output from before a reload is still there after it.

```sh
yv drive 'session:{{sid_think}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"$"},{"type":"echo TEST-before-$((6*7))\n","target":"label:Terminal input"},{"expect":"css:.xterm","text":"TEST-before-42"},{"reload":true},{"goto":"session:{{sid_think}}"},{"click":"role:button:Terminal"},{"expect":"css:.xterm-rows","text":"TEST-before-42","timeout":20000}]'
```

### scrollback-keeps-wrapped-lines
Viewports: desktop
A prompt line longer than the pane wraps; after a reload the replay must show
it exactly as before: both prompts intact, the command after the first one.
Found 2026-11-15 (fixed the same day): the replay came back as `…/yard-test-say$((6*7))735$ echo TEST-before-`.

```sh
yv drive 'session:{{sid_fail}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"$"},{"type":"echo TEST-wrap-$((6*7))\n","target":"label:Terminal input"},{"expect":"css:.xterm","text":"TEST-wrap-42"},{"eval":"document.querySelector(\".xterm-rows\").innerText.replace(/\\s+/g, \"\").includes(\"$echoTEST-wrap-$((6*7))TEST-wrap-42\")","equals":true},{"reload":true},{"goto":"session:{{sid_fail}}"},{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"TEST-wrap-42"},{"eval":"document.querySelector(\".xterm-rows\").innerText.replace(/\\s+/g, \"\").includes(\"$echoTEST-wrap-$((6*7))TEST-wrap-42\")","equals":true}]'
```

### scrollback-matches-after-reloads
Viewports: desktop
A terminal with history keeps exactly the same text across two reloads, after
a long command that wraps over three rows. Found by the verifier on PR #1: an
attach at xterm's default 80 columns made the shell redraw into the scrollback.

```sh
yv drive 'session:{{sid_edit}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"$"},{"type":"echo TEST-first\n","target":"label:Terminal input"},{"expect":"css:.xterm","text":"TEST-first"},{"reload":true},{"goto":"session:{{sid_edit}}"},{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"TEST-first"},{"wait":1500},{"type":"for i in 1 2 3; do echo TEST-row-$i-$(printf ab%.0s $(seq 1 40)); done; echo TEST-done-$((5*5))\n","target":"label:Terminal input"},{"expect":"css:.xterm","text":"TEST-done-25"},{"wait":1000},{"eval":"(() => { const t = document.querySelector(\".xterm-rows\").innerText.split(\"\\n\").map((l) => l.trimEnd()).join(\"\\n\").trimEnd(); localStorage.setItem(\"yv-term\", t); return t.length > 0; })()","equals":true},{"reload":true},{"goto":"session:{{sid_edit}}"},{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"TEST-done-25"},{"wait":2000},{"eval":"document.querySelector(\".xterm-rows\").innerText.split(\"\\n\").map((l) => l.trimEnd()).join(\"\\n\").trimEnd() === localStorage.getItem(\"yv-term\")","equals":true},{"reload":true},{"goto":"session:{{sid_edit}}"},{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"TEST-done-25"},{"wait":2000},{"eval":"document.querySelector(\".xterm-rows\").innerText.split(\"\\n\").map((l) => l.trimEnd()).join(\"\\n\").trimEnd() === localStorage.getItem(\"yv-term\")","equals":true}]'
```

### scrollback-does-not-reply-to-old-probes
Viewports: desktop
The emulator answers a live device query once. Reloading retains the output
and the next byte to reach the waiting shell comes from the keyboard.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"$"},{"type":"saved=$(stty -g); stty -icanon -echo; printf '"'"'\\033[c'"'"'; IFS= read -r -t 3 -d c reply; stty \"$saved\"; if [ \"$reply\" = \"$(printf '"'"'\\033[?1;2'"'"')\" ]; then label=OK; else label=FAILED; fi; printf '"'"'\\nTEST-device-reply-%s\\n'"'"' \"$label\"; IFS= read -r -s -n 1 -t 12 next; if [ \"$next\" = x ]; then label=OK; else label=UNEXPECTED; fi; printf '"'"'\\nTEST-next-input-%s\\n'"'"' \"$label\"\n","target":"label:Terminal input"},{"expect":"css:.xterm-rows","text":"TEST-device-reply-OK"},{"reload":true},{"goto":"session:{{sid_echo}}"},{"click":"role:button:Terminal"},{"expect":"css:.xterm-rows","text":"TEST-device-reply-OK"},{"wait":500},{"type":"x","target":"label:Terminal input"},{"expect":"css:.xterm-rows","text":"TEST-next-input-OK","timeout":3500},{"shot":"clean-scrollback"}]'
```

### last-terminal-closed-offers-a-new-one
Viewports: desktop
Closing the only tab leaves an empty state that says so, with a button that
opens a terminal again. Found 2026-11-16: the pane went blank with only the
small `+` in its strip.

```sh
yv drive 'session:{{sid_think}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"$"},{"click":"role:button:Close terminal"},{"expect":{"role":"tab"},"count":0},{"expect":"text:No terminal is open."},{"shot":"no-terminal"},{"click":"role:button:Open a terminal"},{"expect":{"role":"tab"},"count":1},{"expect":"css:.xterm","text":"$"},{"expect":"text:No terminal is open.","count":0}]'
```

### exited-shell-survives-reload
Viewports: desktop
A tab whose shell exited keeps its scrollback and the exited line after a
reload, with no error. Found 2026-11-16: the fit before the attach resized
the exited terminal and yardd's refusal showed as a "terminal is closed"
banner.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"$"},{"type":"echo TEST-gone-$((6*7)); exit\n","target":"label:Terminal input"},{"expect":"css:.xterm","text":"shell exited"},{"reload":true},{"goto":"session:{{sid_echo}}"},{"click":"role:button:Terminal"},{"expect":"css:.xterm-rows","text":"TEST-gone-42"},{"expect":"css:.xterm-rows","text":"shell exited"},{"wait":1500},{"expect":"text:terminal is closed","count":0},{"shot":"exited-after-reload"}]'
```

### close-terminal
Viewports: desktop
The selected tab has a "Close terminal" cross. Closing tab 2 removes it and
selects tab 1 again. Read back: the daemon lists one terminal for the session.

```sh
yv drive 'session:{{sid_fail}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"expect":{"role":"tab","name":"1"}},{"click":"role:button:New terminal"},{"expect":{"role":"tab","name":"2"}},{"click":"role:button:Close terminal"},{"expect":{"role":"tab","name":"2"},"count":0},{"eval":"document.querySelector(\"[role=tab][aria-selected=true]\")?.textContent?.trim()","equals":"1"}]'
yv api '{"op":"term.list","sessionId":"{{sid_fail}}"}' --wait '{"op":"term.list"}' | jq -e '.reply.terms | length == 1'
```

### folder-gone
Viewports: both
If a worktree is gone, the terminal explains it in Workspace. Conversation
history remains visible without a global banner. The expected failure on the
wire is confined to the terminal pane.

```sh
yv state --save cmd_cwd='sessions[id={{sid_cmd}}].cwd'
rm -rf "$(v cmd_cwd)"
yv drive 'session:{{sid_cmd}}' --viewport desktop --allow-errors --steps '[{"expect":"role:heading:[cmd] TEST-run the migration"},{"expect":{"role":"region","name":"Workspace"},"text":"no longer exists"},{"expect":"css:.banner-error","count":0},{"expect":{"role":"tab","name":"1"},"count":0},{"shot":"missing-folder"}]'
yv drive 'session:{{sid_cmd}}' --viewport mobile --allow-errors --steps '[{"expect":"css:.banner-error","count":0},{"click":"role:button:Workspace"},{"expect":{"role":"region","name":"Workspace"},"text":"no longer exists"},{"expect":"role:button:Try again"},{"click":"role:button:Back to conversation"},{"expect":"css:.banner-error","count":0},{"shot":"history-without-banner"}]'
yv api '{"op":"term.list","sessionId":"{{sid_cmd}}"}' --wait '{"op":"term.list"}' | jq -e '.reply.terms | length == 0'
```

### attach-terminal-rows
Viewports: desktop
The Terminal's Attach menu puts terminal output in a card above the reply box
(where from, line count, monospace, More, ×): the rows on screen, or the last
100 or 500 lines (wrapped rows joined, the blank screen under the prompt left
out). Several stack; × removes one. Sending puts each before the typed text
as a labelled fenced block, and the cards go; the sent bubble shows the
block as code.

```sh
yv drive 'session:{{sid_claude}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"$"},{"wait":1500},{"type":"seq -f TEST-row-%g 3\n","target":"label:Terminal input"},{"expect":"css:.xterm","text":"TEST-row-3"},{"wait":500},{"click":{"role":"button","name":"Attach","exact":true}},{"click":{"role":"menuitem","name":"Rows on screen"}},{"expect":"css:.quote-card.snippet","text":"TEST-row-3"},{"expect":"css:.quote-card.snippet .quote-card-label","text":"Terminal 1 · "},{"click":{"role":"button","name":"Attach","exact":true}},{"click":{"role":"menuitem","name":"Last 100 lines"}},{"eval":"document.querySelectorAll('\''.quote-card.snippet'\'').length","equals":2},{"click":{"role":"button","name":"Remove terminal output"}},{"eval":"document.querySelectorAll('\''.quote-card.snippet'\'').length","equals":1},{"shot":"attached"},{"fill":"label:Message","value":"TEST-what failed?"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"TEST-what failed?"},{"eval":"document.querySelectorAll('\''.quote-card.snippet'\'').length","equals":0},{"expect":"css:.bubble-code","text":"TEST-row-3"},{"shot":"sent"}]'
yv api '{"op":"session.open","sessionId":"{{sid_claude}}"}' --wait '{"op":"history"}' | jq -e '[.reply.events[] | select(.type=="user_message") | .text] | last | test("^Terminal 1, [0-9]+ lines:\\n\\x60{3}text\\n(.|\\n)*TEST-row-3\\n(.|\\n)*\\x60{3}\\n\\nTEST-what failed\\?$")'
```

### first-terminal-opens-at-the-pane-size
Viewports: mobile, desktop
The first terminal of a session shows one shell prompt, sized to its pane: the
phone Workspace sheet and the desktop right pane (about 50 columns) are both
narrower than 80. Fails today (#376): the terminal is opened at 80 columns
before anything measured the pane, so readline redraws after the resize and a
stale, cut-off copy of the prompt sits above the real one (the count reads 2).
Each viewport uses its own new session: a seeded one may already have a
terminal from an earlier recipe, and then nothing opens at the default size.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"TEST-first terminal phone"}' --wait '{"op":"created"}' --save first_term_sid=sessionId
yv api '{"op":"term.list","sessionId":"{{first_term_sid}}"}' --wait '{"op":"term.list"}' | jq -e '.reply.terms | length == 0'
yv drive 'session:{{first_term_sid}}' --viewport mobile --steps '[{"click":"role:button:Workspace"},{"expect":"css:.xterm","text":"$"},{"wait":1500},{"eval":"(document.querySelector(\".xterm-rows\").innerText.match(/@[^:\\s]+:/g) || []).length","equals":1}]'
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"TEST-first terminal desktop"}' --wait '{"op":"created"}' --save first_term_desk_sid=sessionId
yv api '{"op":"term.list","sessionId":"{{first_term_desk_sid}}"}' --wait '{"op":"term.list"}' | jq -e '.reply.terms | length == 0'
yv drive 'session:{{first_term_desk_sid}}' --viewport desktop --steps '[{"click":"role:button:Terminal"},{"expect":"css:.xterm","text":"$"},{"wait":1500},{"eval":"(document.querySelector(\".xterm-rows\").innerText.match(/@[^:\\s]+:/g) || []).length","equals":1}]'
```

### attach-terminal-selection (manual: needs a mouse selection inside xterm)
Select text in the terminal, open Attach, pick Selection: the card holds the
selection. Selecting through xterm's renderer is not scripted yet.

## Gotchas
- Fixed 2026-11-15: the scrollback replayed after a reload used to be written at xterm's default 80 columns before the first fit and then reflowed, garbling wrapped lines and overwriting output. scrollback-after-reload and scrollback-keeps-wrapped-lines guard it.
- Open 2026-11-22 (#376): the first terminal of a session shows the shell prompt twice, a stale copy cut off at the right edge above the real one, whenever its pane is narrower than 80 columns and the prompt wraps: the phone Workspace sheet and the desktop right pane alike. `TerminalPane` sends `term.open` with the default 80x24 before any view has fitted. first-terminal-opens-at-the-pane-size fails at both viewports until it is fixed.
- Wait for the prompt (`expect` text `$`) before typing: keys sent before the shell is ready are echoed out of order.
- xterm ignores `fill`; use the `type` step with `target: "label:Terminal input"`.
