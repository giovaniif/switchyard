# Agents

Named agents the user talks to by name, one conversation each for its whole
life. The Agents page is a grid of cards (side by side on a wide window,
stacked below 700 px, rows lining up): avatar, name and `@name`, what it is
for, how it runs, a status line from its conversation, Ask or Message, and
Edit. Scout is built in: it answers questions about the hosts from read-only
evidence, without tools, and took over Switchy (`#/assistant` lands here;
Switchy's old conversation is not read). An agent's conversation is an
ordinary session tagged with it but never a sidebar row: the Agents nav row
and the card carry its state (working, needs approval, an unseen answer).
Its view has the agent's header and, for Scout or an agent in its own
folder, no Terminal pane. Typing `@` in a message box offers the agents; a
message that starts with `@Name` goes to that agent's conversation, and the
box says so before it is sent. An agent runs on this host or on a remote
one, picked in its form with the host picker; the account picker shows when
the engine has more than one account there. A remote agent's card and
header name its host; while the host is offline they say so and nothing
can be sent. Its tools (needs you, messaging another agent, its memory) act
here on the hub, passed back by its host.

## Setup

```sh
yv iso up
yv iso seed
yv api '{"op":"agent.save","agent":{"id":"scout","model":"fake-fast","effort":"low","scope":"local"}}' --wait '{"op":"agent.saved"}'
yv api '{"op":"agent.save","agent":{"name":"TEST-Reviewer","engine":"codex","model":"fake-fast","effort":"low","color":"violet","about":"TEST-reviews pull requests","instructions":"TEST-be terse","memory":"TEST-main is trunk"}}' --wait '{"op":"agent.saved"}' --save reviewer=agent.id
yv var set remote_port "$(yv iso up --name map-agents-remote --host-name TEST-agents-box | jq -r .iso.port)"
yv api '{"op":"host.add","name":"TEST-agents-box","kind":"direct","url":"ws://127.0.0.1:{{remote_port}}/ws"}' --wait '{"op":"hosts","hosts":[{"name":"TEST-agents-box","status":"connected"}]}' --save box='hosts[name=TEST-agents-box].id'
yv api '{"op":"account.add","engine":"codex","name":"TEST-Work","host":"{{box}}"}' --wait '{"op":"accounts","hostId":"{{box}}"}'
```

## Cleanup

```sh
yv iso down
yv iso down --name map-agents-remote || true
```

## Recipes

### cards-show-scout-and-the-users-agents
Scout is first, built in and read-only; the user's agent shows its mention
name, what it is for and how it runs. Neither has a message yet. At 390 px
the page is in the drawer and the cards stack inside the window.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Agents"},{"expect":"role:heading:Agents"},{"expect":{"css":".agent-card","hasText":"Built in · reads"}},{"expect":{"css":".agent-card","hasText":"@TEST-Reviewer"}},{"expect":{"css":".agent-card","hasText":"Fake Fast · Low · Ask first"}},{"expect":{"css":".agent-card","hasText":"No messages yet"}},{"expect":"role:button:Edit: Scout"},{"expect":"role:button:New chat with Scout","count":0},{"shot":"cards"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Agents"},{"expect":"role:heading:Agents","inViewport":true},{"expect":{"css":".agent-card","hasText":"Scout"},"inViewport":true},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"cards"}]'
```

### cards-in-a-row-line-up
Viewports: desktop
A two-line description does not push one card's status line and buttons below
its neighbours': cards in a row share its height, with the status line and
footer on the bottom edge (owner review of #383).

```sh
yv api '{"op":"agent.save","agent":{"name":"TEST-Short","engine":"codex","model":"fake-fast","effort":"low","about":"TEST-one line"}}' --wait '{"op":"agent.saved"}'
yv api '{"op":"agent.save","agent":{"name":"TEST-Long","engine":"codex","model":"fake-fast","effort":"low","about":"TEST-a description long enough to wrap onto a second line in a card of the agents grid"}}' --wait '{"op":"agent.saved"}'
yv drive agents --viewport desktop --steps '[{"expect":{"css":".agent-card","hasText":"TEST-Long"}},{"shot":"aligned"},{"eval":"(() => { const top = (el) => Math.round(el.getBoundingClientRect().top); const cards = [...document.querySelectorAll(\".agent-card\")]; const row = cards.filter((c) => Math.abs(top(c) - top(cards[0])) < 2); const about = new Set(row.map((c) => Math.round(c.querySelector(\".agent-about\")?.getBoundingClientRect().height ?? 0))); const status = new Set(row.map((c) => top(c.querySelector(\".automation-status\")))); const foot = new Set(row.map((c) => top(c.querySelector(\".automation-foot\")))); return row.length > 2 && about.size > 1 && status.size === 1 && foot.size === 1; })()","equals":true}]'
```

### scout-answers-in-its-one-conversation
Viewports: desktop
Ask opens Scout's conversation, empty at first, with its suggested
questions. A question gets a "Checked" step listing the sources with their
ids, then the answer under Scout's name. The conversation is not a sidebar
row; its header is Scout's (face, name, Edit), there is no Terminal pane,
and the reply box has no permission picker: Scout only reads.

```sh
yv drive agents --viewport desktop --steps '[{"click":"role:button:Ask"},{"expect":"role:heading:What would you like to know?"},{"expect":"role:group:Suggested questions"},{"fill":"label:Message","value":"TEST-what is running here?"},{"press":"Enter"},{"expect":"css:.agent-says","text":"Scout"},{"expect":"css:.thread","text":"scripted host agent answer"},{"click":{"css":".fold-row","hasText":"Read 1 host snapshot"}},{"click":{"css":".tool-row","hasText":"Checked"}},{"expect":"css:.tool-output","text":"[local:1]"},{"expect":"css:.agent-bar","text":"Scout"},{"expect":"css:#session-workspace","count":0},{"expect":{"css":".session-row","hasText":"TEST-what is running here?"},"count":0},{"expect":"css:.composer .agent-runs-chip","text":"Read-only"},{"expect":"css:.composer .permission","count":0},{"shot":"scout-conversation"}]'
yv state --expect '{"sessions":[{"title":"~TEST-what is running here?","agent":"scout","permission":"plan","status":"idle"}]}'
```

### agent-views-on-a-phone
Viewports: mobile
At 390 px Scout's conversation takes a second question in the same thread,
and an agent with no messages shows its empty conversation, all inside the
window.

```sh
yv drive agents --viewport mobile --steps '[{"click":"role:button:Ask"},{"expect":"css:.agent-bar","text":"Scout"},{"expect":"css:.thread","text":"TEST-what is running here?"},{"fill":"label:Message","value":"TEST-and on the phone?"},{"press":"Enter"},{"expect":{"css":".user-message, .bubble, .thread","hasText":"TEST-and on the phone?"}},{"expect":"css:.workspace-toggle","count":0},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"scout-conversation"}]'
yv drive agents --viewport mobile --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Long\"] .agent-message"},{"expect":"role:heading:Message TEST-Long","inViewport":true},{"expect":"label:Message","inViewport":true},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"empty-conversation"}]'
```

### create-an-agent-from-the-form
The form names the rule before saving (one word, unique), takes a face and a colour,
what it is for, instructions and memory, and the folder, model, effort and
permission pickers. Saved, it is a card. Each viewport makes its own.

```sh
yv drive agents --viewport desktop --steps '[{"click":"role:button:New agent"},{"fill":"label:Name","value":"two words"},{"expect":"role:alert","text":"One word"},{"fill":"label:Name","value":"test-reviewer"},{"expect":"role:alert","text":"already called"},{"fill":"label:Name","value":"TEST-Desk"},{"expect":"css:.agent-form-name","text":"Mention it as @TEST-Desk"},{"click":"role:button:Colour: rose"},{"click":"role:button:Face: wink"},{"expect":"css:.agent-face-option[aria-pressed=\"true\"][aria-label=\"Face: wink\"]"},{"fill":"label:What it is for","value":"TEST-desk work"},{"fill":"label:Instructions","value":"TEST-plan first"},{"fill":"label:Memory","value":"TEST-remember this"},{"expect":{"css":".agent-form .chip","hasText":"Its own folder"}},{"shot":"form"},{"click":"role:button:Create agent"},{"expect":{"css":".agent-card","hasText":"@TEST-Desk"}},{"shot":"saved"}]'
yv drive agents --viewport mobile --steps '[{"click":"role:button:New agent"},{"fill":"label:Name","value":"TEST-Phone"},{"click":"role:button:Colour: teal"},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"form"},{"click":"role:button:Create agent"},{"expect":{"css":".agent-card","hasText":"@TEST-Phone"}}]'
yv api '{"op":"agent.list"}' --wait '{"op":"agents"}' --expect '{"op":"agents","agents":[{"name":"TEST-Desk","color":"rose","face":"wink","instructions":"TEST-plan first","memory":"TEST-remember this","permission":"ask"},{"name":"TEST-Phone","color":"teal"}]}'
```

### a-name-problem-shows-by-the-name
A name the form refuses says why right under the name field, in view, not
only at the end of the form below the fold: a taken name, then two words.
Fixed, the line under the field names the mention again (#391).

```sh
yv drive agents --steps '[{"click":"role:button:New agent"},{"fill":"label:Name","value":"Scout"},{"expect":"role:button:Create agent","enabled":false},{"expect":"css:.agent-form-name [role=alert]","text":"Another agent is already called Scout.","inViewport":true},{"shot":"taken"},{"fill":"label:Name","value":"two words"},{"expect":"css:.agent-form-name [role=alert]","text":"One word","inViewport":true},{"fill":"label:Name","value":"TEST-Fine"},{"expect":"css:.agent-form-name","text":"Mention it as @TEST-Fine"},{"expect":"role:alert","count":0}]'
```

### at-mention-from-new-session-goes-to-the-agents-conversation
`@` offers the agents, filtered by what follows; Enter picks one. A message
that starts with the mention says where it goes, and Send opens the agent's
conversation instead of a new session. The engine got the preamble; the
thread shows only what was typed, and nothing new is in the sidebar.

```sh
yv drive new --viewport desktop --steps '[{"fill":"label:Message","value":"@test-r"},{"expect":"role:listbox:Agents"},{"expect":{"css":".mention-list .menu-item","hasText":"TEST-Reviewer"}},{"expect":{"css":".mention-list .menu-item","hasText":"Scout"},"count":0},{"shot":"mention-list"},{"press":"Enter"},{"expect":"label:Message","value":"@TEST-Reviewer "},{"expect":"css:.mention-route","text":"Goes to TEST-Reviewer"},{"fill":"label:Message","value":"@TEST-Reviewer TEST-look at the diff"},{"press":"Enter"},{"expect":"css:.agent-bar","text":"TEST-Reviewer"},{"expect":"css:.agent-says","text":"TEST-Reviewer"},{"expect":"css:.thread","text":"Echo: TEST-look at the diff"},{"expect":{"css":".thread","hasText":"The user'"'"'s message"},"count":0},{"expect":{"css":".session-row","hasText":"TEST-look at the diff"},"count":0},{"shot":"reviewer-conversation"}]'
yv state --expect '{"sessions":[{"title":"~TEST-look at the diff","agent":"{{reviewer}}"}]}'
```

### at-mention-in-a-session-goes-to-the-agents-conversation
In a coding session's reply box a leading `@Name` says the message goes to
that agent, not this session; sent, it opens the agent's conversation with
the message as its next turn. Escape closes only the list.

```sh
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"fill":"label:Message","value":"@TEST"},{"expect":"role:listbox:Agents"},{"press":"Escape"},{"expect":"role:listbox:Agents","count":0},{"expect":"css:.thread"},{"fill":"label:Message","value":"@TEST-Reviewer TEST-second ask"},{"expect":"css:.mention-route","text":"Goes to TEST-Reviewer, not this session"},{"shot":"routed"},{"press":"Enter"},{"expect":"css:.agent-bar","text":"TEST-Reviewer"},{"expect":"css:.thread","text":"Echo: TEST-second ask"},{"expect":"css:.thread","text":"Echo: TEST-look at the diff"}]'
```

### one-conversation-and-the-agents-mark
Every message to an agent lands in its one conversation. An answer the user
has not opened marks the Agents row and the agent's card, which quotes it;
opening the conversations clears both. (The drive forgets what this browser
has seen first, so every answer counts as new, as the sidebar recipes do.)

```sh
yv api '{"op":"agent.ask","agentId":"{{reviewer}}","text":"TEST-while you were away"}' --wait '{"op":"event","event":{"type":"turn_completed"}}' --save away=sessionId
yv api '{"op":"agent.ask","agentId":"{{reviewer}}","text":"TEST-one more","fresh":true}' --wait '{"op":"created"}' --save again=sessionId
test "$(yv var get away | jq -r .value)" = "$(yv var get again | jq -r .value)"
yv drive new --viewport desktop --steps '[{"eval":"(()=>{localStorage.setItem(\"yard.seen.since\",\"1\");localStorage.removeItem(\"yard.seen\");return 1})()"},{"reload":true},{"expect":"css:.nav-row .dot-answered"},{"click":"role:button:Agents"},{"expect":{"css":".agent-card[aria-label=\"TEST-Reviewer\"] .automation-status","hasText":"Answered"}},{"expect":{"css":".agent-card[aria-label=\"TEST-Reviewer\"] .automation-status","hasText":"Echo: TEST-one more"}},{"expect":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .mark-answered"},{"shot":"answered"},{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"expect":"css:.thread","text":"TEST-while you were away"},{"expect":"css:.thread","text":"Echo: TEST-look at the diff"},{"click":"role:button:Agents"},{"expect":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .mark-done"},{"click":"role:button:Ask"},{"expect":"css:.agent-bar","text":"Scout"},{"click":"role:button:Agents"},{"expect":"css:.nav-row .dot-answered","count":0}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":"role:button:Agents"},{"expect":{"css":".session-row","hasText":"TEST-while you were away"},"count":0},{"shot":"drawer"}]'
```

### agents-message-each-other-with-badges
An agent's `message_agent` call puts the message into the other agent's one
conversation, badged "From @Builder" with the sender's face; when that turn
ends its answer comes back into the sender's conversation, badged "Reply from
@Reviewer". The scripted Codex calls the real tools server (`yardd
agent-tools`) for a `[[tool NAME {json}]]` marker.

```sh
yv api '{"op":"agent.save","agent":{"name":"TEST-Builder","engine":"codex","model":"fake-fast","effort":"low","color":"orange","about":"TEST-builds things"}}' --wait '{"op":"agent.saved"}' --save builder=agent.id
yv api '{"op":"agent.ask","agentId":"{{builder}}","text":"[[tool message_agent {\"to\":\"TEST-Reviewer\",\"text\":\"TEST-check the build on main\"}]]"}' --wait '{"op":"event","event":{"type":"user_message","origin":{"kind":"agentReply","name":"TEST-Reviewer"}}}'
yv drive agents --viewport desktop --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Reviewer\"] .agent-message"},{"expect":{"css":".origin-badge","hasText":"From @TEST-Builder"}},{"expect":"css:.origin-badge .agent-avatar"},{"expect":{"css":".user-message .bubble","hasText":"TEST-check the build on main"}},{"eval":"(() => { const b = document.querySelector(\".origin-badge\").getBoundingClientRect(); const m = document.querySelector(\".origin-badge + .bubble\").getBoundingClientRect(); return b.bottom <= m.top && Math.abs(b.right - m.right) <= 6; })()","equals":true},{"shot":"from-badge"},{"click":"role:button:Agents"},{"click":"css:.agent-card[aria-label=\"TEST-Builder\"] .agent-message"},{"expect":{"css":".origin-badge","hasText":"Reply from @TEST-Reviewer"}},{"expect":"css:.thread","text":"Sent to @TEST-Reviewer (hop 1 of 3)"},{"shot":"reply-badge"}]'
yv drive agents --viewport mobile --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Builder\"] .agent-message"},{"expect":{"css":".origin-badge","hasText":"Reply from @TEST-Reviewer"},"inViewport":true},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"reply-badge"}]'
```

### an-agent-that-needs-you
An agent's `notify_owner` call shows an OS notification (recorded here) that
opens its conversation, and marks its card "Needs you · <message>" and the
Agents row with an amber dot until the user opens the conversation.

```sh
yv api '{"op":"agent.save","agent":{"name":"TEST-Pager","engine":"codex","model":"fake-fast","effort":"low","color":"rose","about":"TEST-watches the deploys"}}' --wait '{"op":"agent.saved"}' --save pager=agent.id
yv api '{"op":"agent.ask","agentId":"{{pager}}","text":"[[tool notify_owner {\"message\":\"TEST-the deploy needs your key\"}]]"}' --wait '{"op":"notify","title":"TEST-Pager needs you","body":"TEST-the deploy needs your key"}'
yv drive new --viewport desktop --steps '[{"expect":"css:.nav-row .dot-needsYou"},{"click":"role:button:Agents"},{"expect":{"css":".agent-card[aria-label=\"TEST-Pager\"] .automation-status","hasText":"Needs you · TEST-the deploy needs your key"}},{"expect":"css:.agent-card[aria-label=\"TEST-Pager\"] .mark-approval .mark-bang"},{"shot":"needs-you"},{"click":"css:.agent-card[aria-label=\"TEST-Pager\"] .agent-message"},{"expect":"css:.agent-bar","text":"TEST-Pager"},{"click":"role:button:Agents"},{"expect":{"css":".agent-card[aria-label=\"TEST-Pager\"] .automation-status","hasText":"Needs you"},"count":0},{"expect":"css:.nav-row .dot-needsYou","count":0}]'
yv api '{"op":"agent.ask","agentId":"{{pager}}","text":"[[tool notify_owner {\"message\":\"TEST-still need that key\"}]]"}' --wait '{"op":"notify","title":"TEST-Pager needs you"}'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":"css:.nav-row .dot-needsYou"},{"wait":700},{"shot":"agents-mark"},{"click":"role:button:Agents"},{"expect":{"css":".agent-card[aria-label=\"TEST-Pager\"] .automation-status","hasText":"Needs you · TEST-still need that key"}},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"wait":700},{"shot":"needs-you"},{"click":"css:.agent-card[aria-label=\"TEST-Pager\"] .agent-message"},{"expect":"css:.agent-bar","text":"TEST-Pager"},{"click":"role:button:Open sessions"},{"expect":"css:.nav-row .dot-needsYou","count":0}]'
```

### an-agents-own-memory-shows-in-the-form
An agent's `remember` call adds a line to its memory: the thread shows a
"Remembered" step, and its form shows the new memory.

```sh
yv api '{"op":"agent.ask","agentId":"{{reviewer}}","text":"[[tool remember {\"text\":\"TEST-deploys go out on Fridays\"}]]"}' --wait '{"op":"agents","agents":[{"name":"TEST-Reviewer","memory":"TEST-main is trunk\nTEST-deploys go out on Fridays"}]}'
yv drive agents --viewport desktop --steps '[{"click":"role:button:Edit: TEST-Reviewer"},{"expect":"label:Memory","value":"TEST-main is trunk\nTEST-deploys go out on Fridays"},{"eval":"document.getElementById(\"agent-memory\").scrollIntoView({block:\"center\"}) ?? true","equals":true},{"wait":300},{"shot":"memory"}]'
yv drive agents --viewport mobile --steps '[{"click":"role:button:Edit: TEST-Reviewer"},{"expect":"label:Memory","value":"TEST-main is trunk\nTEST-deploys go out on Fridays"},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"eval":"document.getElementById(\"agent-memory\").scrollIntoView({block:\"center\"}) ?? true","equals":true},{"wait":300},{"shot":"memory"}]'
```

### switchys-address-lands-on-the-agents
The old `#/assistant` (bookmarks, the Mac app's history) opens the Agents
page; Cmd-K finds it by Switchy's name and lists each agent, which opens its
conversation.

