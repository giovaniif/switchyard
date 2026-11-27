# Layout

Switchyard is laid out for a desktop window: sessions sidebar, the session in
the middle, and the workspace pane (Terminal, Subagents and Processes) on the
right, hidden below 1100 px. The web version is also opened from phones over
the tailnet, so the main column must stay usable at 390 px: below 700 px the
sidebar is a drawer over the page, opened by the menu button ("Open sessions")
first in each page's bar, and closed by picking anything in it, tapping the
backdrop, or Escape.

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

### desktop-three-columns
Viewports: desktop
Sidebar, session and workspace pane are all visible at 1440 px.

```sh
yv drive 'session:{{sid_edit}}' --viewport desktop --steps '[{"expect":"role:navigation:Sessions"},{"expect":"role:heading:[edit] TEST-fix the greeting"},{"expect":"role:region:Workspace"},{"shot":"desktop"}]'
```

### narrow-hides-pane
Viewports: mobile
Below 1100 px the workspace pane is hidden and the composer stays on screen.

```sh
yv drive 'session:{{sid_edit}}' --viewport mobile --steps '[{"expect":"role:region:Workspace","visible":false},{"expect":"label:Message","inViewport":true}]'
```

### desktop-sidebars-collapse-independently
Viewports: desktop
Each sidebar can give its space to the conversation independently. The open
Workspace tab and panel widths survive a toggle. The collapse preferences
survive reloads and do not prevent opening a panel on a narrower screen.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[
  {"eval":"localStorage.setItem(\"yard.sidebarWidth\", \"296\"); localStorage.setItem(\"yard.paneWidth\", \"360\"); true"},
  {"reload":true},
  {"click":"role:button:Processes"},
  {"click":"role:button:Collapse sessions sidebar"},
  {"expect":"role:navigation:Sessions","visible":false},
  {"expect":"role:separator:Resize the sessions sidebar","visible":false},
  {"expect":"role:region:Workspace","inViewport":true},
  {"click":"role:button:Collapse workspace"},
  {"expect":"role:region:Workspace","visible":false},
  {"expect":"role:button:Expand sessions sidebar","inViewport":true},
  {"expect":"role:button:Expand workspace","inViewport":true},
  {"eval":"document.querySelector(\".thread\").clientWidth >= innerWidth - 48","equals":true},
  {"shot":"sidebars-collapsed"},
  {"click":"role:button:Expand workspace"},
  {"expect":"css:.workspace-tabs .tab[aria-pressed=true]","text":"Processes"},
  {"expect":"role:navigation:Sessions","visible":false},
  {"eval":"document.querySelector(\".pane\").getBoundingClientRect().width","equals":360},
  {"click":"role:button:Collapse workspace"},
  {"reload":true},
  {"expect":"role:navigation:Sessions","visible":false},
  {"expect":"role:region:Workspace","visible":false},
  {"resize":{"width":390,"height":844}},
  {"click":"role:button:Open sessions"},
  {"click":"role:button:Close sessions"},
  {"click":{"role":"button","name":"Workspace","exact":true}},
  {"expect":"role:region:Workspace","inViewport":true},
  {"resize":{"width":1440,"height":900}},
  {"expect":"role:region:Workspace","visible":false},
  {"expect":"role:navigation:Sessions","visible":false},
  {"click":"role:button:Expand sessions sidebar"},
  {"expect":"role:region:Workspace","visible":false},
  {"expect":"css:button[aria-label=\"Collapse sessions sidebar\"]:focus"},
  {"eval":"document.querySelector(\".sidebar\").getBoundingClientRect().width","equals":296},
  {"click":"role:button:Expand workspace"},
  {"expect":"role:region:Workspace","inViewport":true},
  {"shot":"sidebars-expanded"}
]'
```

### mobile-panels-have-close-controls
Viewports: mobile
Close buttons return to the conversation, with focus back on the panel's opener.

```sh
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[
  {"click":"role:button:Open sessions"},
  {"wait":240},
  {"expect":"role:button:Close sessions","inViewport":true},
  {"shot":"sessions-drawer-close"},
  {"click":"role:button:Close sessions"},
  {"expect":"role:navigation:Sessions","visible":false},
  {"expect":"css:button[aria-label=\"Open sessions\"]:focus"},
  {"click":{"role":"button","name":"Workspace","exact":true}},
  {"click":"role:button:Processes"},
  {"expect":"role:button:Back to conversation","inViewport":true},
  {"shot":"workspace-close"},
  {"click":"role:button:Back to conversation"},
  {"expect":"role:region:Workspace","visible":false},
  {"expect":"css:button.workspace-toggle:focus","text":"Workspace"},
  {"click":{"role":"button","name":"Workspace","exact":true}},
  {"expect":"css:.workspace-tabs .tab[aria-pressed=true]","text":"Processes"}
]'
```

### mobile-main-column-usable
Viewports: mobile
At 390 px the session column must be at least 300 px wide, so prompts and
answers are readable.

```sh
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"eval":"Math.round(document.querySelector(\".main\").getBoundingClientRect().width) >= 300","equals":true}]'
```

### mobile-hosts-page
Viewports: mobile
At 390 px the Hosts heading and the host card's stats must be readable.

```sh
yv drive hosts --viewport mobile --steps '[{"expect":"role:heading:Hosts","inViewport":true},{"eval":"Math.round(document.querySelector(\".host-summary\").getBoundingClientRect().width) >= 300","equals":true}]'
```

### mobile-drawer-opens-and-closes
Viewports: mobile
At 390 px the sidebar starts closed. The menu button opens it on screen;
Escape closes it, a tap on the backdrop beside it closes it, and picking a
session opens that session and closes it, leaving the session the full width.

```sh
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"expect":"role:navigation:Sessions","visible":false},{"click":"role:button:Open sessions"},{"wait":400},{"expect":"role:navigation:Sessions","inViewport":true},{"expect":"role:button:New session","inViewport":true},{"shot":"drawer-open"},{"press":"Escape"},{"expect":"role:navigation:Sessions","visible":false},{"click":"role:button:Open sessions"},{"wait":400},{"eval":"document.elementFromPoint(370, 400).className","equals":"drawer-backdrop"},{"eval":"document.elementFromPoint(370, 400).click() ?? 1","equals":1},{"expect":"role:navigation:Sessions","visible":false},{"click":"role:button:Open sessions"},{"click":{"css":".session-row","hasText":"[edit] TEST-fix the greeting"}},{"expect":"role:heading:[edit] TEST-fix the greeting"},{"expect":"role:navigation:Sessions","visible":false},{"eval":"Math.round(document.querySelector(\".main\").getBoundingClientRect().width)","equals":390},{"shot":"session-after-pick"}]'
```

### mobile-menu-on-every-page
Viewports: mobile
The menu button is on screen on the new-session, Hosts and Automations pages too.

```sh
yv drive new --viewport mobile --steps '[{"expect":"role:button:Open sessions","inViewport":true},{"goto":"hosts"},{"expect":"role:button:Open sessions","inViewport":true},{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"expect":"role:heading:Automations"},{"expect":"role:button:Open sessions","inViewport":true}]'
```

### mobile-delete-confirm-fits-the-bar
Viewports: mobile
At 390 px, Delete on a session with its own worktree arms two choices in the
bar. Each stays on one line inside the bar (nothing spills over the thread or
off the screen), and the first click still only arms them. The bar is the
measure, not `innerWidth`: in mobile emulation `innerWidth` grows with the
overflow, so "Delete + worktree" at x 342-475 passed against it (#310).

```sh
yv drive 'session:{{sid_edit}}' --viewport mobile --steps '[{"click":{"role":"button","name":"Delete","exact":true}},{"expect":"role:button:Delete + worktree"},{"expect":{"css":".main header.bar .bar-choice"},"count":2},{"eval":"(() => { const bar = document.querySelector(\".main header.bar\").getBoundingClientRect(); return document.documentElement.scrollWidth <= Math.round(bar.right) && [...document.querySelectorAll(\".main header.bar button\")].filter((b) => b.getClientRects().length > 0).every((b) => { const r = document.createRange(); r.selectNodeContents(b); const box = r.getBoundingClientRect(); const pill = b.getBoundingClientRect(); return new Set([...r.getClientRects()].filter((x) => x.width > 0).map((x) => Math.round(x.top))).size === 1 && box.bottom <= bar.bottom && pill.right <= bar.right; }); })()","equals":true},{"shot":"delete-armed"}]'
yv state --expect '{"sessions":[{"id":"{{sid_edit}}"}]}'
```

### mobile-delete-confirm-fits-the-bar-without-worktree
Viewports: mobile
A session without its own worktree arms one "Delete session?" button. With a
long title it stays on one line inside its pill and the bar; the title gives
way (#214: it wrapped and spilled out of the button).

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","text":"TEST-a long first prompt that becomes the title of this session and keeps going"}' --wait '{"op":"event","event":{"type":"turn_completed"}}' --save long_title=sessionId
yv drive 'session:{{long_title}}' --viewport mobile --steps '[{"click":{"role":"button","name":"Delete","exact":true}},{"expect":"role:button:Delete session?"},{"eval":"(() => { const bar = document.querySelector(\".main header.bar\").getBoundingClientRect(); const b = [...document.querySelectorAll(\".main header.bar button\")].find((e) => e.textContent.trim() === \"Delete session?\"); const pill = b.getBoundingClientRect(); const r = document.createRange(); r.selectNodeContents(b); const box = r.getBoundingClientRect(); return new Set([...r.getClientRects()].filter((x) => x.width > 0).map((x) => Math.round(x.top))).size === 1 && box.bottom <= pill.bottom && pill.bottom <= bar.bottom && pill.right <= window.innerWidth; })()","equals":true},{"shot":"delete-armed-no-worktree"}]'
yv state --expect '{"sessions":[{"id":"{{long_title}}"}]}'
```

