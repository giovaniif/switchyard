# Attachments

Files go with a prompt: paste, drop or the paperclip. Each uploads at once
(`attach.put`) and shows as a chip; images get a preview. 10 MB per file,
checked in the UI (red chip, Send disabled) and by yardd. Sent images show as
thumbnails in the thread; Codex gets image paths as `localImage` items.

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

### upload-image-and-send
An image picked with the paperclip shows as a chip, goes with the prompt, shows
as a thumbnail, and reaches the engine as a localImage.

```sh
printf 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==' | base64 -d > "$(v work)/TEST-dot.png"
yv drive new --viewport desktop --steps '[{"click":"css:.account-trigger"},{"click":{"role":"menuitemradio","name":"Codex · Default"}},{"press":"Escape"},{"click":"role:button:work"},{"fill":"role:textbox:Folder","value":"{{repo}}"},{"press":"Enter"},{"upload":"input","files":["{{work}}/TEST-dot.png"]},{"expect":"css:.attachments","text":"TEST-dot.png"},{"fill":"label:Message","value":"TEST-look at this"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-look at this"},{"expect":"role:img:TEST-dot.png"}]'
grep -q localImage "$(v fake)/turn-inputs.jsonl"
```

### size-cap
Viewports: desktop
An 11 MB file shows a red chip and Send stays disabled.

```sh
head -c 11000000 /dev/zero > "$(v work)/TEST-big.bin"
yv drive new --viewport desktop --steps '[{"upload":"input","files":["{{work}}/TEST-big.bin"]},{"expect":"css:.attachment-error"},{"fill":"label:Message","value":"TEST-too big"},{"expect":{"role":"button","name":"Send"},"enabled":false}]'
```

### upload-over-api
`attach.put` stores a file and answers with its id.

```sh
yv api '{"op":"attach.put","ref":"r1","name":"TEST-note.txt","mime":"text/plain","data":"VEVTVC1ub3Rl"}' --wait '{"op":"attach.stored","ref":"r1"}' --expect '{"attachment":{"name":"TEST-note.txt"}}' --save att_id=attachment.id
```