Viewports: desktop

```sh
yv drive new --viewport desktop --steps '[{"eval":"location.hash = \"#/assistant\"; true","equals":true},{"expect":"role:heading:Agents"},{"press":"Control+k"},{"fill":"role:combobox:Search sessions and pages","value":"switchy"},{"expect":{"css":".navigation-option","hasText":"Agents"}},{"press":"Enter"},{"expect":"role:heading:Agents"},{"press":"Control+k"},{"fill":"role:combobox:Search sessions and pages","value":"scout"},{"expect":{"css":".navigation-option","hasText":"Scout"}},{"expect":{"css":".navigation-option","hasText":"TEST-what is running here?"},"count":0},{"click":{"css":".navigation-option","hasText":"Answers quick questions"}},{"expect":"css:.agent-bar","text":"Scout"}]'
```

### deleting-an-agent-deletes-its-conversation
Delete asks once more and says the conversation goes too. Scout has no
Delete.

Viewports: desktop

```sh
yv drive agents --viewport desktop --steps '[{"click":"role:button:Edit: Scout"},{"expect":"role:button:Delete agent","count":0},{"click":"role:button:Cancel"},{"click":"role:button:Edit: TEST-Reviewer"},{"click":"role:button:Delete agent"},{"expect":"css:.automation-form","text":"Its conversation is deleted too."},{"click":"role:button:Confirm delete"},{"expect":"css:.agent-card[aria-label=\"TEST-Reviewer\"]","count":0}]'
yv api '{"op":"session.open","sessionId":"{{away}}"}' --wait '{"op":"history"}' --expect '{"op":"history","events":[]}'
```

