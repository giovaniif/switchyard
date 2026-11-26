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
Hosts card, which offers the public TestFlight beta with no configuration.
This host lists no address a phone can open, so the card asks for one; a
valid one gives a QR code on a desktop and an Open link on a phone, and
localhost (the phone itself) gives neither.

```sh
yv drive new --viewport desktop --steps '[{"expect":"role:button:Set up iPhone app"},{"click":"role:button:Set up iPhone app"},{"expect":"role:status","text":"No phone connected yet"},{"expect":"role:link:Install with TestFlight"},{"eval":"document.querySelector(\".mobile-setup .btn-primary\").getAttribute(\"href\")","equals":"https://testflight.apple.com/join/BygX29JY"},{"fill":"label:Address the phone can reach","value":"https://yard.test"},{"expect":"css:.phone-qr-code"},{"eval":"document.querySelector(\".phone-qr-code\").getAttribute(\"aria-label\")","equals":"QR code that opens Switchyard on your iPhone with yard.test"},{"fill":"label:Address the phone can reach","value":"http://localhost:47717"},{"expect":"css:.phone-qr-code","count":0},{"expect":"css:.mobile-setup .warn","text":"point to the phone itself"},{"shot":"setup"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Set up iPhone app"},{"fill":"label:Address the phone can reach","value":"https://yard.test"},{"expect":"role:link:Open in Switchyard"},{"eval":"document.querySelector(\".mobile-open\").getAttribute(\"href\")","equals":"switchyard://connect?host=https%3A%2F%2Fyard.test"},{"shot":"setup"}]'
yv drive 'session:{{sid_echo}}' --steps '[{"expect":"role:button:Set up iPhone app"},{"shot":"session-shortcut"}]'
```

### qr-for-the-hosts-address
Viewports: both
The desktop app's page is on loopback, which a phone cannot open, so the card
uses the HTTPS address the host serves its page at (`YARD_ALLOW_ORIGIN`): a
QR code, the address with Copy, and a TestFlight button when the invitation is
set. A phone whose app is connected shows as connected now; once it leaves,
as last seen. It used to offer an empty box and a `switchyard://` link that
does nothing on a desktop, and said nothing about the phone.

```sh
yv iso up --name phoneaddr --env "YARD_ALLOW_ORIGIN=https://switchyard.example.ts.net,YARD_TESTFLIGHT_URL=https://testflight.apple.com/join/AbCd1234"
yv drive hosts --iso phoneaddr --viewport desktop --steps '[{"expect":"role:link:Install with TestFlight"},{"expect":"css:.phone-qr-code"},{"eval":"document.querySelector(\".phone-qr-code\").getAttribute(\"aria-label\")","equals":"QR code that opens Switchyard on your iPhone with switchyard.example.ts.net"},{"eval":"document.querySelector(\".phone-qr-code\").dataset.href","equals":"https://switchyard.example.ts.net/?phone&testflight=https%3A%2F%2Ftestflight.apple.com%2Fjoin%2FAbCd1234"},{"expect":"css:.phone-address code","text":"https://switchyard.example.ts.net"},{"expect":"role:status","text":"No phone connected yet"},{"eval":"(window.__phone = new WebSocket(location.origin.replace(\"http\",\"ws\") + \"/ws\"), window.__phone.onopen = () => window.__phone.send(JSON.stringify({op:\"mobile.connected\",device:\"TEST-iPhone\"})), 1)"},{"expect":"role:status","text":"TEST-iPhone connected now"},{"expect":"role:button:Set up iPhone app","count":0},{"shot":"phone-live"},{"eval":"(window.__phone.close(), 1)"},{"expect":"role:status","text":"TEST-iPhone · last seen just now"}]'
yv drive hosts --iso phoneaddr --viewport mobile --steps '[{"expect":"role:link:Open in Switchyard"},{"expect":"css:.phone-address code","text":"https://switchyard.example.ts.net"},{"shot":"phone-address"}]'
yv iso down --name phoneaddr
```

### qr-opens-a-page-safari-can-show
Viewports: mobile
The QR code is the host's own `https://…/?phone` page, because the iPhone
Camera offers nothing for a `switchyard://` code while the app is not
installed: the first scan looked broken. That page opens no socket; it offers
Open in Switchyard (HTTPS hosts only, so here a warning), TestFlight (the
invitation when the hub has one, otherwise how to get it), and the web UI.

```sh
yv drive hosts --viewport mobile --steps '[{"eval":"(location.href = \"/?phone&testflight=https://testflight.apple.com/join/AbCd1234\", 1)"},{"expect":"role:region:Switchyard for iPhone"},{"eval":"document.querySelector(\".phone-welcome .btn\").getAttribute(\"href\")","equals":"https://testflight.apple.com/join/AbCd1234"},{"expect":"css:.phone-welcome .warn","text":"not on an HTTPS address"},{"shot":"phone-page"},{"eval":"(location.href = \"/?phone&testflight=javascript:alert(1)\", 1)"},{"expect":"role:link:Install with TestFlight","count":0},{"expect":"role:link:Get TestFlight"},{"expect":"css:.phone-welcome","text":"invitation Apple emailed you"},{"shot":"phone-page-no-invite"},{"click":"role:link:Use Switchyard in Safari instead"},{"expect":"role:button:Set up iPhone app"}]'
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
yv drive hosts --steps '[{"expect":"role:status","text":"iPhone · last seen"},{"shot":"configured-host"}]'
```

### native-device (manual: requires a signed build and physical iPhone)
Follow `mobile/README.md` for device voice, permissions, attachments, lifecycle,
network and TestFlight checks. Swift unit tests cover host and deep-link
validation, the thread built from recorded yardd messages, and the voice rules.

## Gotchas

- No push delivery while the app is suspended; no APNs service exists.
- The TestFlight invitation stays pending until Apple signing and beta setup
  are completed. A simulator artifact cannot be installed with TestFlight.
