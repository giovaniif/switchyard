# Engine logins

yardd reports each engine as installed or not and logged in or not
(`engines.status`). A logged-out engine shows a notice with **Log in** on the
new-session page and on the Hosts page. Codex logs in by device code on the
shared app-server; Claude Code runs `claude auth login` in a PTY and takes the
pasted code. Grok runs `grok login --device-auth` in a PTY and shows its link
and code. OpenCode works with its free models alone, so it is never "logged
out": its **Add provider** runs a short-lived `opencode serve` and offers its
providers, then an API key or the provider's sign-in. This area starts with
the fakes logged out.

## Setup

```sh
yv iso up --logged-out
yv iso seed --profile empty
```

## Cleanup

```sh
yv iso down
```

## Recipes

### hosts-page-shows-logged-out
Viewports: both
Engine logins lists each engine as not logged in, with its own Log in action.

```sh
yv drive 'hosts' --steps '[{"click":"role:tab:Engine logins"},{"expect":{"css":".engine-row","hasText":"Claude Code"},"text":"not logged in"},{"expect":{"css":".engine-row","hasText":"Codex"},"text":"not logged in"},{"expect":"role:button:Log in"},{"shot":"logged-out"}]'
```

### usage-trigger-skips-signed-out-account
Viewports: both
The sidebar's Usage row (desktop) and the phone's usage pill show no percent
for an account that needs sign-in, matching the popover's "Sign in to see
usage". The fake Claude reports 47% when it starts, even logged out, like a
real account whose last report outlived its sign-in. Until 2026-11-21 the
trigger read "47% used" while the popover had no number to show. Keep it
before the recipes that log Claude Code in: after them 47% is right.

```sh
yv drive new --viewport desktop --steps '[{"wait":2000},{"expect":"css:.usage-trigger","text":"View limits"},{"click":"css:.usage-trigger"},{"expect":"css:.usage-popover","text":"Sign in to see usage"},{"shot":"usage-signed-out"}]'
yv drive new --viewport mobile --steps '[{"wait":2000},{"eval":"!document.querySelector(\".mobile-usage-trigger\").textContent.includes(\"%\")","equals":true},{"click":"css:.mobile-usage-trigger"},{"expect":"css:.usage-popover","text":"Sign in to see usage"},{"shot":"usage-signed-out"}]'
```

### new-session-log-in-fits
Viewports: both
The new-session page's notice for a logged-out engine keeps **Log in** on one
line inside its button, even at phone width where the message wraps.

```sh
yv drive new --steps '[{"expect":"css:.engine-notice","text":"isn\u0027t logged in"},{"eval":"(() => { const b = [...document.querySelectorAll(\".engine-notice button\")].find(x => x.textContent.trim() === \"Log in\"); const r = document.createRange(); r.selectNodeContents(b); const lines = new Set([...r.getClientRects()].map(x => Math.round(x.top))).size; return lines + \",\" + (b.scrollHeight <= b.clientHeight); })()","equals":"1,true"},{"shot":"new-session-notice"}]'
```

### closing-a-login-cancels-it
Viewports: desktop
Closing the Claude Code login dialog part way (Close, then Escape) cancels
the login (`login.cancel`): the dialog goes, Claude Code still reads "not
logged in", Log in again opens a fresh sign-in with an empty code field (not
the half-typed one), and no `claude auth login` is left running under yardd.
Keep it before claude-code-login, which logs Claude Code in.

```sh
yv drive hosts --viewport desktop --steps '[{"click":"role:tab:Engine logins"},{"click":{"role":"button","name":"Log in","nth":0}},{"expect":"role:dialog:Log in to Claude Code","text":"Open the sign-in page","timeout":15000},{"fill":"label:Code from the sign-in page","value":"TEST-half-typed"},{"click":"role:button:Close"},{"expect":"role:dialog","count":0},{"expect":{"css":".engine-row","hasText":"Claude Code"},"text":"not logged in"},{"click":{"role":"button","name":"Log in","nth":0}},{"expect":"role:dialog:Log in to Claude Code","text":"Open the sign-in page","timeout":15000},{"expect":"label:Code from the sign-in page","value":""},{"shot":"fresh-login"},{"press":"Escape"},{"expect":"role:dialog","count":0},{"expect":{"css":".engine-row","hasText":"Claude Code"},"text":"not logged in"}]'
yardd_pid=$(ss -Htlnp "sport = :$(v port)" | grep -o 'pid=[0-9]*' | head -1 | cut -d= -f2)
test -n "$yardd_pid"
sleep 1
! pgrep -P "$yardd_pid" -f 'auth login'
```