### form-host-and-account-pickers
The form's Where and how card starts with the host picker (as on New
session). This host has one account per engine, so there is no account picker;
TEST-agents-box has two Codex accounts, so picking it and a Codex model
shows the account picker, and the
explanation names the host. Saved, the card's how-it-runs line starts with
the host after a cloud and names the account.

```sh
yv drive agents --viewport desktop --steps '[{"click":"role:button:New agent"},{"fill":"label:Name","value":"TEST-Remote"},{"eval":"document.querySelector(\".agent-form-foot\").previousElementSibling.scrollIntoView({block:\"end\"}); true","equals":true},{"expect":"css:.agent-form .account-trigger","count":0},{"click":{"css":".agent-form-where .chip","nth":0}},{"click":{"css":".picker-item","hasText":"TEST-agents-box"}},{"expect":"css:.agent-form-card-head:has(#agent-form-where) p","text":"It works in a folder of its own on TEST-agents-box."},{"click":{"css":".agent-form-where button[aria-label^=\"Model\"]"}},{"click":{"css":".picker-fold","hasText":"Codex"}},{"click":{"role":"button","name":"Fake Fast","exact":true}},{"press":"Escape"},{"expect":"css:.agent-form .account-trigger","text":"Codex · Default"},{"click":"css:.agent-form .account-trigger"},{"click":{"css":".account-option","hasText":"TEST-Work"}},{"expect":"css:.agent-form .account-trigger","text":"Codex · TEST-Work"},{"shot":"form"},{"click":"role:button:Create agent"},{"expect":{"css":".agent-card[aria-label=\"TEST-Remote\"] .agent-runs-remote","hasText":"TEST-agents-box · Fake Fast · TEST-Work"}},{"shot":"saved"}]'
yv drive agents --viewport mobile --steps '[{"click":"role:button:New agent"},{"fill":"label:Name","value":"TEST-Phone-far"},{"eval":"document.querySelector(\".agent-form-foot\").previousElementSibling.scrollIntoView({block:\"end\"}); true","equals":true},{"click":{"css":".agent-form-where .chip","nth":0}},{"click":{"css":".picker-item","hasText":"TEST-agents-box"}},{"click":{"css":".agent-form-where button[aria-label^=\"Model\"]"}},{"click":{"css":".picker-fold","hasText":"Codex"}},{"click":{"role":"button","name":"Fake Fast","exact":true}},{"press":"Escape"},{"expect":"css:.agent-form .account-trigger","text":"Codex · Default"},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"form"},{"click":"role:button:Cancel"},{"expect":"role:heading:Agents"}]'
yv api '{"op":"agent.list"}' --wait '{"op":"agents"}' --expect '{"op":"agents","agents":[{"name":"TEST-Remote","host":"{{box}}"}]}'
```

