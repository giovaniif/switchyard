#!/bin/bash
# setup.sh <checkout> <out dir>: an isolated hub and an isolated box (scripted
# engines), the box added to the hub as a direct host behind proxy.py.
set -u
CO=$1; OUT=$2
mkdir -p "$OUT"; cd "$CO" || exit 1
verify/yv iso down --all > /dev/null 2>&1
verify/yv iso up --name hub > "$OUT/hub-up.json" || { cat "$OUT/hub-up.json"; exit 1; }
verify/yv iso up --name box --host-name TEST-vbox > "$OUT/box-up.json" || { cat "$OUT/box-up.json"; exit 1; }
BOX=$(jq -r .iso.port "$OUT/box-up.json")
PP=$(python3 -c 'import socket
while True:
    s=socket.socket(); s.bind(("127.0.0.1",0)); p=s.getsockname()[1]; s.close()
    if not 4510<=p<=4559 and p!=47717: print(p); break')
: > "$OUT/armed"; : > "$OUT/proxy.log"
nohup python3 /tmp/pipe-ev2/proxy.py "$PP" "$BOX" "$OUT/armed" "$OUT/proxy.log" > /dev/null 2>&1 9>&- &
echo $! > "$OUT/proxy.pid"
sleep 0.5
verify/yv api "{\"op\":\"host.add\",\"name\":\"TEST-vbox\",\"kind\":\"direct\",\"url\":\"ws://127.0.0.1:$PP/ws\"}" --iso hub > "$OUT/host-add.json"
sleep 3
verify/yv state --iso hub --full | jq -c '{hosts: .state.hosts}' 
echo "hub $(jq -r .iso.port "$OUT/hub-up.json") box $BOX proxy $PP pid $(cat "$OUT/proxy.pid")"