### codex-device-login
Log in to Codex: the dialog shows the device code; the fake completes and the
host shows Codex logged in.

```sh
yv drive hosts --viewport desktop --steps '[{"click":"role:tab:Engine logins"},{"click":{"role":"button","name":"Log in","nth":1}},{"expect":"role:dialog","text":"Codex is logged in on iso-host","timeout":15000}]'
```

### claude-code-login
A wrong code is refused ("That code was not accepted. Start again…", which the UI also reports as a failed `login` reply, hence `--allow-errors`); after Try again the right one ("good-code" for the fake) logs in.

```sh
yv drive hosts --viewport desktop --allow-errors --steps '[{"click":"role:tab:Engine logins"},{"click":{"role":"button","name":"Log in","nth":0}},{"expect":"role:dialog","text":"Log in to Claude"},{"fill":"label:Code from the sign-in page","value":"bad-code"},{"press":"Enter"},{"expect":"role:dialog","text":"That code was not accepted","timeout":15000},{"click":"role:button:Try again"},{"fill":"label:Code from the sign-in page","value":"good-code","timeout":15000},{"press":"Enter"},{"expect":"role:dialog","text":"Claude Code is logged in on iso-host","timeout":15000}]'
```

### grok-device-login
Log in to Grok: the dialog shows the device link and code (GROK-1234 for the
fake, which confirms 2.5 s later), then the host shows Grok logged in.

```sh
yv drive hosts --viewport desktop --steps '[{"click":"role:tab:Engine logins"},{"click":"css:.engine-block:nth-child(4) .engine-login"},{"expect":"role:dialog","text":"GROK-1234"},{"expect":"role:dialog","text":"Grok is logged in on iso-host","timeout":15000},{"wait":2500},{"expect":{"css":".engine-block:nth-child(4)","hasText":"logged in as default@grok.example"}}]'
```

### gemini-check-login-says-still-logged-out
Viewports: both
Gemini logs in from a terminal on the host. **Check login** in its dialog asks
again; while Gemini is still logged out the dialog says so, instead of looking
exactly as it did before the click.

```sh
yv drive hosts --steps '[{"click":"role:tab:Engine logins"},{"click":"css:.engine-block:has-text(\"Gemini\") .engine-login"},{"expect":"role:dialog","text":"Run this in a terminal"},{"click":"role:button:Check login"},{"expect":"role:dialog","text":"Still not logged in on iso-host","timeout":15000},{"expect":"role:button:Check login"},{"shot":"gemini-still-logged-out"}]'
```

### opencode-api-key
Expect at both viewports: Add provider lists OpenCode's providers with a
filter; picking Fake Provider asks for its API key; saving adds it, and the
host lists it among OpenCode's providers. Viewports: mobile adds the key,
desktop then finds it marked added.

```sh
yv drive hosts --viewport mobile --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Add provider"},{"fill":"label:Filter providers","value":"fake prov"},{"click":{"css":".provider-row","hasText":"Fake Provider"}},{"expect":"css:.login-key","text":"API key for fakeprov"},{"fill":"css:#login-key","value":"sk-TEST-not-real"},{"click":"role:button:Save"},{"expect":"role:dialog","text":"OpenCode can use the provider","timeout":15000},{"wait":2500},{"expect":{"css":".engine-block:nth-child(3)","hasText":"providers: opencode, fakeprov"}}]'
yv drive hosts --viewport desktop --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Add provider"},{"expect":{"css":".provider-row","hasText":"Fake Provider"},"text":"added"}]'
```

### opencode-device-sign-in
A provider with two ways in shows both; its device code (FAKE-4321) shows
until the fake's server finishes, 2.5 s later.

```sh
yv drive hosts --viewport desktop --steps '[{"click":"role:tab:Engine logins"},{"click":"role:button:Add provider"},{"click":{"css":".provider-row","hasText":"Fake Sign-in"}},{"click":"role:button:Device code"},{"expect":"role:dialog","text":"FAKE-4321"},{"expect":"role:dialog","text":"OpenCode can use the provider","timeout":15000}]'
```

## Gotchas
- The fake Codex finishes a device login 300 ms after it starts, so the dialog with the code (FAKE-1234) is gone before a script can read it.
- The order of the Log in buttons on a host card is Claude Code, then Codex, then Grok; OpenCode's button is Add provider.
- The fake OpenCode lists models only for `fakeprov`, so a provider added by sign-in (`fakeauth`) does not show among the host's providers.