### a-mention-reaches-an-agent-on-a-remote-host
Viewports: desktop
A leading `@Name` of an agent on a remote host goes to its conversation
there: its header names the host, and its answer comes back through the
hub. The conversation is not a sidebar row; the card quotes the answer
under the cloud line.

```sh
yv drive new --viewport desktop --steps '[{"fill":"label:Message","value":"@TEST-Remote TEST-hello box"},{"expect":"css:.mention-route","text":"Goes to TEST-Remote"},{"press":"Enter"},{"expect":"css:.agent-bar","text":"TEST-agents-box"},{"expect":"css:.thread","text":"Echo: TEST-hello box"},{"expect":{"css":".session-row","hasText":"TEST-hello box"},"count":0},{"shot":"remote-conversation"},{"click":"role:button:Agents"},{"expect":{"css":".agent-card[aria-label=\"TEST-Remote\"] .automation-status","hasText":"Echo: TEST-hello box"}},{"shot":"remote-card"}]'
```

### a-remote-agents-conversation-on-a-phone
Viewports: mobile
At 390 px the remote agent's conversation keeps its host in the header and
takes a reply in the same thread.

```sh
yv drive agents --viewport mobile --steps '[{"click":"css:.agent-card[aria-label=\"TEST-Remote\"] .agent-message"},{"expect":"css:.agent-bar","text":"TEST-agents-box"},{"fill":"label:Message","value":"TEST-from the phone"},{"press":"Enter"},{"expect":"css:.thread","text":"Echo: TEST-from the phone"},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"remote-conversation"}]'
```

