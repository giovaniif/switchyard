# Accounts

More than one login per engine and per host, for example a personal and a
company Claude Code subscription. Each engine has its **Default** account (the
login the CLI already has) plus added accounts, each with its own config
folder under yardd's data dir (`CLAUDE_CONFIG_DIR` for Claude Code,
`CODEX_HOME` for Codex). The Hosts page lists them per engine with Add
account, Log in and Remove; a separate account picker lists engine, account name and login identity. A session keeps the
account it was created with (shown in its header and sidebar row); it cannot
be changed mid-session. The fakes keep login state per config folder and log
which folder a sidecar started under (`config=` in `sidecar.log`).

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

### add-account
Viewports: desktop
Adding a Claude Code account on this host lists it under Claude Code, not
logged in, with Log in and Remove. Default keeps its own row. (At 390 px the
Hosts page does not fit: see layout/mobile-hosts-page.)

```sh
yv drive hosts --viewport desktop --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Add Claude Code account"},{"fill":"label:Account name","value":"TEST-Work"},{"press":"Enter"},{"expect":{"css":".account-row","hasText":"TEST-Work"},"text":"not logged in"},{"expect":{"css":".engine-row","hasText":"Claude Code"},"text":"logged in as me@example.com"},{"shot":"account-added"}]'
```

### empty-name-add-looks-disabled
Viewports: desktop
With the account name empty, Add is disabled and looks it (faded, like the
other disabled controls), so it does not read as a button that ignores the
click. Typing a name brings it back to full strength.

```sh
yv drive hosts --viewport desktop --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Add Claude Code account"},{"expect":{"css":".account-add .btn-primary","hasText":"Add"},"enabled":false},{"eval":"Number(getComputedStyle(document.querySelector(\".account-add .btn-primary\")).opacity) < 1","equals":true},{"shot":"empty-name"},{"fill":"label:Account name","value":"TEST-Faded"},{"expect":{"css":".account-add .btn-primary","hasText":"Add"},"enabled":true},{"eval":"getComputedStyle(document.querySelector(\".account-add .btn-primary\")).opacity","equals":"1"},{"shot":"named"}]'
```

### refused-add-account-keeps-the-form
Viewports: desktop
A name yardd refuses ("default", or one already in use) shows the error and
keeps the form with the name typed; a free name then adds the account and
closes the form.

```sh
yv drive hosts --viewport desktop --allow-errors --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Add Claude Code account"},{"fill":"label:Account name","value":"default"},{"press":"Enter"},{"expect":"css:.banner-error","text":"account named default already exists"},{"wait":500},{"expect":"label:Account name","value":"default"},{"shot":"refused"},{"fill":"label:Account name","value":"TEST-Second"},{"press":"Enter"},{"expect":{"css":".account-row","hasText":"TEST-Second"},"text":"not logged in"},{"expect":"label:Account name","count":0},{"shot":"added"}]'
```

### log-in-account
Viewports: desktop
Log in to the added account with its own dialog; the fake accepts `good-code`.
The account then shows as logged in under its own email and Default is
unchanged.

```sh
yv drive hosts --viewport desktop --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Log in TEST-Work"},{"expect":"role:dialog","text":"Log in to Claude Code (TEST-Work) on iso-host"},{"fill":"label:Code from the sign-in page","value":"good-code","timeout":15000},{"press":"Enter"},{"expect":"role:dialog","text":"Claude Code (TEST-Work) is logged in on iso-host","timeout":15000},{"expect":{"css":".account-row","hasText":"TEST-Work"},"text":"logged in as","timeout":15000},{"expect":{"css":".engine-row","hasText":"Claude Code"},"text":"logged in as me@example.com"}]'
test -f "$(v fake)/claude.log"
grep -q "auth login config=.*/accounts/" "$(v fake)/claude.log"
```

### pick-account-desktop
Viewports: desktop
The separate account picker selects a login and sending runs under that account; header and sidebar identify it.

