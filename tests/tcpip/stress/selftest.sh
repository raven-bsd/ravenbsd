#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K525 script-level selftest for the stress preflight. It never builds an image and never
# boots QEMU: it proves the cache key, the missing-artefact failure, the cache hit/miss
# decision and the generated cycle case. The real warm/cold/smoke runs are separate.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
cd "$root"

py=tests/tcpip/stress/stress_preflight.py
[ -f "$py" ] || { echo "stress-selftest: missing $py" >&2; exit 1; }

echo "== pure selftest"
python3 "$py" --selftest || { echo "stress-selftest: pure selftest failed" >&2; exit 1; }

echo "== deterministic key"
first=$(python3 "$py" --key --mode kvm2)
second=$(python3 "$py" --key --mode kvm2)
[ -n "$first" ] || { echo "stress-selftest: empty cache key" >&2; exit 1; }
[ "$first" = "$second" ] || {
    echo "stress-selftest: cache key is not deterministic: $first != $second" >&2
    exit 1
}

echo "== missing ssh fails with the exact path"
missing=/tmp/opencode/k525-no-such-ssh
rm -f "$missing"
if STRESS_SSH_BIN="$missing" python3 "$py" --dry-run --mode kvm2 \
        > /tmp/opencode/k525-selftest-missing-ssh.log 2>&1; then
    echo "stress-selftest: missing ssh was accepted" >&2
    exit 1
fi
grep -F "$missing" /tmp/opencode/k525-selftest-missing-ssh.log >/dev/null || {
    echo "stress-selftest: missing-ssh error did not name $missing" >&2
    cat /tmp/opencode/k525-selftest-missing-ssh.log >&2
    exit 1
}

echo "== one-cycle case is rendered from the unchanged case"
out=/tmp/opencode/k525-selftest-case.toml
python3 "$py" --render-case --mode kvm2 --cycles 1 --out "$out"
grep -F '/etc/tcp-soak-workload.sh 1 5 7000' "$out" >/dev/null || {
    echo "stress-selftest: one-cycle case keeps the 8-cycle workload" >&2
    exit 1
}
grep -F "wait = '^tcp-cycle-done: 1$'" "$out" >/dev/null || {
    echo "stress-selftest: one-cycle case lacks the cycle-1 heartbeat" >&2
    exit 1
}
if grep -F 'tcp-cycle: 2' "$out" >/dev/null; then
    echo "stress-selftest: one-cycle case still expects cycle 2" >&2
    exit 1
fi

echo "== warm-cache decision is zero-build"
cache=$(mktemp -d /tmp/opencode/k525-cache.XXXXXX)
python3 "$py" --selftest-cache "$cache" --mode kvm2 || {
    echo "stress-selftest: cache decision selftest failed" >&2
    rm -rf "$cache"
    exit 1
}
rm -rf "$cache"

echo "stress-selftest: PASS"