### a-remote-agents-tools-act-on-the-hub
An agent on a remote host has the same tools: they run on its host's engine
and act here, where its record and the other agents are. Its `remember`
lands in its form, its `message_agent` reaches an agent on this host and the
answer comes back to its conversation there, and its `notify_owner` marks
its card until the conversation is opened.

```sh
yv api '{"op":"agent.list"}' --wait '{"op":"agents"}' --save far='agents[name=TEST-Remote].id'
yv api '{"op":"agent.ask","agentId":"{{far}}","text":"[[tool remember {\"text\":\"TEST-the box builds nightly\"}]] [[tool message_agent {\"to\":\"TEST-Builder\",\"text\":\"TEST-is the build green?\"}]] [[tool notify_owner {\"message\":\"TEST-the box needs a reboot\"}]]"}' --wait '{"op":"notify","title":"TEST-Remote needs you","body":"TEST-the box needs a reboot"}'
yv api '{"op":"agent.list"}' --wait '{"op":"agents"}' --expect '{"op":"agents","agents":[{"name":"TEST-Remote","memory":"TEST-the box builds nightly"}]}'
yv drive new --viewport desktop --steps '[{"expect":"css:.nav-row .dot-needsYou"},{"click":"role:button:Agents"},{"expect":{"css":".agent-card[aria-label=\"TEST-Remote\"] .automation-status","hasText":"Needs you · TEST-the box needs a reboot"}},{"expect":"css:.agent-card[aria-label=\"TEST-Remote\"] .mark-approval .mark-bang"},{"shot":"remote-needs-you"},{"click":"css:.agent-card[aria-label=\"TEST-Remote\"] .agent-message"},{"expect":"css:.agent-bar","text":"TEST-agents-box"},{"expect":{"css":".origin-badge","hasText":"Reply from @TEST-Builder"}},{"expect":"css:.thread","text":"Sent to @TEST-Builder (hop 1 of 3)"},{"shot":"remote-reply"},{"click":"role:button:Agents"},{"expect":{"css":".agent-card[aria-label=\"TEST-Remote\"] .automation-status","hasText":"Needs you"},"count":0},{"click":"css:.agent-card[aria-label=\"TEST-Builder\"] .agent-message"},{"expect":{"css":".origin-badge","hasText":"From @TEST-Remote"}},{"expect":{"css":".user-message .bubble","hasText":"TEST-is the build green?"}},{"click":"role:button:Agents"},{"click":"role:button:Edit: TEST-Remote"},{"expect":"label:Memory","value":"TEST-the box builds nightly"},{"click":"role:button:Cancel"}]'
yv api '{"op":"agent.ask","agentId":"{{far}}","text":"[[tool notify_owner {\"message\":\"TEST-still needs that reboot\"}]]"}' --wait '{"op":"notify","title":"TEST-Remote needs you"}'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"expect":"css:.nav-row .dot-needsYou"},{"click":"role:button:Agents"},{"expect":{"css":".agent-card[aria-label=\"TEST-Remote\"] .automation-status","hasText":"Needs you · TEST-still needs that reboot"}},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"eval":"document.querySelector(\".agent-card[aria-label=\\\"TEST-Remote\\\"]\").scrollIntoView({block:\"center\"}) ?? true","equals":true},{"wait":700},{"shot":"remote-needs-you"},{"click":"css:.agent-card[aria-label=\"TEST-Remote\"] .agent-message"},{"expect":{"css":".origin-badge","hasText":"Reply from @TEST-Builder"}},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"remote-reply"},{"click":"role:button:Open sessions"},{"expect":"css:.nav-row .dot-needsYou","count":0}]'
```