```sh
yv drive 'new' --viewport desktop --steps '[{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Claude · TEST-Work"}},{"expect":"role:button:Account: Claude, TEST-Work"},{"click":{"role":"button","name":"work","exact":true}},{"fill":"role:textbox:Folder","value":"{{repo}}"},{"press":"Enter"},{"fill":"label:Message","value":"TEST-account pick desktop"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-account pick desktop"},{"expect":"css:.session-account","text":"TEST-Work"},{"click":"role:button:Account: Claude, TEST-Work"},{"expect":"role:dialog:Engine account identity","text":"This session runs as"},{"expect":"css:.anchored-popover [role=menuitemradio]","count":0},{"press":"Escape"},{"expect":{"css":".session-row","hasText":"TEST-account pick desktop"},"text":"TEST-Work"},{"shot":"session-on-account"}]'
grep -q "start .*config=.*/accounts/" "$(v fake)/sidecar.log"
```

### pick-account-mobile
Viewports: mobile
The separate account picker selects a login and sending runs under that account; header and sidebar identify it.

```sh
yv drive 'new' --viewport mobile --steps '[{"click":"role:button:Session setup"},{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Claude \u00b7 TEST-Work"}},{"expect":"role:button:Account: Claude, TEST-Work"},{"press":"Escape"},{"click":{"role":"button","name":"work","exact":true}},{"fill":"role:textbox:Folder","value":"{{repo}}"},{"press":"Enter"},{"fill":"label:Message","value":"TEST-account pick mobile"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-account pick mobile"},{"expect":"css:.session-account","text":"TEST-Work"},{"click":"role:button:Session setup"},{"click":"role:button:Account: Claude, TEST-Work"},{"expect":"role:dialog:Engine account identity","text":"This session runs as"},{"expect":"css:.anchored-popover [role=menuitemradio]","count":0},{"press":"Escape"},{"press":"Escape"},{"click":"role:button:Open sessions"},{"expect":{"css":".session-row","hasText":"TEST-account pick mobile"},"text":"TEST-Work"},{"shot":"session-on-account"}]'
grep -q "start .*config=.*/accounts/" "$(v fake)/sidecar.log"
```

### default-has-no-account-row-without-a-choice
Viewports: desktop
Default is an explicit identity in the account picker. The model menu contains no account controls.

```sh
yv drive 'new' --viewport desktop --steps '[{"click":"css:.account-trigger"},{"expect":{"role":"menuitemradio","name":"Codex · Default"},"count":1},{"expect":{"role":"menuitemradio","name":"Claude · TEST-Work"}},{"press":"Escape"},{"click":"role:button:Model Opus 5.5"},{"expect":"css:.picker-accounts","count":0},{"shot":"separate-controls"}]'
```

### pick-other-engine-account-switches-engine
Viewports: both
Choosing a Codex account moves the account and model together, and creates a Codex session with that account.

```sh
yv api '{"op":"account.add","engine":"codex","name":"TEST-Codex"}' --wait '{"op":"accounts"}'
yv drive 'new' --viewport desktop --steps '[{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Codex · TEST-Codex"}},{"expect":"role:button:Account: Codex, TEST-Codex"},{"expect":"role:button:Model Fake Fast"},{"click":{"role":"button","name":"work","exact":true}},{"fill":"role:textbox:Folder","value":"{{repo}}"},{"press":"Enter"},{"fill":"label:Message","value":"TEST-account pick codex"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-account pick codex"},{"expect":"css:.session-account","text":"TEST-Codex"},{"shot":"codex-account"}]'
yv drive 'new' --viewport mobile --steps '[{"click":"role:button:Session setup"},{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Codex \u00b7 TEST-Codex"}},{"expect":"role:button:Account: Codex, TEST-Codex"},{"expect":"role:button:Model Fake Fast"},{"press":"Escape"},{"click":{"role":"button","name":"work","exact":true}},{"fill":"role:textbox:Folder","value":"{{repo}}"},{"press":"Enter"},{"fill":"label:Message","value":"TEST-account pick codex"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-account pick codex"},{"expect":"css:.session-account","text":"TEST-Codex"},{"shot":"codex-account"}]'
yv state | jq -e '.sessions[] | select(.title == "TEST-account pick codex") | select(.engine == "codex")'
```

