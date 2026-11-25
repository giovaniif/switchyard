# Switchyard for iPhone and iPad

Build operations, the server's existing Apple/EAS credentials, and TestFlight
setup: [docs/IOS_BUILDS.md](../docs/IOS_BUILDS.md). Read that before creating
new certificates or assuming the repository's secrets are the only source.

A native SwiftUI app (iOS 26, Liquid Glass) that talks to your yardd over its
WebSocket, the same protocol as the web page. It runs no engines on the phone
and stores no engine credentials.

- **Sessions**: one list with live status (working, needs you, failed),
  search, pin and archive by swiping, and the host's CPU and memory.
- **A session**: the thread as it streams (Markdown, thinking, tool steps with
  subagents nested, edited files), approvals as cards (Allow once, Always,
  Deny), queued messages, Stop, model, effort and permission.
- **New session**: the prompt first, then engine, model and effort, folder
  (recent folders and a browser of the host's), its own worktree, permission,
  and the host when remotes are connected.
- **Voice**: dictation (the mic in the reply box) and voice mode (the waveform
  button): it listens, sends after a pause, reads the answer aloud and listens
  again; talk over it or say "stop". Approvals are announced and answered on
  screen. Voice mode started on New session goes on in the session it opens.
- **Workspace**: terminals, changes, pull requests and processes stay in the
  host's web page, opened in a sheet from a session's menu.
- **Demo**: "Look around with demo sessions" on the connect screen shows the
  app with bundled sessions and no host.

## Code

| Folder | What |
| --- | --- |
| `Switchyard/Core` | `JSON` (lenient decoding), `Models`, `Thread` (`applyEvent` ported from `ui/src/protocol.ts`), `Transport` (the WebSocket), `Yard` (the store, like `ui/src/store.ts`), `Markdown` |
| `Switchyard/Voice` | `VoiceRules` (ported from `ui/src/voice.ts`), `SpeechIO` (microphone, recognizer, voice), `VoiceController` (the state machine's effects) |
| `Switchyard/UI` | The screens |
| `Switchyard/Demo` | `demo.json` and a pretend yardd for the demo and screenshots |
| `SwitchyardTests` | Unit tests; `Fixtures/yardd-capture.json` is a snapshot and histories recorded from a yardd with scripted engines |

Message shapes live in `ui/src/protocol.ts` and yardd. When they change, change
`Core/Models.swift` and `Core/Thread.swift` too; the fixture test reads real
messages, so recapture it (a script that opens `/ws` on a `verify/yv iso`
daemon, sends `session.open` for each session and saves the replies).

## Voice on the phone

`SpeechIO` runs one `AVAudioEngine` for both directions. The microphone feeds
`SFSpeechRecognizer` (on-device when the phone supports it: private, no
one-minute limit). Answers are written by `AVSpeechSynthesizer` into buffers
and played through the same engine with **voice processing** on, so Apple's
echo cancellation keeps the phone from hearing itself and you can talk over
an answer. The transcript is also checked against what is being read (the
web's echo rule), and voice that writes no audio falls back to plain playback.

A recognition run gives one transcript that grows while it listens.
Words already used (an echo, text typed over) are counted, not indexed by
result; that is what kept voice mode from ever sending in WebKit browsers.

Speech and the microphone stop when the app goes to the background. Sessions
keep running on the host.

## Connect

1. Install the beta using the host page's TestFlight invitation.
2. Join the same private network as the host (for example, Tailscale).
3. On the desktop, open **Hosts**: the iPhone card shows a QR code. Point the
   iPhone Camera at it; Switchyard opens with the address filled in
   (`switchyard://connect?host=<encoded-origin>`), and you tap Connect. Or type
   the host's **HTTPS origin**, such as `https://yard.example.ts.net`.

The card's address is each host's `YARD_ALLOW_ORIGIN` entries a phone can open
(HTTPS, not loopback), reported in the host's `mobile` state; the desktop
app's own page is on loopback, so remote hosts relay theirs to the hub. With
none, the card asks for the address. It also says which phone is connected
now, or when one was last seen: the app sends `mobile.connected` with its
device name on every connection, and yardd counts it live until the socket
closes (`mobile.json` keeps the last five phones).

The app checks `/health`, then opens `wss://<host>/ws` with the host's own
origin as `Origin`, which `YARD_ALLOW_ORIGIN` already lists for its web page.
The host is saved once the first snapshot arrives. A host that tracks phones
gets `mobile.connected`, and the desktop's phone shortcut goes away.

`localhost` on the phone is the phone, and a desktop SSH forward is not
reachable from it. Use the existing privately reachable HTTPS deployment. Do
not put an unauthenticated yardd on the public internet.

Set `YARD_TESTFLIGHT_URL=https://testflight.apple.com/join/<8-character-code>`
on the hub when Apple has issued the real public invitation. Until then, Hosts
shows the beta's pending state instead of a fake download link.

## Build and test on a Mac

Requires Xcode 27+ (the iOS 27 SDK, which App Store Connect requires) with the iOS SDK and XcodeGen (`brew install xcodegen`).

```sh
bash mobile/scripts/prepare.sh
open mobile/Switchyard.xcodeproj
# Choose an iPhone simulator, then Product > Test.
```

The generated project and plist are ignored; `project.yml` is the source of
truth. Launch arguments `-demo -screen <name>` open the demo on a screen
(`sessions`, `session:<id>`, `voice:<id>`, `new`, `settings`, `connect`);
`scripts/screenshots.sh` uses them.

The **iOS app** workflow builds, runs the unit tests on a simulator, then
takes light and dark screenshots of the demo (artifact
`Switchyard-iOS-screenshots`). This server has no Xcode, so that artifact is
how changes are seen from here:
`gh run download <run> --repo giovaniif/switchyard -n Switchyard-iOS-screenshots`.

## TestFlight

Run **iOS app → Run workflow → Upload a signed beta to TestFlight** from the
reviewed branch (see [the build guide](../docs/IOS_BUILDS.md)). The simulator
job must pass first. Build numbers use the run number and attempt.

## Device verification before distributing

CI cannot prove hardware microphone behavior. On a physical iPhone, check:

- First-run Speech Recognition and microphone prompts, refusal (the error
  offers Settings), dictation with words appearing as you speak, typing while
  dictating, and sending.
- Voice mode: the greeting, pause to send, the answer read aloud on the
  speaker, talking over it, "stop", the Stop and X buttons, an approval
  announced and answered on screen, a failed turn, a phone call interrupting.
- Background/foreground with a running turn; switching hosts; airplane mode.
- Photo and file attachments; approvals; queued messages and Edit.
- Notifications (Settings) for a session that finishes while another is open.

Speech recognition runs on the phone when it can; otherwise Apple's servers
see the audio. Only the text you send reaches yardd. Local notifications come
from the live socket: **there is no APNs push service**, so a suspended app
is not told.
