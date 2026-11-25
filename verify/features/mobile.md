# Mobile app

The iOS app is native (mobile/README.md) and opens the host's web page for a
session's workspace. On the web: setup is always on Hosts; the phone shortcut
appears in session headers until an actual phone connection is recorded by the
hub. The workspace tabs remain accessible on narrow screens. The native screens
are checked by the iOS workflow's simulator tests and demo screenshots, not here.

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

### setup-entry-points
Viewports: both
The shortcut is present on new and existing sessions. It opens the persistent
Hosts card, with no false TestFlight URL before the beta has an invitation.

```sh
yv drive new --steps '[{"expect":"role:button:Set up iPhone app"},{"click":"role:button:Set up iPhone app"},{"expect":"role:region:iPhone app","text":"TestFlight beta"},{"expect":"role:region:iPhone app","text":"first build is available"},{"fill":"label:Enter your host’s HTTPS address in the iPhone app.","value":"https://yard.test"},{"expect":"role:link:Open in Switchyard on this iPhone"},{"fill":"label:Enter your host’s HTTPS address in the iPhone app.","value":"http://localhost:47717"},{"expect":"role:link:Open in Switchyard on this iPhone","count":0},{"expect":"css:.mobile-setup","text":"Localhost points to the phone itself"},{"shot":"setup"}]'
yv drive 'session:{{sid_echo}}' --steps '[{"expect":"role:button:Set up iPhone app"},{"shot":"session-shortcut"}]'
```

### phone-workspace
Viewports: mobile
All workspace tabs can be reached and the conversation can be reopened without
losing its composer. The terminal starts only when opened.

```sh
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"fill":"label:Message","value":"TEST-draft on phone"},{"click":"role:button:Workspace"},{"expect":"role:region:Workspace"},{"expect":"css:.workspace-tabs [aria-pressed=true]","text":"Terminal"},{"expect":"css:.xterm"},{"click":"role:button:Processes"},{"expect":"role:button:Refresh processes"},{"click":"role:button:Back to conversation"},{"expect":"label:Message","value":"TEST-draft on phone"},{"shot":"conversation-restored"}]'
```

### workspace-button-text-centered
Viewports: mobile
The session header's Workspace button reads on the same line as Delete. Its
text used to sit at the top of the button, 6 px above Delete, because the
button is a flex box with no vertical centring.

```sh
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"expect":"role:button:Workspace"},{"eval":"const mid = (s) => { const b = document.querySelector(s); const r = document.createRange(); r.selectNodeContents(b); const t = r.getBoundingClientRect(); return t.top + t.height / 2; }; Math.abs(mid(\".workspace-toggle\") - mid(\".bar-choice\")) <= 1","equals":true},{"shot":"header"}]'
```

### successful-connection-removes-shortcuts
Viewports: both
A native connection is persisted by the hub. Reloading and reopening another
session retains setup status; Hosts keeps its card for additional devices.

```sh
yv api '{"op":"mobile.connected"}' --wait '{"op":"mobile.status","mobile":{"configured":true}}'
yv drive new --steps '[{"expect":"role:button:Set up iPhone app","count":0}]'
yv drive 'session:{{sid_echo}}' --steps '[{"expect":"role:button:Set up iPhone app","count":0},{"reload":true},{"expect":"css:.thread"},{"expect":"role:button:Set up iPhone app","count":0},{"shot":"configured-session"}]'
yv drive hosts --steps '[{"expect":"role:region:iPhone app","text":"Connected"},{"shot":"configured-host"}]'
```

### native-device (manual: requires a signed build and physical iPhone)
Follow `mobile/README.md` for device voice, permissions, attachments, lifecycle,
network and TestFlight checks. Swift unit tests cover host and deep-link
validation, the thread built from recorded yardd messages, and the voice rules.

## Gotchas

- No push delivery while the app is suspended; no APNs service exists.
- The TestFlight invitation stays pending until Apple signing and beta setup
  are completed. A simulator artifact cannot be installed with TestFlight.
