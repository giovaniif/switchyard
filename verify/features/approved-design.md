# Approved Switchyard design

The conversation workspace contains Terminal, Subagents and Processes. Any pull request
opens on GitHub from the conversation header. The composer has one toolbar row.
Connectors are grouped by account, and Usage starts with one compact summary.

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

### terminal-first-and-external-pr
Viewports: desktop
The Workspace contains exactly three tabs and opens on Terminal. The branch's PR
opens externally from the conversation header.

```sh
yv drive 'session:{{sid_edit}}' --viewport desktop --steps '[{"expect":"css:.workspace-tabs .tab","count":3},{"expect":{"css":".workspace-tabs [aria-pressed=true]","hasText":"Terminal"}},{"expect":{"css":".thread .bar a[target=_blank]","hasText":"#4242"}},{"expect":"role:button:Subagents"},{"click":"role:button:Processes"},{"expect":"role:button:Refresh processes"},{"shot":"workspace"}]'
```

### one-row-composers
Viewports: both
Attachments, dictation, voice, session settings and Send fit on a single row.
On phones, the setup button opens all the settings.

```sh
yv drive new --steps '[{"expect":"role:button:Attach files"},{"expect":"role:button:Voice mode"},{"eval":"(() => { const row=document.querySelector(\".composer-row\"); const a=row.querySelector(\".attach-btn\").getBoundingClientRect(); const b=row.querySelector(\".send\").getBoundingClientRect(); return Math.abs((a.top+a.bottom)/2-(b.top+b.bottom)/2) < 2 && b.right<=innerWidth; })()","equals":true},{"shot":"new-composer"}]'
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"click":"role:button:Session setup"},{"expect":"role:dialog:Session setup"},{"expect":"role:button:Ask first"},{"press":"Escape"},{"shot":"reply-composer"}]'
```

### account-card-separation
Viewports: both
A host's account cards are visible together and their connector lists stay
within their respective cards.

```sh
yv drive connectors --steps '[{"expect":{"css":".connectors-card","hasText":"Claude"}},{"expect":{"css":".connectors-card","hasText":"Codex"}},{"expect":"css:.connectors-card","count":2},{"shot":"per-account-connectors"}]'
```

### compact-usage
Viewports: desktop
One account is shown first and other accounts can be expanded.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Subscription usage"},{"expect":"role:dialog:Subscription usage"},{"expect":"role:button:Other accounts"},{"click":"role:button:Other accounts"},{"expect":"css:.compact-subscription"},{"click":"role:button:Refresh usage"},{"expect":"role:button:Refresh usage","enabled":true},{"shot":"usage"}]'
```

### phone-usage-opens-from-header
Viewports: mobile
The phone header opens the compact usage sheet without opening the sidebar.

```sh
yv drive new --viewport mobile --steps '[{"click":"css:.new-bar .mobile-usage-trigger"},{"expect":"role:dialog:Subscription usage","inViewport":true},{"expect":"css:.app.drawer-open","count":0},{"press":"Escape"},{"expect":"role:dialog:Subscription usage","count":0}]'
```


### usage-popover-follows-prototype
Viewports: both
Usage opens above the rail row on desktop and in a full-width bottom sheet
on phones. The account name, engine and limit details have distinct rows.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Subscription usage"},{"shot":"usage-desktop"},{"eval":"(() => { const p = document.querySelector('"'"'.usage-popover'"'"').getBoundingClientRect(); const a = document.querySelector('"'"'.usage-trigger'"'"').getBoundingClientRect(); return [p.left - a.left, a.top - p.bottom, p.width].map(Math.round).join(); })()","equals":"0,6,320"},{"expect":"css:.usage-account-line","text":"Codex"},{"expect":"role:dialog:Subscription usage","text":"5-hour window"},{"expect":"role:dialog:Subscription usage","text":"12% used"},{"expect":"css:.usage-reset","text":"Resets"},{"expect":"css:.usage-other-account","count":0},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude \u00b7 Default","text":"47%"},{"click":"role:button:Refresh usage"},{"expect":"role:status","text":"Updated at"},{"click":"role:button:Manage accounts"},{"expect":"role:heading:Connectors"}]'
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Subscription usage"},{"shot":"usage-phone"},{"eval":"(() => { const p = document.querySelector('"'"'.usage-popover'"'"').getBoundingClientRect(); return [p.left, p.right - innerWidth, p.bottom - innerHeight].map(Math.round).join(); })()","equals":"0,0,0"},{"expect":"role:dialog:Subscription usage","text":"5-hour window"},{"expect":"role:button:Close usage popover"},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude \u00b7 Default","text":"47%"},{"shot":"usage-phone-expanded"},{"press":"Escape"},{"expect":"role:dialog:Subscription usage","count":0}]'
```

