#!/usr/bin/env bash
# loopbin.sh <test-target> <runs> <busy-procs> [filter]: whole test binary repeatedly, with CPU burners.
set -u
target=$1 runs=$2 busy=$3 filter=${4:-}
cd ${WT:-/home/giovani/www/better-than-claude-code/.claude/worktrees/flaky-tests-2}
pids=()
for _ in $(seq "$busy"); do (while :; do :; done) & pids+=($!); done
trap 'kill "${pids[@]}" 2>/dev/null' EXIT
echo "checkout $(git rev-parse --short HEAD), $(nproc) cpus"
fail=0
for i in $(seq "$runs"); do
  out=$(cargo test -q -p yardd --test "$target" $filter 2>&1)
  if echo "$out" | grep -q "test result: ok"; then echo "run $i: ok"; else fail=$((fail+1)); echo "run $i: FAILED"; echo "$out" | grep -E "^test .* FAILED|panicked|left:|right:|timed out|not connected" | cut -c1-300 | head -8; fi
done
echo "failures: $fail/$runs (busy=$busy, RUST_TEST_THREADS=${RUST_TEST_THREADS:-}, load $(cut -d' ' -f1-3 /proc/loadavg))"
