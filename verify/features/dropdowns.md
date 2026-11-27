# Dropdown layout

Menus keep their options readable and reachable inside the viewport.

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

### composer-options-fit
Viewports: both
Descriptions fit their own rows, arrows move focus, and Speed uses the model panel.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Effort Low"},{"eval":"[...document.querySelectorAll('"'"'[data-anchored-panel]'"'"')].every(p => { const r=p.getBoundingClientRect(); return r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight; })","equals":true},{"eval":"[...document.querySelectorAll('"'"'.effort-menu .picker-item'"'"')].every(b => { const r=b.getBoundingClientRect(); const d=b.querySelector('"'"'small'"'"')?.getBoundingClientRect(); return r.height>=44 && (!d || d.bottom<=r.bottom-4); })","equals":true},{"shot":"effort"},{"press":"ArrowDown"},{"eval":"document.activeElement.querySelector('"'"'span'"'"').textContent","equals":"High"},{"press":"Escape"},{"click":"role:button:Model Fake Fast"},{"click":"role:button:Speed Standard"},{"expect":"role:dialog:Codex speed","inViewport":true},{"eval":"[...document.querySelectorAll('"'"'[data-anchored-panel]'"'"')].every(p => { const r=p.getBoundingClientRect(); return r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight; })","equals":true},{"eval":"[...document.querySelectorAll('"'"'.effort-menu .picker-item'"'"')].every(b => { const r=b.getBoundingClientRect(); const d=b.querySelector('"'"'small'"'"')?.getBoundingClientRect(); return r.height>=44 && (!d || d.bottom<=r.bottom-4); })","equals":true},{"eval":"[...document.querySelectorAll('"'"'[data-anchored-panel]'"'"')].filter(p=>p.getAttribute('"'"'aria-label'"'"')==='"'"'Model'"'"').length","equals":0},{"shot":"speed"},{"press":"Escape"},{"expect":"role:dialog:Model","inViewport":true},{"shot":"model"}]'
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"resize":{"width":320,"height":568}},{"click":"role:button:Session setup"},{"click":"role:button:Effort Low"},{"eval":"[...document.querySelectorAll('"'"'[data-anchored-panel]'"'"')].every(p => { const r=p.getBoundingClientRect(); return r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight; })","equals":true},{"eval":"[...document.querySelectorAll('"'"'.effort-menu .picker-item'"'"')].every(b => { const r=b.getBoundingClientRect(); const d=b.querySelector('"'"'small'"'"')?.getBoundingClientRect(); return r.height>=44 && (!d || d.bottom<=r.bottom-4); })","equals":true},{"resize":{"width":390,"height":844}},{"shot":"effort"},{"resize":{"width":320,"height":568}},{"press":"ArrowDown"},{"eval":"document.activeElement.querySelector('"'"'span'"'"').textContent","equals":"High"},{"press":"Escape"},{"click":"role:button:Model Fake Fast"},{"click":"role:button:Speed Standard"},{"expect":"role:dialog:Codex speed","inViewport":true},{"eval":"[...document.querySelectorAll('"'"'[data-anchored-panel]'"'"')].every(p => { const r=p.getBoundingClientRect(); return r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight; })","equals":true},{"eval":"[...document.querySelectorAll('"'"'.effort-menu .picker-item'"'"')].every(b => { const r=b.getBoundingClientRect(); const d=b.querySelector('"'"'small'"'"')?.getBoundingClientRect(); return r.height>=44 && (!d || d.bottom<=r.bottom-4); })","equals":true},{"eval":"[...document.querySelectorAll('"'"'[data-anchored-panel]'"'"')].filter(p=>p.getAttribute('"'"'aria-label'"'"')==='"'"'Model'"'"').length","equals":0},{"resize":{"width":390,"height":844}},{"shot":"speed"},{"resize":{"width":320,"height":568}},{"press":"Escape"},{"expect":"role:dialog:Model","inViewport":true},{"resize":{"width":390,"height":844}},{"shot":"model"},{"resize":{"width":320,"height":568}}]'
```

### workspace-tab-spacing
Viewports: both
Workspace tabs have separate hit areas and fit alongside the refresh action.

```sh
yv drive 'session:{{sid_edit}}' --viewport desktop --steps '[{"eval":"localStorage.setItem('"'"'yard.paneWidth'"'"','"'"'300'"'"') ?? true"},{"reload":true},{"expect":"role:button:Subagents","inViewport":true},{"eval":"(() => { const h=document.querySelector('"'"'.workspace-tabs'"'"'); const r=h.getBoundingClientRect(); const tabs=[...h.querySelectorAll('"'"'.tab'"'"')].map(b=>b.getBoundingClientRect()); return tabs.every((b,i)=>b.left>=r.left && b.right<=r.right && b.top===tabs[0].top && (!i || b.left-tabs[i-1].right>=4)); })()","equals":true},{"shot":"workspace-tabs"},{"click":"role:button:Processes"},{"expect":"role:button:Refresh processes","inViewport":true}]'
yv drive 'session:{{sid_edit}}' --viewport mobile --steps '[{"resize":{"width":320,"height":568}},{"click":"role:button:Workspace"},{"expect":"role:button:Subagents","inViewport":true},{"eval":"(() => { const h=document.querySelector('"'"'.workspace-tabs'"'"'); const r=h.getBoundingClientRect(); const tabs=[...h.querySelectorAll('"'"'.tab'"'"')].map(b=>b.getBoundingClientRect()); return tabs.every((b,i)=>b.left>=r.left && b.right<=r.right && b.top===tabs[0].top && (!i || b.left-tabs[i-1].right>=4)); })()","equals":true},{"resize":{"width":390,"height":844}},{"shot":"workspace-tabs"},{"resize":{"width":320,"height":568}},{"click":"role:button:Processes"},{"expect":"role:button:Refresh processes","inViewport":true}]'
```

### custom-schedule-options
Viewports: both
Scheduling uses app menus for the schedule and its fields.

```sh
yv drive 'new' --viewport desktop --steps '[{"click":"role:button:Automations"},{"click":"role:button:New automation"},{"expect":"css:select","count":0},{"click":"role:button:Schedule type"},{"click":"role:menuitemradio:Weekly"},{"click":"role:button:Day of the week"},{"expect":"role:menuitemradio:Friday","inViewport":true},{"eval":"[...document.querySelectorAll('"'"'[data-anchored-panel]'"'"')].every(p => { const r=p.getBoundingClientRect(); return r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight; })","equals":true},{"shot":"day-options"},{"click":"role:menuitemradio:Friday"},{"expect":"role:button:Day of the week","text":"Friday"}]'
yv drive 'new' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Automations"},{"click":"role:button:New automation"},{"expect":"css:select","count":0},{"click":"role:button:Schedule type"},{"click":"role:menuitemradio:Weekly"},{"click":"role:button:Day of the week"},{"expect":"role:menuitemradio:Friday","inViewport":true},{"eval":"[...document.querySelectorAll('"'"'[data-anchored-panel]'"'"')].every(p => { const r=p.getBoundingClientRect(); return r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight; })","equals":true},{"shot":"day-options"},{"click":"role:menuitemradio:Friday"},{"expect":"role:button:Day of the week","text":"Friday"}]'
```

### connector-host-and-refresh
Viewports: both
The connector host list uses an app menu, next to Check again (an icon button since the page became one card per service).

```sh
yv api '{"op":"host.add","name":"TEST-edge host","kind":"direct","url":"ws://127.0.0.1:9/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-edge host"}]}'
yv drive connectors --viewport desktop --steps '[{"expect":"role:heading:Connectors"},{"expect":"css:select","count":0},{"expect":"role:button:Check again"},{"eval":"(() => { const h=document.querySelector('"'"'.cx-head [aria-label=\"Connector host\"]'"'"').getBoundingClientRect(), r=document.querySelector('"'"'.cx-head .cx-refresh'"'"').getBoundingClientRect(); return r.left>=h.right && Math.abs(r.top+r.height/2-h.top-h.height/2)<=2; })()","equals":true},{"click":"role:button:Connector host"},{"expect":"role:dialog:Connector host","inViewport":true},{"expect":"role:menuitemradio:TEST-edge host"},{"shot":"connector-host-menu"},{"click":"role:menuitemradio:TEST-edge host"},{"expect":"role:button:Connector host","text":"TEST-edge host"}]'
yv drive connectors --viewport mobile --steps '[{"expect":"role:heading:Connectors"},{"expect":"css:select","count":0},{"click":"role:button:Connector host"},{"expect":"role:dialog:Connector host","inViewport":true},{"expect":"role:menuitemradio:TEST-edge host"},{"shot":"connector-host-menu"},{"click":"role:menuitemradio:TEST-edge host"},{"expect":"role:button:Connector host","text":"TEST-edge host"}]'
```

### handover-nests-the-new-session
Viewports: both
Handover can use another account or model under the same agent. The new
session appears below its parent in the sidebar.

```sh
yv api '{"op":"account.add","engine":"codex","name":"TEST-Handover"}' --wait '{"op":"accounts"}'
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Actions for TEST-say hello"},{"click":"role:menuitem:Handover"},{"click":{"role":"button","name":"Account","exact":true,"within":"role:dialog:Handover session"}},{"click":"role:menuitemradio:TEST-Handover"},{"expect":"role:dialog:Handover session","inViewport":true},{"shot":"handover"},{"click":{"role":"button","name":"Handover","exact":true}},{"expect":"css:.thread","text":"Handed off from Codex"},{"expect":{"role":"group","name":"Handovers from TEST-say hello"},"text":"TEST-Handover"},{"shot":"subsession"}]'
yv api '{"op":"session.update","sessionId":"{{sid_edit}}","archived":true}' --wait '{"op":"session","session":{"id":"{{sid_edit}}","archived":true}}'
yv drive 'session:{{sid_edit}}' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Actions for [edit] TEST-fix the greeting"},{"click":"role:menuitem:Handover"},{"click":{"role":"button","name":"Model","exact":true,"within":"role:dialog:Handover session"}},{"click":"role:menuitemradio:Fake Deep"},{"expect":"role:dialog:Handover session","inViewport":true},{"shot":"handover"},{"click":{"role":"button","name":"Handover","exact":true}},{"expect":"css:.thread","text":"Handed off from Codex"},{"click":"role:button:Open sessions"},{"expect":{"role":"group","name":"Handovers from [edit] TEST-fix the greeting"},"text":"Fake Deep"},{"wait":240},{"shot":"subsession"}]'
```
