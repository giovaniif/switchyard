#!/usr/bin/env bash
# loop.sh <test-target> <test-name> <runs> <busy-procs>
# Runs one yardd test repeatedly with <busy-procs> CPU burners alongside, inside the caller's scope.
set -u
target=$1 name=$2 runs=$3 busy=$4
cd ${WT:-/home/giovani/www/better-than-claude-code/.claude/worktrees/flaky-tests-2}
pids=()
for _ in $(seq "$busy"); do (while :; do :; done) & pids+=($!); done
trap 'kill "${pids[@]}" 2>/dev/null' EXIT
echo "checkout $(git rev-parse --short HEAD), $(nproc) cpus"
fail=0
for i in $(seq "$runs"); do
  out=$(cargo test -q -p yardd --test "$target" "$name" -- --exact 2>&1)
  if echo "$out" | grep -q "test result: ok. 1 passed"; then echo "run $i: ok"; else fail=$((fail+1)); echo "run $i: FAILED"; echo "$out" | grep -E -A2 "panicked" | cut -c1-300 | head -6; fi
done
echo "failures: $fail/$runs (busy=$busy, load $(cut -d' ' -f1-3 /proc/loadavg))"
