#!/bin/bash
# Live check: an isolated yardd (scripted engines, sweep every 2 s) removes an archived session's worktree and branch.
set -eu
cd /home/giovani/.yard/wt/better-than-claude-code/yard-the-ux-patrol-automation-on-4773
verify/yv iso up --name sweepdemo --env YARD_WORKTREE_SWEEP_SECS=2 >/dev/null
trap 'verify/yv iso down --name sweepdemo >/dev/null' EXIT
R=$(mktemp -d /tmp/sweepdemo-repo.XXXX)
git -C $R init -q -b main; echo hi > $R/a.txt; git -C $R add .; git -C $R -c user.email=t@t -c user.name=t commit -qm init
sid=$(verify/yv api "{\"op\":\"session.create\",\"cwd\":\"$R\",\"text\":\"hello\",\"engine\":\"codex\",\"model\":\"fake-fast\",\"effort\":\"low\",\"worktree\":true}" --wait '{"op":"created"}' --iso sweepdemo | python3 -c "import json,sys;print(json.load(sys.stdin)['reply']['sessionId'])")
sleep 4
cwd=$(verify/yv state --iso sweepdemo | python3 -c "import json,sys;print([x for x in json.load(sys.stdin)['sessions'] if x['id']=='$sid'][0]['cwd'])")
[ -d "$cwd" ] || { echo "worktree never made"; exit 1; }
sleep 5
[ -d "$cwd" ] || { echo "FAIL: listed session's worktree was removed"; exit 1; }
echo "listed session: worktree $cwd kept after 9s"
verify/yv api "{\"op\":\"session.update\",\"sessionId\":\"$sid\",\"archived\":true}" --iso sweepdemo >/dev/null
for i in $(seq 1 20); do [ -d "$cwd" ] || break; sleep 1; done
[ -d "$cwd" ] && { echo "FAIL: archived session's worktree still there after 20s"; exit 1; }
sleep 2
echo "archived: worktree gone after ${i}s; branches left: $(git -C $R branch --format='%(refname:short)' | tr '\n' ' ')"
[ "$(git -C $R branch --format='%(refname:short)')" = "main" ] || { echo "FAIL: branch left behind"; exit 1; }
rm -rf $R