### remove-refused-while-in-use
Viewports: desktop
An account with sessions cannot be removed: yardd says so and the row stays.
(The refusal is also an API error, hence `--allow-errors`.)

```sh
yv drive hosts --viewport desktop --allow-errors --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Remove TEST-Work"},{"click":"role:button:Remove TEST-Work"},{"expect":"css:.banner-error","text":"session"},{"expect":{"css":".account-row","hasText":"TEST-Work"}}]'
```

### refusal-names-the-sessions-archived-too
Viewports: both
The refusal names the sessions still on the account and marks an archived
one, which is not in the sidebar and so comes first, so the user knows what
to delete. The
confirming click disarms Remove, so the row does not keep saying "Click again
to remove" beside the refusal. (Before #397: "1 session still use this
account. Delete it first.", nothing named.)

```sh
sid="$(yv state | jq -r '.sessions[] | select(.title == "TEST-account pick mobile") | .id')"
yv api "{\"op\":\"session.update\",\"sessionId\":\"$sid\",\"archived\":true}"
yv drive hosts --allow-errors --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Remove TEST-Work"},{"click":"role:button:Remove TEST-Work"},{"expect":"css:.banner-error","text":"2 sessions still use this account: \"TEST-account pick mobile\" (archived) and \"TEST-account pick desktop\". Delete them first."},{"eval":"document.querySelector(\"[aria-label=\\\"Remove TEST-Work\\\"]\").textContent","equals":"Remove"},{"eval":"document.querySelector(\".banner-error\").scrollIntoView(); 1"},{"shot":"refusal-names-sessions"}]'
yv api "{\"op\":\"session.update\",\"sessionId\":\"$sid\",\"archived\":false}"
```

### remove-account
Viewports: desktop
Once its sessions are deleted the account can be removed: the row goes, and
Default is untouched.

```sh
for t in "TEST-account pick desktop" "TEST-account pick mobile"; do
  sid="$(yv state | jq -r --arg t "$t" '.sessions[] | select(.title == $t) | .id')"
  yv api "{\"op\":\"session.delete\",\"sessionId\":\"$sid\"}" --wait '{"op":"session.deleted"}'
done
yv drive hosts --viewport desktop --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Remove TEST-Work"},{"click":"role:button:Remove TEST-Work"},{"expect":{"css":".account-row","hasText":"TEST-Work"},"visible":false},{"expect":{"css":".engine-row","hasText":"Claude Code"},"text":"logged in as me@example.com"},{"shot":"account-removed"}]'
```

### add-account-stays-at-the-right
Viewports: both
Desktop actions align at the right; on mobile the whole action group moves under the login identity, with no overflow.

```sh
yv drive 'hosts' --steps '[{"click":"role:tab:Engine logins"},{"expect":"role:button:Add OpenCode account"},{"expect":"role:button:Add Gemini account"},{"eval":"[...document.querySelectorAll(\".engine-actions\")].every(a => { const r=a.getBoundingClientRect(), p=a.closest(\".engine-row\").getBoundingClientRect(); return r.left>=p.left && r.right<=p.right+1; })","equals":true},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"engine-actions"}]'
```

### install-command-stays-at-the-right
An engine that is not installed shows its install command at the right edge
of its row, like Add account on the others. The iso cannot uninstall an
engine, so the page rewrites the next `engines` reply to say Grok is not
installed and drops the socket once to ask again.

