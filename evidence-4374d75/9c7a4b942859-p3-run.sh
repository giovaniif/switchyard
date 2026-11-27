#!/bin/bash
# run.sh <checkout> <hub port> <box port> <box sid waiting for approval> <its requestId> <out dir>
set -u
CO=$1; HUB=$2; BOX=$3; SID=$4; RID=$5; OUT=$6
mkdir -p "$OUT"; cd "$CO"
PP=$(python3 -c 'import socket
while True:
    s=socket.socket(); s.bind(("127.0.0.1",0)); p=s.getsockname()[1]; s.close()
    if not 4510<=p<=4559 and p!=47717: print(p); break')
echo pass > "$OUT/mode"; : > "$OUT/proxy.log"
python3 /tmp/pipe-ev/proxy.py "$PP" "$BOX" "$OUT/mode" "$OUT/proxy.log" & PROXY=$!
echo "proxy pid $PROXY port $PP" > "$OUT/run.txt"
sleep 0.5
verify/yv api "{\"op\":\"host.add\",\"name\":\"TEST-box\",\"kind\":\"direct\",\"url\":\"ws://127.0.0.1:$PP/ws\"}" --iso hub > "$OUT/host-add.json"
sleep 3
echo swallow > "$OUT/mode"
bun /tmp/pipe-ev/page.ts "$HUB" TEST-box '{"op":"attach.put","host":"HOST","name":"TEST-note.txt","mime":"text/plain","data":"aGVsbG8gZnJvbSB0aGUgaHVi","ref":"up"}' 8000 > "$OUT/upload.json"
sleep 1
echo swallow > "$OUT/mode"
bun /tmp/pipe-ev/page.ts "$HUB" TEST-box "{\"op\":\"session.approve\",\"sessionId\":\"HOST~$SID\",\"requestId\":\"$RID\",\"decision\":\"accept\"}" 8000 > "$OUT/approve.json"
# Did the box do it? The approval is no longer pending there.
verify/yv api "{\"op\":\"session.open\",\"sessionId\":\"$SID\"}" --iso box --wait '{"op":"history"}' | python3 -c "
import json,sys; r=json.load(sys.stdin)['reply']
print(json.dumps({'box_approval_events':[{k:e[k] for k in ('type','requestId','decision') if k in e} for e in r.get('events',[]) if e.get('type','').startswith('approval')]}))" > "$OUT/box-after.json"
kill $PROXY
cat "$OUT/proxy.log"
