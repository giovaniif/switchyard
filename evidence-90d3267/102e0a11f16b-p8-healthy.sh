#!/bin/bash
# healthy.sh <checkout> <hub port> <box repo path> <out>: the hub already has TEST-box (run.sh); proxy gone,
# so this adds TEST-box2 straight to the box (no proxy) and sends ops that are answered.
set -u
CO=$1; HUB=$2; BOX=$3; REPO=$4; OUT=$5
mkdir -p "$OUT"; cd "$CO"
verify/yv api "{\"op\":\"host.add\",\"name\":\"TEST-box2\",\"kind\":\"direct\",\"url\":\"ws://127.0.0.1:$BOX/ws\"}" --iso hub > "$OUT/host-add.json"
sleep 3
bun /tmp/pipe-ev/page.ts "$HUB" TEST-box2 '{"op":"attach.put","host":"HOST","name":"TEST-ok.txt","mime":"text/plain","data":"aGVsbG8=","ref":"up2"}' 3000 > "$OUT/upload.json"
bun /tmp/pipe-ev/page.ts "$HUB" TEST-box2 "{\"op\":\"session.create\",\"host\":\"HOST\",\"engine\":\"codex\",\"model\":\"gpt-5.6-luna\",\"effort\":\"low\",\"cwd\":\"$REPO\",\"text\":\"TEST healthy create\",\"permission\":\"ask\"}" 4000 > "$OUT/create.json"
