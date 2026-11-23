# Agents

Named agents the user talks to by name. The Agents page is a grid of cards
(side by side on a wide window, stacked below 700 px): avatar, name and
`@name`, what it is for, how it runs, a status line from its ongoing chat, and
Ask or Message, New chat, the list of its chats and Edit. Scout is built in:
it answers questions about the hosts from read-only evidence, without tools,
and took over Switchy (`#/assistant` lands here; Switchy's conversation is
moved into a Scout chat, covered by `tests/agents.rs`). A message to an agent
is an ordinary session tagged with it: its avatar and name are before the
title, over each answer and in its sidebar row. Typing `@` in a message box
offers the agents; a message that starts with `@Name` goes to that agent's
chat, and the box says so before it is sent.

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
name, what it is for and how it runs. Neither has a chat yet. At 390 px the
page is in the drawer and the cards stack inside the window.

```sh
yv drive new --viewport desktop --steps '[{"click":"role:button:Agents"},{"expect":"role:heading:Agents"},{"expect":{"css":".agent-card","hasText":"Built in · reads"}},{"expect":{"css":".agent-card","hasText":"@TEST-Reviewer"}},{"expect":{"css":".agent-card","hasText":"Fake Fast · Low · Ask first"}},{"expect":{"css":".agent-card","hasText":"No chat yet"}},{"expect":"role:button:Edit: Scout"},{"shot":"cards"}]'
yv drive new --viewport mobile --steps '[{"click":"role:button:Open sessions"},{"click":"role:button:Agents"},{"expect":"role:heading:Agents","inViewport":true},{"expect":{"css":".agent-card","hasText":"Scout"},"inViewport":true},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"cards"}]'
```

### scout-answers-from-host-evidence-in-a-chat
Ask opens Scout's start page; a question becomes Scout's chat. The thread
shows the question, a "Checked" step whose output lists the sources with their
ids, then the answer under Scout's name. The chat is a sidebar row with
Scout's name instead of a model, and its reply box has no permission picker:
Scout only reads.

```sh
yv drive agents --viewport desktop --steps '[{"click":"role:button:Ask"},{"expect":"role:heading:What would you like to know?"},{"expect":"role:group:Suggested questions"},{"fill":"label:Message","value":"TEST-what is running here?"},{"press":"Enter"},{"expect":"css:.agent-says","text":"Scout"},{"expect":"css:.thread","text":"scripted host agent answer"},{"click":{"css":".fold-row","hasText":"Read 1 host snapshot"}},{"click":{"css":".tool-row","hasText":"Checked"}},{"expect":"css:.tool-output","text":"[local:1]"},{"expect":{"css":".session-row","hasText":"TEST-what is running here?"}},{"expect":{"css":".session-row .row-agent","hasText":"Scout"}},{"expect":"css:.composer .agent-runs-chip","text":"Read-only"},{"expect":"css:.composer .permission","count":0},{"shot":"scout-chat"}]'
yv state --expect '{"sessions":[{"title":"TEST-what is running here?","agent":"scout","permission":"plan","status":"idle"}]}'
```

### scout-on-a-phone
The start page fits a 390 px window: avatar, question, box and suggestions;
a suggested question starts the chat.

Viewports: mobile

```sh
yv drive agents --viewport mobile --steps '[{"click":"role:button:New chat with Scout"},{"expect":"role:heading:What would you like to know?","inViewport":true},{"expect":"label:Message","inViewport":true},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"scout-start"},{"click":{"css":".agent-suggestions button","hasText":"What changed since yesterday?"}},{"expect":"css:.agent-says","text":"Scout"},{"expect":"css:.thread","text":"scripted host agent answer"},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"scout-chat"}]'
```

### create-an-agent-from-the-form
The form names the rule before saving (one word, unique), takes a colour,
what it is for, instructions and memory, and the folder, model, effort and
permission pickers. Saved, it is a card. Each viewport makes its own.

```sh
yv drive agents --viewport desktop --steps '[{"click":"role:button:New agent"},{"fill":"label:Name","value":"two words"},{"expect":"role:alert","text":"One word"},{"fill":"label:Name","value":"test-reviewer"},{"expect":"role:alert","text":"already called"},{"fill":"label:Name","value":"TEST-Desk"},{"expect":"css:.agent-form-name","text":"Mention it as @TEST-Desk"},{"click":"role:button:Colour: rose"},{"fill":"label:What it is for","value":"TEST-desk work"},{"fill":"label:Instructions","value":"TEST-plan first"},{"fill":"label:Memory","value":"TEST-remember this"},{"shot":"form"},{"click":"role:button:Create agent"},{"expect":{"css":".agent-card","hasText":"@TEST-Desk"}},{"shot":"saved"}]'
yv drive agents --viewport mobile --steps '[{"click":"role:button:New agent"},{"fill":"label:Name","value":"TEST-Phone"},{"click":"role:button:Colour: teal"},{"eval":"document.documentElement.scrollWidth <= innerWidth","equals":true},{"shot":"form"},{"click":"role:button:Create agent"},{"expect":{"css":".agent-card","hasText":"@TEST-Phone"}}]'
yv api '{"op":"agent.list"}' --wait '{"op":"agents"}' --expect '{"op":"agents","agents":[{"name":"TEST-Desk","color":"rose","instructions":"TEST-plan first","memory":"TEST-remember this","permission":"ask"},{"name":"TEST-Phone","color":"teal"}]}'
```

