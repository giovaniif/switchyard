#!/bin/bash
# run.sh <repo> <hub port> <box port> <out dir>: each scenario on its own host
# entry behind a fresh proxy.
set -u
repo=$1 hub=$2 box=$3 out=$4
mkdir -p "$out"
cd "$repo"
for scenario in stall removed; do
  pport=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
  echo pass > "$out/mode"
  python3 /tmp/pipe-ev/proxy.py "$pport" "$box" "$out/mode" 2> "$out/proxy-$scenario.log" &
  ppid=$!
  sleep 0.3
  verify/yv api "{\"op\":\"host.add\",\"name\":\"TEST-box-$scenario\",\"kind\":\"direct\",\"url\":\"ws://127.0.0.1:$pport/ws\"}" --iso hub \
    --wait "{\"op\":\"hosts\",\"hosts\":[{\"name\":\"TEST-box-$scenario\",\"status\":\"connected\"}]}" --timeout 20000 > "$out/add-$scenario.json"
  hid=$(jq -r --arg n "TEST-box-$scenario" '.. | objects | select(.name? == $n and .id? != null) | .id' "$out/add-$scenario.json" | head -1)
  [ "$scenario" = stall ] && echo stall > "$out/mode" || echo blackhole > "$out/mode"
  bun /tmp/pipe-ev/drive.ts "$hub" "$scenario" "$hid" > "$out/$scenario.json"
  cat "$out/$scenario.json"
  cat "$out/proxy-$scenario.log"
  [ "$scenario" = stall ] && verify/yv api "{\"op\":\"host.remove\",\"hostId\":\"$hid\"}" --iso hub > /dev/null
  kill "$ppid"
done
