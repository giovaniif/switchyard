#!/bin/bash
# Screenshots of the demo on a booted simulator: scripts/screenshots.sh <udid> <Switchyard.app> <out dir>
set -euo pipefail
udid=$1 app=$2 out=$3
bundle=sh.giovani.switchyard.mobile
mkdir -p "$out"
xcrun simctl bootstatus "$udid" -b >/dev/null
xcrun simctl install "$udid" "$app"
xcrun simctl status_bar "$udid" override --time 9:41 --batteryState charged --batteryLevel 100 --cellularBars 4 --wifiBars 3 || true
shoot() {
  local name=$1 screen=$2
  xcrun simctl launch --terminate-running-process "$udid" "$bundle" -demo -screen "$screen" >/dev/null
  sleep "${3:-4}"
  xcrun simctl io "$udid" screenshot --type=png "$out/$name.png" >/dev/null
}
for appearance in light dark; do
  xcrun simctl ui "$udid" appearance "$appearance"
  shoot "$appearance-sessions" sessions
  shoot "$appearance-session-running" session:d1
  shoot "$appearance-voice" voice:d4
  if [ "$appearance" = light ]; then
    shoot light-connect connect
    shoot light-approval session:d2
    shoot light-markdown session:d4
    shoot light-subagent session:d3
    shoot light-failed session:d6
    shoot light-new new
    shoot light-settings settings
  fi
done
xcrun simctl terminate "$udid" "$bundle" || true
ls -la "$out"
