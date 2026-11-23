# Switchyard for iPhone and iPad

Build operations, the server's existing Apple/EAS credentials, and TestFlight
setup: [docs/IOS_BUILDS.md](../docs/IOS_BUILDS.md). Read that before creating
new certificates or assuming the repository's secrets are the only source.

An iOS shell around the **same live UI served by your yardd**. The editor,
attachments, model/engine switching, approvals, queued messages, terminals,
changes, pull requests, processes, hosts, accounts, connectors and automations
use their existing implementation. Agents (Scout included) use that same host UI, including
its shared editor and voice controls. On narrow screens, **Workspace** opens the workspace tabs and its
close button returns to the conversation.

The native shell adds saved hosts, HTTPS connection checks, setup deep links,
Apple speech recognition/synthesis, local notifications and foreground
reconnection. It runs no engines on the phone and stores no engine credentials.
Dictation and hands-free voice use the existing composer and voice rules.

## Connect

1. Install the beta using the host page's TestFlight invitation.
2. Join the same private network as the host (for example, Tailscale).
3. Enter the **HTTPS origin** of the existing web host, such as
   `https://yard.example.ts.net`. A `switchyard://connect?host=<encoded-origin>`
   link fills the address; you still choose Connect on the phone.

The app validates `/health`, then waits for the UI's WebSocket snapshot before
saving the host. The native bridge observes a valid snapshot from the selected
host's own `/ws` socket, so older host UIs can also finish connecting. Updated
host UIs additionally report `mobile.connected` over that same socket;
yardd persists `mobile.json` in its data directory and broadcasts `mobile.status`.
The phone icon in new-session and session headers disappears across all clients
of that hub only after a successful phone connection. Hosts always retains the
setup card for additional phones. Older hosts remain usable but cannot record
setup status until updated.

`localhost` on the phone is the phone, and a desktop SSH forward is not reachable
from it. Use the existing privately reachable HTTPS deployment. This change does
not expose a daemon or change loopback binding or WebSocket Origin checks.
`YARD_ALLOW_ORIGIN` must include the web host's exact HTTPS origin. Do not put an
unauthenticated yardd on the public internet.

Set `YARD_TESTFLIGHT_URL=https://testflight.apple.com/join/<8-character-code>` on
the hub when Apple has issued the real public invitation. Until then, Hosts
shows the beta's pending state instead of a fake download link. Only invitation
URLs on `testflight.apple.com/join/` are linked.

## Build and test on a Mac

Requires Xcode 26+ with the iOS SDK and XcodeGen (`brew install xcodegen`). No new
JavaScript dependencies or Rust cross-compilation is needed.

```sh
bash mobile/scripts/prepare.sh
open mobile/Switchyard.xcodeproj
# Choose an installed iPhone simulator, then Product > Test.
```

The generated project and plist are ignored; `project.yml` is the source of
truth. The icon reuses the desktop source and is flattened to opaque RGB.
The default bundle ID is `sh.giovani.switchyard.mobile`, separate from desktop.
Register that ID with Apple, or change it consistently in `project.yml`, the
URL type, and `scripts/testflight.sh` before creating a provisioning profile.

The **iOS app** Actions workflow builds and runs unit tests on a simulator for
mobile pull requests and pushes to main. UI and native bridge JavaScript tests
run in the existing UI job. `verify/yv map run mobile` checks discovery and
workspace navigation against scripted engines at both screen sizes.
The workflow uses macOS 26 and the iOS 26+ SDK to meet
[Apple's upload requirements](https://developer.apple.com/news/upcoming-requirements/?id=04282026a).

## TestFlight first

Create the iOS app record in App Store Connect once. Switchyard's bundle ID and
App Store provisioning profile have been registered; the server's EAS account
already holds a valid **iPhone Distribution** certificate and Admin API key.
Reuse those following [the build guide](../docs/IOS_BUILDS.md); the desktop
Developer ID certificate cannot sign iOS distribution. The workflow uses these
GitHub Actions secrets, either on the repository or the `testflight` environment:

| Secret | Value |
| --- | --- |
| `IOS_CERTIFICATE` | Base64 of the distribution `.p12` with private key |
| `IOS_CERTIFICATE_PASSWORD` | The `.p12` password |
| `IOS_PROVISION_PROFILE` | Base64 of the App Store `.mobileprovision` |
| `MACOS_TEAM_ID` | The existing Apple Developer team ID |
| `APP_STORE_CONNECT_KEY_ID` | App Store Connect API key ID |
| `APP_STORE_CONNECT_ISSUER_ID` | API issuer ID |
| `APP_STORE_CONNECT_API_KEY` | Base64 of the API `.p8` private key |

Run **iOS app → Run workflow → Upload a signed beta to TestFlight** from the
reviewed branch. The simulator tests must pass before the archive/upload job.
The script imports credentials into a temporary keychain, archives for iOS,
uploads to App Store Connect with `xcodebuild -exportArchive`, then deletes
the temporary signing material. Build numbers use the run number and attempt.
No App Store release is submitted. Apple must process the build before it is
available to internal testers; external testers need Apple's beta review and a
TestFlight group/invitation. See [Apple's upload guide](https://developer.apple.com/help/app-store-connect/manage-builds/upload-builds/)
and [TestFlight](https://developer.apple.com/testflight/).

## Device verification before distributing

CI cannot prove hardware microphone behavior. On a physical iPhone, check:

- First-run microphone and Speech Recognition permission prompts, refusal,
  dictation with interim results, editing while dictating and sending.
- Voice mode: pause to send, answer readback, talking over it, Stop, approvals
  announced without answering them, permission denial and audio interruptions.
- Background/foreground recovery with an active host turn and drafts retained;
  host switching, airplane mode, expired/untrusted TLS and a blocked WebSocket.
- Camera/photo/file attachments via the system file picker, selection/quoting,
  keyboard layout in portrait/landscape and workspace tabs including a PTY.
- Opt-in notifications and notification taps while connected.

Speech recognition can send audio to Apple depending on device/language; only
transcribed text sent by the composer reaches yardd. Recording and speech stop
when backgrounded. Local notifications use the live socket: **there is no APNs
push service**, so suspended/terminated apps do not receive new session alerts.
Sessions themselves continue on the host. Native bridge calls are accepted only
from the main frame at the selected HTTPS origin; external links open outside
the app, and connector iframes receive no native bridge.