### mobile-delete-ask-keeps-the-title
Viewports: mobile
At 390 px the single "Delete session?" ask of a session without its own
worktree steps the side controls aside like the worktree choices do (#312),
so the title stays whole and you see which session you are deleting (#516:
it shrank to "TES…" while iPhone and Workspace kept their room). When the ask
lapses they come back, and nothing was deleted. It works in the plain
folder: a turn still going in `{{repo}}` (from the recipe above) would
refuse a second session there.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"TEST-remove the old one"}' --wait '{"op":"event","event":{"type":"turn_completed"}}' --save delete_ask=sessionId
yv drive 'session:{{delete_ask}}' --viewport mobile --steps '[{"click":{"role":"button","name":"Delete","exact":true}},{"expect":"role:button:Delete session?"},{"expect":"role:button:Workspace","visible":false},{"expect":"role:button:Set up iPhone app","visible":false},{"eval":"(() => { const h = document.querySelector(\".main header.bar h1\"); const bar = document.querySelector(\".main header.bar\").getBoundingClientRect(); return h.scrollWidth <= h.clientWidth && document.documentElement.scrollWidth <= Math.round(bar.right); })()","equals":true},{"shot":"delete-ask-title"},{"expect":"role:button:Workspace","timeout":6000},{"expect":{"role":"button","name":"Delete","exact":true}}]'
yv state --expect '{"sessions":[{"id":"{{delete_ask}}"}]}'
```

### double-click-on-delete-only-asks
A double click on Delete leaves the ask up and deletes nothing. The first
click arms "Delete session?" (or, in its own worktree, "Delete session" and
"Delete and remove worktree"); the second lands where the pointer is, on the
ask it just opened. It used to answer it: a double click deleted the session,
and in a worktree session it hit "Delete and remove worktree" (#519). The
eval sends what a browser sends for a double click: two mousedown, mouseup,
click runs at the same point, the second with `detail: 2`, a task apart so
React has drawn the ask in between. A separate click on the ask (`detail` 1),
however soon, is an answer: `worktrees/delete-removes-clean-worktree` clicks
the choice right away and must still delete.

```sh
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{plain}}","text":"TEST-keep me after a double click"}' --wait '{"op":"event","event":{"type":"turn_completed"}}' --save dbl_plain=sessionId
yv drive 'session:{{dbl_plain}}' --steps '[{"expect": {"role": "button", "name": "Delete", "exact": true}}, {"eval": "(async () => { const b = [...document.querySelectorAll(\".main header.bar button\")].find((x) => x.textContent.trim() === \"Delete\"); const r = b.getBoundingClientRect(); const x = r.left + r.width / 2, y = r.top + r.height / 2; const click = (el, detail) => { for (const type of [\"mousedown\", \"mouseup\", \"click\"]) el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window, button: 0, clientX: x, clientY: y, detail })); }; click(b, 1); await new Promise((res) => setTimeout(res, 0)); const second = document.elementFromPoint(x, y).closest(\"button\"); const label = second ? second.textContent.trim() : \"none\"; second && click(second, 2); await new Promise((res) => setTimeout(res, 800)); return \"second click hit: \" + label; })()"}, {"expect": {"css": ".main header.bar .bar-armed"}}, {"shot": "double-click-delete"}]'
yv state --expect '{"sessions":[{"id":"{{dbl_plain}}"}]}'
yv api '{"op":"session.create","engine":"codex","model":"fake-fast","effort":"low","cwd":"{{repo}}","worktree":true,"text":"TEST-keep my worktree after a double click"}' --wait '{"op":"event","event":{"type":"turn_completed"}}' --save dbl_wt=sessionId
yv drive 'session:{{dbl_wt}}' --steps '[{"expect": {"role": "button", "name": "Delete", "exact": true}}, {"eval": "(async () => { const b = [...document.querySelectorAll(\".main header.bar button\")].find((x) => x.textContent.trim() === \"Delete\"); const r = b.getBoundingClientRect(); const x = r.left + r.width / 2, y = r.top + r.height / 2; const click = (el, detail) => { for (const type of [\"mousedown\", \"mouseup\", \"click\"]) el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window, button: 0, clientX: x, clientY: y, detail })); }; click(b, 1); await new Promise((res) => setTimeout(res, 0)); const second = document.elementFromPoint(x, y).closest(\"button\"); const label = second ? second.textContent.trim() : \"none\"; second && click(second, 2); await new Promise((res) => setTimeout(res, 800)); return \"second click hit: \" + label; })()"}, {"expect": {"css": ".main header.bar .bar-armed"}}, {"shot": "double-click-delete"}]'
yv state --expect '{"sessions":[{"id":"{{dbl_wt}}"}]}'
```

### send-stays-at-the-right
At 390 px the reply box's tools (attach, dictate, voice mode, model, permission)
fill the row, so Send moves to a second line. It stays at the right edge of
the box there, where it sits on one line and on the New session box.

```sh
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"expect":"label:Message"},{"eval":"(() => { const row = document.querySelector(\".composer-row\").getBoundingClientRect(); const send = document.querySelector(\".composer-row .send\").getBoundingClientRect(); return Math.round(row.right - send.right); })()","equals":0},{"shot":"reply-send"}]'
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"expect":"label:Message"},{"eval":"(() => { const row = document.querySelector(\".composer-row\").getBoundingClientRect(); const send = document.querySelector(\".composer-row .send\").getBoundingClientRect(); return Math.round(row.right - send.right); })()","equals":0}]'
yv drive new --steps '[{"expect":"label:Message"},{"eval":"(() => { const row = document.querySelector(\".composer-row\").getBoundingClientRect(); const send = document.querySelector(\".composer-row .send\").getBoundingClientRect(); return Math.round(row.right - send.right); })()","equals":0}]'
```

### new-session-send-shares-controls-row
Viewports: desktop
Send stays next to the composer controls as the New session box narrows.

```sh
yv drive new --viewport desktop --steps '[{"expect":"label:Message"},{"expect":"role:button:Send"},{"eval":"(async () => { const inner = document.querySelector(\".new-inner\"); const row = inner.querySelector(\".composer-row\"); const send = row.querySelector(\".send\"); for (let width = 300; width <= 580; width += 2) { inner.style.maxWidth = width + \"px\"; await new Promise(requestAnimationFrame); const lastControl = Math.max(...[...row.querySelectorAll(\"button:not(.send)\")].filter(b => b.getClientRects().length).map(b => b.getBoundingClientRect().bottom)); if (send.getBoundingClientRect().top >= lastControl) return {width}; } return true; })()","equals":true},{"shot":"composer-controls"}]'
```

### desktop-sidebar-is-a-column
Viewports: desktop
At 1440 px nothing changes: no menu button, and the sidebar is a 248 px column
beside the session, not a drawer.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"expect":"role:button:Open sessions","visible":false},{"expect":"role:navigation:Sessions","inViewport":true},{"eval":"[Math.round(document.querySelector(\".sidebar\").getBoundingClientRect().width), getComputedStyle(document.querySelector(\".sidebar\")).position, Math.round(document.querySelector(\".main\").getBoundingClientRect().left)]","equals":[248,"static",248]}]'
```