### at-mention-from-new-session-starts-the-agents-chat
`@` offers the agents, filtered by what follows; Enter picks one. A message
that starts with the mention says where it goes, and Send opens the agent's
chat instead of a new session. The engine got the preamble; the thread shows
only what was typed.

```sh
yv drive new --viewport desktop --steps '[{"fill":"label:Message","value":"@test-r"},{"expect":"role:listbox:Agents"},{"expect":{"css":".mention-list .menu-item","hasText":"TEST-Reviewer"}},{"expect":{"css":".mention-list .menu-item","hasText":"Scout"},"count":0},{"shot":"mention-list"},{"press":"Enter"},{"expect":"label:Message","value":"@TEST-Reviewer "},{"expect":"css:.mention-route","text":"Starts a chat with TEST-Reviewer"},{"fill":"label:Message","value":"@TEST-Reviewer TEST-look at the diff"},{"press":"Enter"},{"expect":"css:.agent-says","text":"TEST-Reviewer"},{"expect":"css:.thread","text":"Echo: TEST-look at the diff"},{"expect":{"css":".thread","hasText":"The user'"'"'s message"},"count":0},{"shot":"reviewer-chat"}]'
yv state --expect '{"sessions":[{"title":"TEST-look at the diff","agent":"{{reviewer}}"}]}'
```

### at-mention-in-a-session-goes-to-the-agents-ongoing-chat
In another session's reply box a leading `@Name` says the message goes to the
agent's chat, not this session; sent, it opens that chat with the message as
its next turn. Escape closes only the list.

```sh
yv api '{"op":"agent.ask","agentId":"{{reviewer}}","text":"TEST-first ask"}' --wait '{"op":"created"}' --save chat=sessionId
yv drive 'session:{{sid_echo}}' --viewport desktop --steps '[{"fill":"label:Message","value":"@TEST"},{"expect":"role:listbox:Agents"},{"press":"Escape"},{"expect":"role:listbox:Agents","count":0},{"expect":"css:.thread"},{"fill":"label:Message","value":"@TEST-Reviewer TEST-second ask"},{"expect":"css:.mention-route","text":"Goes to TEST-Reviewer'"'"'s chat, not this session"},{"shot":"routed"},{"press":"Enter"},{"expect":"css:.thread","text":"Echo: TEST-second ask"},{"expect":"css:.bar .agent-chip","text":"TEST-Reviewer"}]'
yv state --expect '{"sessions":[{"id":"{{chat}}","agent":"{{reviewer}}","status":"idle"}]}'
```

### new-chat-and-the-chats-list
Message opens the ongoing chat; New chat starts another, which becomes the
ongoing one; the card's list has both, newest first, and each opens.

Viewports: desktop

```sh
yv api '{"op":"agent.ask","agentId":"{{reviewer}}","text":"TEST-older chat","fresh":true}' --wait '{"op":"created"}'
yv drive agents --viewport desktop --steps '[{"click":"role:button:Message"},{"expect":"css:.thread","text":"TEST-older chat","fresh":true},{"click":"css:.bar .agent-chip"},{"expect":"role:heading:New chat with TEST-Reviewer"},{"expect":"role:button:Current chat"},{"fill":"label:Message","value":"TEST-a fresh start"},{"press":"Enter"},{"expect":"css:.thread","text":"Echo: TEST-a fresh start"},{"click":"role:button:Agents"},{"expect":{"css":".agent-card","hasText":"TEST-a fresh start"}},{"click":"role:button:Chats with TEST-Reviewer"},{"expect":{"css":".automation-run","hasText":"TEST-a fresh start"}},{"expect":{"css":".automation-run","hasText":"TEST-older chat"}},{"shot":"chats"},{"click":{"css":".automation-run","hasText":"TEST-older chat"}},{"expect":"css:.thread","text":"Echo: TEST-older chat"}]'
```

### switchys-address-lands-on-the-agents
The old `#/assistant` (bookmarks, the Mac app's history) opens the Agents
page, and Cmd-K finds it by Switchy's name.

Viewports: desktop

```sh
yv drive new --viewport desktop --steps '[{"eval":"location.hash = \"#/assistant\"; true","equals":true},{"expect":"role:heading:Agents"},{"press":"Control+k"},{"fill":"role:combobox:Search sessions and pages","value":"switchy"},{"expect":{"css":".navigation-option","hasText":"Agents"}},{"press":"Enter"},{"expect":"role:heading:Agents"}]'
```

### deleting-an-agent-keeps-its-chats
Delete asks once more and says the chats stay; they stay as plain sessions.
Scout has no Delete.

Viewports: desktop

```sh
yv api '{"op":"agent.ask","agentId":"{{reviewer}}","text":"TEST-kept chat","fresh":true}' --wait '{"op":"created"}' --save kept=sessionId
yv drive agents --viewport desktop --steps '[{"click":"role:button:Edit: Scout"},{"expect":"role:button:Delete agent","count":0},{"click":"role:button:Cancel"},{"click":"role:button:Edit: TEST-Reviewer"},{"click":"role:button:Delete agent"},{"expect":"css:.automation-form","text":"Its chats stay as sessions."},{"click":"role:button:Confirm delete"},{"expect":{"css":".agent-card","hasText":"TEST-Reviewer"},"count":0},{"expect":{"css":".session-row","hasText":"TEST-kept chat"}}]'
yv state --expect '{"sessions":[{"id":"{{kept}}","agent":null}]}'
```

## Gotchas

- Real engines are not exercised here: the scripted Codex echoes what the user
  typed (it strips the preamble) and gives a canned host answer. Whether a
  real model follows an agent's instructions, or cites evidence well, is not
  proven by this map.