### moving-an-agent-says-what-happens-to-its-conversation
Picking another host for an agent with a conversation says, before saving,
that the conversation stays readable where it is and the next message
starts a new one on the new host with its notes and the latest messages.

```sh
yv drive agents --viewport desktop --steps '[{"click":"role:button:Edit: TEST-Remote"},{"eval":"document.querySelector(\".agent-form-foot\").previousElementSibling.scrollIntoView({block:\"end\"}); true","equals":true},{"click":{"css":".agent-form-where .chip","hasText":"TEST-agents-box"}},{"click":{"css":".picker-item","nth":0}},{"expect":"css:.agent-form-card-head:has(#agent-form-where) p","text":"Its conversation stays on TEST-agents-box, readable. The next message starts a new one on"},{"shot":"move-note"},{"click":"role:button:Cancel"}]'
yv drive agents --viewport mobile --steps '[{"click":"role:button:Edit: TEST-Remote"},{"eval":"document.querySelector(\".agent-form-foot\").previousElementSibling.scrollIntoView({block:\"end\"}); true","equals":true},{"click":{"css":".agent-form-where .chip","hasText":"TEST-agents-box"}},{"click":{"css":".picker-item","nth":0}},{"expect":"css:.agent-form-card-head:has(#agent-form-where) p","text":"Its conversation stays on TEST-agents-box, readable."},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"move-note"},{"click":"role:button:Cancel"}]'
```