### too-many-files-keeps-the-draft
More than 20 files on one message (yardd's per-message cap) says so in the
composer and keeps Send off; Enter keeps the message and the files. Removing
one turns Send back on. Before, Send went through, yardd refused it, and the
composer had already cleared the message and every chip.

```sh
for i in $(seq -w 1 21); do echo "TEST-note $i" > "$(v work)/TEST-many-$i.txt"; done
yv drive 'session:{{sid_echo}}' --steps '[{"upload":"input","files":["{{work}}/TEST-many-01.txt","{{work}}/TEST-many-02.txt","{{work}}/TEST-many-03.txt","{{work}}/TEST-many-04.txt","{{work}}/TEST-many-05.txt","{{work}}/TEST-many-06.txt","{{work}}/TEST-many-07.txt","{{work}}/TEST-many-08.txt","{{work}}/TEST-many-09.txt","{{work}}/TEST-many-10.txt","{{work}}/TEST-many-11.txt","{{work}}/TEST-many-12.txt","{{work}}/TEST-many-13.txt","{{work}}/TEST-many-14.txt","{{work}}/TEST-many-15.txt","{{work}}/TEST-many-16.txt","{{work}}/TEST-many-17.txt","{{work}}/TEST-many-18.txt","{{work}}/TEST-many-19.txt","{{work}}/TEST-many-20.txt","{{work}}/TEST-many-21.txt"]},{"expect":"css:.composer .attachments","text":"TEST-many-21.txt"},{"fill":"label:Message","value":"TEST-twenty one files"},{"expect":"css:.attachments-error","text":"Too many files: 21. The limit is 20 per message."},{"expect":{"role":"button","name":"Send"},"enabled":false},{"press":"Enter"},{"expect":"label:Message","value":"TEST-twenty one files"},{"expect":"css:.attachment","count":21},{"click":"role:button:Remove TEST-many-21.txt"},{"expect":"css:.attachments-error","visible":false},{"expect":{"role":"button","name":"Send"},"enabled":true}]'
```

### queued-files-come-back
A message queued with a file during a turn comes back with its file: Edit
puts the text and the chip back in the reply box, and so does Stop. Before,
only the text came back and the file was dropped.

```sh
echo "TEST-note" > "$(v work)/TEST-queued.txt"
steps='[{"fill":"label:Message","value":"[slow] TEST-long job"},{"click":"role:button:Send"},{"expect":{"role":"button","name":"Add to queue"}},{"upload":"input","files":["{{work}}/TEST-queued.txt"]},{"expect":"css:.composer .attachment","text":"TEST-queued.txt"},{"fill":"label:Message","value":"TEST-read this file"},{"click":{"role":"button","name":"Add to queue"}},{"expect":"css:.pending-row","text":"1 file"},{"expect":"css:.composer .attachment","count":0},{"click":{"css":".pending-row .pending-action","hasText":"Edit"}},{"expect":"label:Message","value":"TEST-read this file"},{"expect":"css:.composer .attachment","text":"TEST-queued.txt"},{"click":{"role":"button","name":"Add to queue"}},{"expect":"css:.pending-row","text":"1 file"},{"click":{"role":"button","name":"Stop","exact":true}},{"expect":"label:Message","value":"TEST-read this file"},{"expect":"css:.composer .attachment","text":"TEST-queued.txt"},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-read this file"},{"expect":"css:.attachments-sent","text":"TEST-queued.txt"}]'
for vp in mobile desktop; do
  yv api '{"op":"session.interrupt","sessionId":"{{sid_echo}}"}'
  yv drive 'session:{{sid_echo}}' --viewport $vp --steps "$steps"
done
```

### many-files-keep-send-on-screen
Twenty files (the most one message takes) keep the composer inside the
screen: the chips scroll in their own row, so Send and the session header
stay in view. Before, at 390 px the chips stacked one per row, the composer
grew to 1132 px, the header scrolled off the top and Send sat below the fold.

```sh
for i in $(seq -w 1 20); do echo "TEST-note $i" > "$(v work)/TEST-many-$i.txt"; done
yv drive 'session:{{sid_echo}}' --steps '[{"upload":"input","files":["{{work}}/TEST-many-01.txt","{{work}}/TEST-many-02.txt","{{work}}/TEST-many-03.txt","{{work}}/TEST-many-04.txt","{{work}}/TEST-many-05.txt","{{work}}/TEST-many-06.txt","{{work}}/TEST-many-07.txt","{{work}}/TEST-many-08.txt","{{work}}/TEST-many-09.txt","{{work}}/TEST-many-10.txt","{{work}}/TEST-many-11.txt","{{work}}/TEST-many-12.txt","{{work}}/TEST-many-13.txt","{{work}}/TEST-many-14.txt","{{work}}/TEST-many-15.txt","{{work}}/TEST-many-16.txt","{{work}}/TEST-many-17.txt","{{work}}/TEST-many-18.txt","{{work}}/TEST-many-19.txt","{{work}}/TEST-many-20.txt"]},{"expect":"css:.composer .attachments","text":"TEST-many-20.txt"},{"fill":"label:Message","value":"TEST-twenty files"},{"expect":{"role":"button","name":"Send"},"inViewport":true},{"expect":"role:button:Delete","inViewport":true},{"click":"role:button:Send"},{"expect":"css:.thread","text":"Echo: TEST-twenty files"}]'
```

### paste-and-drop (manual: clipboard and OS drag events are not scripted)
Pasting an image and dropping a file on the composer add chips the same way.

## Gotchas
- The queued row's Edit button has no class of its own since #294 (Steer and
  Edit share `.pending-action`). Recipes find it by its text. `css:.pending-edit`
  timed out, and the turn it left running made `many-files-keep-send-on-screen`
  fail after it. Fixed on 2026-11-20.
- The file input is hidden; the `upload` step sets its files directly.