### workspace-tabs-fit-minimum-width
Viewports: desktop
At the minimum 300 px pane width, the header stays one row tall, its labels
stay on one line, and every control stays inside the pane. Both views
are directly selectable.

```sh
yv drive 'session:{{sid_edit}}' --viewport desktop --steps '[{"eval":"localStorage.setItem(\"yard.paneWidth\",\"300\") ?? true"},{"reload":true},{"expect":"role:region:Workspace"},{"shot":"minimum-pane"},{"eval":"(() => { const bar=document.querySelector(\".pane .tabs\").getBoundingClientRect(); const buttons=[...document.querySelectorAll(\".pane .tabs button\")].filter(b=>b.getClientRects().length); return Math.round(bar.height)===52 && buttons.every(b=>{const box=b.getBoundingClientRect();const r=document.createRange();r.selectNodeContents(b);return new Set([...r.getClientRects()].filter(x=>x.width>0).map(x=>Math.round(x.top))).size<=1 && box.left>=bar.left && box.right<=bar.right && box.bottom<=bar.bottom;}); })()","equals":true},{"expect":"role:button:Terminal","inViewport":true},{"expect":"role:button:Subagents","inViewport":true},{"expect":"role:button:Processes","inViewport":true},{"click":"role:button:Processes"},{"expect":"role:button:Refresh processes","inViewport":true}]'
```