### a-remote-agents-host-offline
While its host is down, the card says so instead of a stale state, the
conversation's header and dock say messages can't be sent (Send is
disabled), and Edit cannot save. Runs last: it stops the remote.

```sh
yv iso down --name map-agents-remote
yv drive agents --viewport desktop --steps '[{"expect":{"css":".agent-card[aria-label=\"TEST-Remote\"] .automation-status","hasText":"TEST-agents-box is offline"}},{"shot":"offline-card"},{"click":"css:.agent-card[aria-label=\"TEST-Remote\"] .agent-message"},{"expect":"css:.agent-bar","text":"offline"},{"expect":"css:.offline-note","text":"TEST-Remote runs on TEST-agents-box, which is offline"},{"expect":"placeholder:Offline"},{"expect":"role:button:Send","enabled":false},{"shot":"offline-conversation"},{"goto":"agents"},{"click":"role:button:Edit: TEST-Remote"},{"expect":"role:button:Save agent","enabled":false},{"expect":"css:.agent-form-card-head:has(#agent-form-where) p","text":"TEST-agents-box is offline"}]'
yv drive agents --viewport mobile --steps '[{"expect":{"css":".agent-card[aria-label=\"TEST-Remote\"] .automation-status","hasText":"TEST-agents-box is offline"}},{"click":"css:.agent-card[aria-label=\"TEST-Remote\"] .agent-message"},{"expect":"css:.offline-note","text":"which is offline"},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"offline-conversation"}]'
yv api '{"op":"agent.list"}' --wait '{"op":"agents"}' --save remote_agent='agents[name=TEST-Remote].id'
yv api '{"op":"agent.ask","agentId":"{{remote_agent}}","text":"TEST-anyone there?"}' --wait '{"op":"error"}' --expect '{"op":"error","message":"~which is offline. Your message was not sent."}'
```

## Gotchas

- Real engines are not exercised here: the scripted Codex echoes what the user
  typed (it strips the preamble) and gives a canned host answer. Whether a
  real model follows an agent's instructions, or cites evidence well, is not
  proven by this map.
- A fresh engine context (a full context, a lost thread, a new model) is
  covered by `tests/agents.rs`, not here: its only sign in the UI is a note
  line in the thread.
- An agent's move between hosts (its old conversation left readable, the
  next one seeded with the latest exchanges), the preamble, changed notes and
  a fresh context on a remote are covered by `tests/agents.rs` against two
  daemons; here only the form's note about a move is driven.
- A remote agent's tools when its host cannot reach the hub (the call says
  so), and a host copy from a hub that does not answer tools (no tools
  offered), are covered by `tests/agent_tools.rs`, not here.