### hosts-stays-in-host-status-and-page-search
Viewports: desktop
The top menu has no duplicate Hosts entry. The host status rows and page
search still open the Hosts page.

```sh
yv drive new --viewport desktop --steps '[{"expect":{"css":".sidebar-top button","hasText":"Hosts","exact":true},"count":0},{"expect":{"css":".sidebar-top button","hasText":"Automations","exact":true}},{"click":"css:.host-row"},{"expect":"role:heading:Hosts"},{"click":"role:button:Go to session or page"},{"fill":"role:combobox","value":"hosts"},{"press":"Enter"},{"expect":"role:heading:Hosts"}]'
```

### toolbar-at-320-with-full-access
Viewports: mobile
The narrowest phone keeps a single toolbar row and a visible warning state.
Its settings sheet and nested permission menu stay on screen.

```sh
yv drive new --viewport mobile --steps '[{"resize":{"width":320,"height":568}},{"click":"role:button:Session setup"},{"expect":"role:dialog:Session setup","inViewport":true},{"click":{"role":"button","name":"Ask first","exact":true,"within":"role:dialog:Session setup"}},{"expect":"role:dialog:Permissions","inViewport":true},{"click":"role:menuitemradio:Full access"},{"press":"Escape"},{"expect":"css:.composer-setup.permission-full"},{"eval":"(() => {const row=document.querySelector(\".composer-row\"); const a=row.querySelector(\".attach-btn\").getBoundingClientRect(); const s=row.querySelector(\".send\").getBoundingClientRect(); return document.documentElement.scrollWidth <= document.documentElement.clientWidth && (Math.abs(a.top+a.bottom-s.top-s.bottom) < 3) && s.right <= innerWidth;})()","equals":true},{"resize":{"width":390,"height":844}},{"shot":"narrow-full-access"}]'
```

### toolbar-groups-before-labels-compress
Viewports: desktop
A composer wide enough for its usual selectors can still need Setup when
model names are longer.

```sh
yv drive new --viewport desktop --steps '[{"expect":"css:.composer-row .model-trigger"},{"eval":"(() => { const style = document.createElement(\"style\"); style.textContent = \".session-toolbar .picker > button { font-size: 32px; }\"; document.head.append(style); return true; })()","equals":true},{"expect":"css:.composer-row .composer-setup"},{"expect":"css:.composer-row .model-trigger","count":0},{"click":"role:button:Session setup"},{"expect":"role:dialog:Session setup"},{"shot":"uncompressed-selectors"}]'
```

### codex-speed-in-model-menu
Viewports: both
The Codex model menu offers reported speed tiers. An existing session keeps its selection.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Model Fake Fast"},{"click":"role:button:Speed Standard"},{"expect":"role:menuitemradio:Standard"},{"expect":"role:menuitemradio:Fast"},{"expect":"role:menuitemradio:Ultrafast"},{"shot":"codex-speeds"},{"click":"role:menuitemradio:Ultrafast"},{"expect":"role:button:Speed Ultrafast"},{"reload":true},{"click":"role:button:Model Fake Fast"},{"expect":"role:button:Speed Ultrafast"}]'
yv drive 'session:{{sid_edit}}' --viewport mobile --steps '[{"click":"role:button:Session setup"},{"click":"role:button:Model Fake Fast"},{"click":"role:button:Speed Standard"},{"expect":"role:menuitemradio:Standard"},{"expect":"role:menuitemradio:Fast"},{"expect":"role:menuitemradio:Ultrafast"},{"shot":"codex-speeds"},{"click":"role:menuitemradio:Ultrafast"},{"expect":"role:button:Speed Ultrafast"}]'
```