```sh
yv drive hosts --steps '[{"click":"role:tab:Engine logins"},{"expect":"role:button:Add OpenCode account"},{"eval":"(() => { const d = Object.getOwnPropertyDescriptor(MessageEvent.prototype, \"data\"); let closed = false; Object.defineProperty(MessageEvent.prototype, \"data\", { configurable: true, get() { const v = d.get.call(this); if (!closed && this.target instanceof WebSocket) { closed = true; this.target.close(); } if (typeof v !== \"string\") return v; let m; try { m = JSON.parse(v); } catch { return v; } if (m.op !== \"engines\") return v; m.engines.grok = { installed: false, loggedIn: false }; return JSON.stringify(m); } }); return true; })()"},{"expect":"css:.engine-cmd","text":"grok"},{"eval":"(() => { const c=document.querySelector(\".engine-cmd\").getBoundingClientRect(), p=document.querySelector(\".engine-cmd\").closest(\".engine-row\").getBoundingClientRect(); return c.left>=p.left && c.right<=p.right+1; })()","equals":true},{"shot":"install-command"}]'
```

### account-chip-shows-the-whole-name
Viewports: both
The account chip names the whole account wherever it shows one. In a
session's desktop toolbar the chip is the approved avatar badge ("D") and its
title and accessible name carry "Codex · Default" (the name itself is hidden
there since #290). On a phone the Session setup sheet shows "Codex · Default"
in full, and a long account name ends in an ellipsis inside the sheet instead
of pushing the page wider. A long name never widens the desktop badge. (Before
#262, `.account-trigger { max-width: 100% }` resolved against the picker's own
shrink-to-fit width and clipped the chip to "Codex · Def…" next to free space.)

```sh
yv drive 'session:{{sid_edit}}' --viewport desktop --steps '[{"expect":"role:button:Account: Codex, Default"},{"eval":"(() => { const b = document.querySelector(\".composer .account-trigger\"); return [b.title, b.querySelector(\".account-avatar\").textContent].join(\"|\"); })()","equals":"Codex · Default|D"},{"shot":"account-chip"},{"eval":"(() => { const b = document.querySelector(\".composer .account-trigger\"), w = b.getBoundingClientRect().width; b.querySelector(\".account-trigger-name\").textContent = \"Codex · \" + \"TEST-averyverylongaccountname\".repeat(4); return b.getBoundingClientRect().width === w && document.documentElement.scrollWidth <= innerWidth; })()","equals":true}]'
yv drive 'session:{{sid_edit}}' --viewport mobile --steps '[{"click":"role:button:Session setup"},{"expect":"css:.setup-options .account-trigger-name","text":"Codex · Default"},{"eval":"(() => { const n = document.querySelector(\".setup-options .account-trigger-name\"); return n.scrollWidth <= n.clientWidth; })()","equals":true},{"shot":"account-chip"},{"eval":"(() => { const n = document.querySelector(\".setup-options .account-trigger-name\"); n.textContent = \"Codex · \" + \"TEST-averyverylongaccountname\".repeat(4); const b = n.parentElement.getBoundingClientRect(), sheet = document.querySelector(\".setup-options\").getBoundingClientRect(); return n.scrollWidth > n.clientWidth && b.right <= sheet.right + 0.5 && document.documentElement.scrollWidth <= innerWidth; })()","equals":true}]'
```

### account-chip-chevron-keeps-its-size
Viewports: both
The chip's chevron stays its full 12 px, so the chip still reads as a menu: in
the desktop toolbar next to the badge, and on a phone after a long account
name that ends in an ellipsis. (The chevron was a flex item that could shrink,
and a long name squeezed it to a dot at 390 px.)

```sh
yv drive 'session:{{sid_edit}}' --viewport desktop --steps '[{"expect":"role:button:Account: Codex, Default"},{"eval":"(() => { const b = document.querySelector(\".composer .account-trigger\"); b.querySelector(\".account-trigger-name\").textContent = \"Codex · \" + \"TEST-averyverylongaccountname\".repeat(4); return Math.round(b.querySelector(\"svg\").getBoundingClientRect().width) === 12; })()","equals":true},{"shot":"account-chip-long"}]'
yv drive 'session:{{sid_edit}}' --viewport mobile --steps '[{"click":"role:button:Session setup"},{"expect":"css:.setup-options .account-trigger-name","text":"Codex · Default"},{"eval":"(() => { const n = document.querySelector(\".setup-options .account-trigger-name\"); n.textContent = \"Codex · \" + \"TEST-averyverylongaccountname\".repeat(4); const s = n.parentElement.querySelector(\"svg\").getBoundingClientRect(); return n.scrollWidth > n.clientWidth && Math.round(s.width) === 12; })()","equals":true},{"shot":"account-chip-long"}]'
```

### long-account-name-fits-the-setup-sheet
Viewports: mobile
On a phone, an account name as long as yardd allows (40 characters) ends
in an ellipsis inside the Session setup sheet: every row of the sheet, the account chip and
its chevron included, stays within its edges. (The sheet's grid column sized
to the name's full width, so one long name pushed every row and chevron past
the sheet's right edge and the name was cut off mid-word.)

```sh
yv drive hosts --viewport desktop --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Add Codex account"},{"fill":"label:Account name","value":"TEST-ACME WORLDWIDE ENGINEERING PLATFORM"},{"press":"Enter"},{"expect":{"css":".account-row","hasText":"TEST-ACME WORLDWIDE ENGINEERING PLATFORM"}}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Session setup"},{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Codex · TEST-ACME WORLDWIDE ENGINEERING PLATFORM"}},{"expect":"role:button:Account: Codex, TEST-ACME WORLDWIDE ENGINEERING PLATFORM"},{"expect":"role:dialog:Engine account identity","count":0},{"expect":"role:dialog:Session setup"},{"shot":"long-account-sheet"},{"eval":"(() => { const sheet = document.querySelector(\".setup-options\").getBoundingClientRect(), n = document.querySelector(\".setup-options .account-trigger-name\"), chevron = n.parentElement.querySelector(\"svg\").getBoundingClientRect(); return n.scrollWidth > n.clientWidth && chevron.right <= sheet.right + 0.5 && [...document.querySelectorAll(\".setup-options button\")].every((b) => b.getBoundingClientRect().right <= sheet.right + 0.5); })()","equals":true}]'
```

### long-account-name-wraps-in-the-login-banner
Viewports: mobile
On a phone, New session with an added account that is not logged in and
whose name has no spaces (40 characters, yardd's limit) wraps the name inside
the "isn't logged in" banner: the banner and its Log in button stay on
screen. (The banner's message did not wrap an unbroken name, so it pushed
the Log in button past the right edge of the screen.)

```sh
yv drive hosts --viewport desktop --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Add Codex account"},{"fill":"label:Account name","value":"TEST-ACME_WORLDWIDE_ENGINEERING_PLATFORM"},{"press":"Enter"},{"expect":{"css":".account-row","hasText":"TEST-ACME_WORLDWIDE_ENGINEERING_PLATFORM"},"text":"not logged in"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Session setup"},{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Codex · TEST-ACME_WORLDWIDE_ENGINEERING_PLATFORM"}},{"press":"Escape"},{"expect":"role:dialog:Session setup","count":0},{"expect":{"css":".new-inner .engine-notice","hasText":"TEST-ACME_WORLDWIDE_ENGINEERING_PLATFORM"},"text":"logged in on iso-host"},{"shot":"long-account-banner"},{"eval":"(() => { const n = document.querySelector(\".new-inner .engine-notice\").getBoundingClientRect(), b = document.querySelector(\".new-inner .engine-notice .btn-primary\").getBoundingClientRect(); return n.right <= innerWidth + 0.5 && b.left >= n.left && b.right <= n.right + 0.5; })()","equals":true}]'
```

## Gotchas
- An account's folder is deleted with it, login included. There is no "log out" separate from Remove.
- The account of a session cannot be changed after it starts.
- The session toolbar shows the account as an avatar badge only (approved design, #290); the full name is in the chip's title and accessible name, and in the phone's Session setup sheet. `account-chip-*` expected the visible name in `.composer` until 2026-11-21.
