# Privacy

Switchyard runs on computers you own. It has no Switchyard server, account,
analytics or advertising, and collects no data about you.

## Switchyard for iPhone and iPad

- **What it connects to.** Only the Switchyard host whose address you enter (or
  scan), usually over your own private network such as Tailscale. Your
  sessions, messages, files and attachments go between the phone and that host,
  and nowhere else.
- **What it stores on the phone.** The addresses of hosts you connected to and
  your settings. No passwords or engine credentials: those stay on your host.
- **Speech.** Dictation and voice mode use Apple's speech recognition and
  synthesis. Depending on your device and language, Apple may process the audio
  on its servers under [Apple's privacy policy](https://www.apple.com/legal/privacy/).
  Only the transcribed text you send reaches your host. Recording stops when
  the app leaves the screen.
- **Notifications** are local, created on the phone while it is connected. There
  is no push service.
- **No tracking.** The app contains no analytics, crash reporting or third-party
  SDKs, and shares nothing with anyone.

## The desktop app and the daemon

They run on your machines and talk to the AI services you sign in to (such as
Claude, Codex or Grok) under those services' own terms. Switchyard itself sends
nothing elsewhere, apart from checking GitHub for app updates.

## Contact

Open an issue at https://github.com/giovaniif/switchyard/issues.
