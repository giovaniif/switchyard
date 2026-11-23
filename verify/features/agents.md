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
box says so before it is sent.

## Setup

```sh
yv iso up
yv iso seed
yv api '{"op":"agent.save","agent":{"id":"scout","model":"fake-fast","effort":"low","scope":"local"}}' --wait '{"op":"agent.saved"}'
yv api '{"op":"agent.save","agent":{"name":"TEST-Reviewer","engine":"codex","model":"fake-fast","effort":"low","color":"violet","about":"TEST-reviews pull requests","instructions":"TEST-be terse","memory":"TEST-main is trunk"}}' --wait '{"op":"agent.saved"}' --save reviewer=agent.id
```

## Cleanup

```sh
yv iso down
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
yv drive agents --viewport desktop --steps '[{"click":"role:button:Edit: Scout"},{"expect":"role:button:Delete agent","count":0},{"click":"role:button:Cancel"},{"click":"role:button:Edit: TEST-Reviewer"},{"click":"role:button:Delete agent"},{"expect":"css:.automation-form","text":"Its conversation is deleted too."},{"click":"role:button:Confirm delete"},{"expect":{"css":".agent-card","hasText":"TEST-Reviewer"},"count":0}]'
yv api '{"op":"session.open","sessionId":"{{away}}"}' --wait '{"op":"history"}' --expect '{"op":"history","events":[]}'
```

## Gotchas

- Real engines are not exercised here: the scripted Codex echoes what the user
  typed (it strips the preamble) and gives a canned host answer. Whether a
  real model follows an agent's instructions, or cites evidence well, is not
  proven by this map.
- A fresh engine context (a full context, a lost thread, a new model) is
  covered by `tests/agents.rs`, not here: its only sign in the UI is a note
  line in the thread.
