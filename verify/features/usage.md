# Plan usage and the model picker's sections

Plan usage is available from one compact sidebar Usage row, opening a popover with one group per account. Subscriptions is the full page for usage, reset times and source hosts. Limits are shared across hosts and keep the existing report freshness rules. Models and accounts use separate composer controls.

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

### usage-strip-in-sidebar
Viewports: both
One Usage row opens the Codex usage meters at either viewport.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Subscription usage"},{"expect":"role:region:Codex · Default","text":"12%"},{"expect":"role:region:Codex · Default","text":"34%"},{"expect":"role:dialog:Subscription usage","inViewport":true},{"shot":"usage"}]'
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Subscription usage"},{"expect":"role:region:Codex · Default","text":"12%"},{"expect":"role:region:Codex · Default","text":"34%"},{"expect":"role:dialog:Subscription usage","inViewport":true},{"shot":"usage"}]'
```

### usage-strip-claude
Viewports: both
The popover also shows all reported Claude windows.

```sh
yv drive 'new' --viewport desktop --steps '[{"click":"role:button:Subscription usage"},{"expect":"role:region:Claude · Default","text":"47%"},{"expect":"role:region:Claude · Default","text":"17%"}]'
yv drive 'new' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Subscription usage"},{"expect":"role:region:Claude · Default","text":"47%"},{"expect":"role:region:Claude · Default","text":"17%"}]'
```

### picker-has-no-meters
Viewports: both
Models have no plan meters; usage reports reset times in the separate popover:
one per Codex window, the fake's second limit (TEST-Spark's week) included.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Model Fake Fast"},{"expect":{"css":".picker-item","hasText":"Fake Fast"}},{"expect":"css:.model-popover [role=progressbar]","count":0},{"press":"Escape"},{"click":"role:button:Subscription usage"},{"expect":"css:.usage-reset","count":3},{"shot":"usage-reset-times"}]'
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"click":"role:button:Session setup"},{"click":"role:button:Model Fake Fast"},{"expect":{"css":".picker-item","hasText":"Fake Fast"}},{"expect":"css:.model-popover [role=progressbar]","count":0},{"press":"Escape"},{"press":"Escape"},{"click":"role:button:Open sessions"},{"click":"role:button:Subscription usage"},{"expect":"css:.usage-reset","count":3},{"shot":"usage-reset-times"}]'
```

### picker-other-engine-collapsed
In a Codex session the Claude models are folded into one row; clicking it opens them and Escape closes the picker.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Model Fake Fast"},{"expect":{"css":".picker-fold","hasText":"Claude"}},{"expect":{"css":".picker-fold","hasText":"switching hands off"}},{"expect":{"role":"button","name":"Haiku 5.5","exact":true},"visible":false},{"click":{"css":".picker-fold"}},{"expect":{"role":"button","name":"Haiku 5.5","exact":true}},{"press":"Escape"},{"expect":{"css":".popover"},"visible":false}]'
```

### picker-current-model-marked
The current model is marked; effort is its own chip, whose menu marks the current one.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Model Fake Fast"},{"expect":{"css":".picker-item.active","hasText":"Fake Fast"}},{"press":"Escape"},{"expect":"role:button:Effort Low"},{"click":"role:button:Effort Low"},{"expect":{"css":".effort-menu [aria-checked=\"true\"]","hasText":"Low"}},{"press":"Escape"}]'
```