## Gotchas
- Below 700 px the sidebar is a closed drawer, so a recipe that clicks or reads something in it at mobile opens it first (`{"click":"role:button:Open sessions"}`); `yv drive`'s routes do that themselves. At both viewports with one step list, split the recipe per viewport: the menu button is hidden on desktop.
- On desktop, `yv drive`'s routes and `goto` steps reach a page by clicking the sidebar, and they do not expand a collapsed one: after `{"click":"role:button:Collapse sessions sidebar"}` a `goto` times out (`goto hosts` waited 30 s on `.host-row`, 2026-11-20). Expand it first (`{"click":"role:button:Expand sessions sidebar"}`), or change pages with `{"eval":"(location.hash=\"#/hosts\", true)"}`. The collapse is kept in `localStorage` (`yard.sidebarCollapsed`), so a recipe that collapses it expands it again before it ends.
- Fixed 2026-11-16 (#8): there was no layout below ~700 px; the 248 px sidebar left the session column about 140 px wide at 390 px.
- The sidebar and drawer toggles move focus in a `requestAnimationFrame` after the click. An `eval` of `document.activeElement` straight after a step that is already satisfied (such as `{"expect":"role:region:Workspace","visible":false}` while it is hidden) can run before that frame. `desktop-sidebars-collapse-independently` failed 2 of 3 map runs that way on 2026-11-21, and `mobile-panels-have-close-controls` once (`activeElement` was not yet the button), while focus did land in every probe. Wait for focus instead: `{"expect":"css:button[aria-label=\"…\"]:focus"}`.
- Open 2026-11-22 (#249, owner call): at 390 px the session header's branch chip shrinks to "ya…" (the shared `yard/` prefix) because `.bar code` may shrink with no minimum width. No recipe yet: the fix (hide the chip on phones, a minimum width, or show the end of the branch) is the owner's choice, and the recipe follows that choice.