### meters-zero-after-reset
Viewports: both
Past a reset, the 30-second clock redraws its limit as 0%, keeps unexpired weekly usage and drops the reset tooltip.
An Other accounts row shows only its first meter (#292), so Claude's unexpired
weekly 17% is read on its own card, in a Claude session.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"click":"role:button:Subscription usage"},{"expect":"role:region:Codex · Default","text":"12%"},{"eval":"(() => { const real=Date.now; Date.now=() => real()+3*3600*1000; return true; })()","equals":true},{"wait":31000},{"expect":"role:region:Codex · Default","text":"0%"},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude · Default","text":"0% of 5-hour"},{"expect":"role:region:Codex · Default","text":"34%"},{"expect":"css:.compact-meter[title*=now]","count":0},{"shot":"usage-after-reset"}]'
yv drive 'session:{{sid_echo}}' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Subscription usage"},{"expect":"role:region:Codex · Default","text":"12%"},{"eval":"(() => { const real=Date.now; Date.now=() => real()+3*3600*1000; return true; })()","equals":true},{"wait":31000},{"expect":"role:region:Codex · Default","text":"0%"},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude · Default","text":"0% of 5-hour"},{"expect":"role:region:Codex · Default","text":"34%"},{"expect":"css:.compact-meter[title*=now]","count":0},{"shot":"usage-after-reset"}]'
yv drive 'session:{{sid_claude}}' --viewport desktop --steps '[{"click":"role:button:Subscription usage"},{"expect":"role:region:Claude · Default","text":"47%"},{"eval":"(() => { const real=Date.now; Date.now=() => real()+3*3600*1000; return true; })()","equals":true},{"wait":31000},{"expect":"role:region:Claude · Default","text":"0% used"},{"expect":"role:region:Claude · Default","text":"17%"},{"expect":"css:.compact-meter[title*=now]","count":0},{"shot":"claude-after-reset"}]'
yv drive 'session:{{sid_claude}}' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Subscription usage"},{"expect":"role:region:Claude · Default","text":"47%"},{"eval":"(() => { const real=Date.now; Date.now=() => real()+3*3600*1000; return true; })()","equals":true},{"wait":31000},{"expect":"role:region:Claude · Default","text":"0% used"},{"expect":"role:region:Claude · Default","text":"17%"},{"expect":"css:.compact-meter[title*=now]","count":0},{"shot":"claude-after-reset"}]'
```

### claude-usage-after-restart
Viewports: both
A restart preserves Claude usage without spending a new turn.

```sh
yv iso restart
yv drive 'new' --viewport desktop --steps '[{"click":"role:button:Subscription usage"},{"expect":"role:region:Claude · Default","text":"47%"},{"shot":"usage-after-restart"}]'
yv drive 'new' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Subscription usage"},{"expect":"role:region:Claude · Default","text":"47%"},{"shot":"usage-after-restart"}]'
```

### strip-names-every-account
Viewports: both
Every account has a labelled group, including Default and an added account reported without a turn.
The added account is logged in first (the fake Claude's `good-code`): since the
approved Usage design (#292) an account that needs sign-in shows **Sign in** in
its row, not meters.

```sh
yv api '{"op":"account.add","engine":"claude","name":"TEST-Work"}' --wait '{"op":"accounts","accounts":[{"name":"TEST-Work"}]}' --save work_acct='accounts[name=TEST-Work].id'
yv api '{"op":"login.start","engine":"claude","account":"{{work_acct}}"}' --wait '{"op":"login"}'
yv api '{"op":"login.code","engine":"claude","account":"{{work_acct}}","code":"good-code"}' --wait '{"op":"engines","accounts":{"{{work_acct}}":{"loggedIn":true}}}' --timeout 20000
yv drive 'new' --viewport desktop --steps '[{"click":"role:button:Subscription usage"},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude · Default","count":1},{"expect":"role:region:Claude · TEST-Work","count":1},{"expect":"role:region:Claude · TEST-Work","text":"91%"},{"expect":"role:region:Claude · Default","text":"47%"},{"shot":"usage-accounts"}]'
yv drive 'new' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Subscription usage"},{"click":"role:button:Other accounts"},{"expect":"role:region:Claude · Default","count":1},{"expect":"role:region:Claude · TEST-Work","count":1},{"expect":"role:region:Claude · TEST-Work","text":"91%"},{"expect":"role:region:Claude · Default","text":"47%"},{"shot":"usage-accounts"}]'
```

### picker-names-the-account
Viewports: desktop
The account chip shows which login is picked separately from the model; choosing another updates that chip.

```sh
yv drive 'new' --viewport desktop --steps '[{"click":"css:.account-trigger"},{"expect":{"role":"menuitemradio","name":"Claude · Default"}},{"click":{"role":"menuitemradio","name":"Claude · TEST-Work"}},{"expect":"role:button:Account: Claude, TEST-Work"},{"click":"role:button:Model Opus 5.5"},{"expect":"css:.picker-accounts","count":0},{"press":"Escape"},{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Claude · Default"}},{"expect":"role:button:Account: Claude, Default"},{"shot":"account-identity"}]'
yv api '{"op":"account.remove","id":"{{work_acct}}"}' --wait '{"op":"accounts"}'
```

### subscriptions-note-names-every-silent-engine
Viewports: both
The note under the meters names every engine that reports no plan limits:
OpenCode and Gemini. It named only OpenCode and Grok after Gemini was
added (#226), as if Gemini's limits were shown above. Grok left the note when
yardd began reading its credit allowance (`_x.ai/billing`).

```sh
yv drive 'subscriptions' --steps '[{"expect":"role:heading:Subscriptions"},{"expect":"css:.subscription-note","text":"OpenCode and Gemini don\u0027t report subscription limits."},{"shot":"subscriptions-note"}]'
```

### every-window-reset-at-a-glance
Viewports: both
Every limit window of every engine and account says when it resets. The
sidebar's Usage row names the reset of the window whose percent it shows
("5-hour resets in …"). Under Other accounts each account lists every window
it reports, each with "Resets <time> · in …", not only its first percent:
Codex's 5-hour and week, its second limit (the fake's TEST-Spark week) and its
free "Full reset" credits with the first expiry; Grok's weekly credits. The
engines that report nothing are named. Subscriptions gives every reset as a
time and a countdown. Before: the row read only "47% used", other accounts
showed "12% of 5-hour" with no reset, Subscriptions only "resets in …", Codex's
full resets and other limits were dropped by yardd, and Grok said nothing.

```sh
yv drive 'new' --viewport desktop --steps '[{"expect":"role:button:Subscription usage","text":"47% used"},{"expect":"css:.usage-trigger-reset","text":"5-hour resets in"},{"click":"role:button:Subscription usage"},{"click":"role:button:Other accounts"},{"expect":"role:region:Codex · Default","text":"12% of 5-hour"},{"expect":"role:region:Codex · Default","text":"34% of Week"},{"expect":"role:region:Codex · Default","text":"5% of Week · TEST-Spark"},{"expect":"role:region:Codex · Default","text":"2 full resets available"},{"expect":"role:region:Codex · Default","text":"First expires"},{"expect":{"css":".usage-other-account","hasText":"Codex · Default"},"text":"Resets "},{"expect":"role:region:Grok · Default","text":"23% of Week"},{"expect":"role:region:Grok · Default","text":"Resets "},{"expect":"css:.usage-silent","text":"OpenCode and Gemini don\u0027t report limits."},{"expect":"css:.usage-other-meter","count":4},{"shot":"usage-resets"},{"press":"Escape"},{"goto":"subscriptions"},{"expect":"role:region:Claude · Default","text":"· in 1 h"},{"expect":"role:region:Codex · Default","text":"2 full resets available"},{"expect":"role:region:Grok · Default","text":"23% used"},{"expect":{"css":".subscription-row","hasText":"Grok · Default"},"text":"Resets "},{"shot":"subscriptions-resets"}]'
yv drive 'new' --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":"role:button:Subscription usage","text":"47% used"},{"expect":"css:.usage-trigger-reset","text":"5-hour resets in"},{"eval":"document.querySelector(\".usage-dock\").getBoundingClientRect().height <= 48","equals":true},{"click":"role:button:Subscription usage"},{"click":"role:button:Other accounts"},{"expect":"role:region:Codex · Default","text":"5% of Week · TEST-Spark"},{"expect":"role:region:Codex · Default","text":"2 full resets available"},{"expect":"role:region:Grok · Default","text":"23% of Week"},{"expect":"css:.usage-silent","text":"OpenCode and Gemini don\u0027t report limits."},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"usage-resets"}]'
yv drive 'subscriptions' --viewport mobile --steps '[{"expect":"role:region:Claude · Default","text":"· in 1 h"},{"expect":"role:region:Codex · Default","text":"2 full resets available"},{"expect":"role:region:Grok · Default","text":"23% used"},{"expect":{"css":".subscription-row","hasText":"Grok · Default"},"text":"Resets "},{"eval":"document.documentElement.scrollWidth <= document.documentElement.clientWidth","equals":true},{"shot":"subscriptions-resets"}]'
```

### sign-in-from-usage-while-on-hosts
Viewports: both
Sign in on a signed-out account in the usage popover opens Engine logins,
also when Hosts is already open on another tab. Before the fix, the URL
changed to `tab=engines` but the page stayed on Sessions, so Sign in looked
dead.

```sh
yv api '{"op":"account.add","engine":"claude","name":"TEST-Away"}' --wait '{"op":"accounts","accounts":[{"name":"TEST-Away"}]}' --save away_acct='accounts[name=TEST-Away].id'
yv drive hosts --viewport desktop --steps '[{"expect":"css:[role=tab][aria-selected=true]","text":"Sessions"},{"click":"role:button:Subscription usage"},{"click":"role:button:Other accounts"},{"click":"role:button:Sign in — Claude · TEST-Away"},{"expect":"css:[role=tab][aria-selected=true]","text":"Engine logins"},{"expect":{"css":".account-row","hasText":"TEST-Away"},"text":"not logged in"},{"shot":"signed-out-account"}]'
yv drive hosts --viewport mobile --steps '[{"expect":"css:[role=tab][aria-selected=true]","text":"Sessions"},{"click":"role:button:Open sessions"},{"click":"role:button:Subscription usage"},{"click":"role:button:Other accounts"},{"click":"role:button:Sign in — Claude · TEST-Away"},{"expect":"css:[role=tab][aria-selected=true]","text":"Engine logins"},{"expect":"css:.host-inspector-heading","inViewport":true},{"expect":{"css":".account-row","hasText":"TEST-Away"},"text":"not logged in"},{"shot":"signed-out-account"}]'
yv api '{"op":"account.remove","id":"{{away_acct}}"}' --wait '{"op":"accounts"}'
```

## Gotchas
- Grok's allowance comes from its agent's `_x.ai/billing` (no turn), read at
  start, every 10 minutes, after a sign-in and after each Grok turn. The
  fake answers in the field names of grok 1.0.77's BillingConfig; the real
  answer's nesting was not seen (no signed-in Grok account on the dev box), so
  yardd parses it loosely and shows nothing without `creditUsagePercent`.
- The 5-hour and week limits come from the engines (Claude's `rate_limit_event` during a turn and its usage report, read without a turn at start, every 10 minutes and when an account is added or logs in; Codex's `account/rateLimits`). A yardd started with `YARD_CLAUDE_USAGE_EVERY_MS=0` (the daemon tests) shows no Claude meters until a session has run (later restarts keep the last ones, from `usage.json`).
