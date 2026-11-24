#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Durable negative check for the fixture checker and packet harness. Both
# mutations happen in a temporary copy; the repository is never modified.
# The checker/harness must fail on each mutation, otherwise this script fails.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/pf-conformance-negative.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

mkdir -p "$tmp/tests/fixtures"
cp -R "$root/tests/pf-conformance" "$tmp/tests/"
cp -R "$root/tests/fixtures/pfctl" "$tmp/tests/fixtures/"
cp -R "$root/tests/fixtures/tcpdump" "$tmp/tests/fixtures/"

echo "-- mutation: pcap SHA-256 in the manifest"
zeros=0000000000000000000000000000000000000000000000000000000000000000
sed -i "0,/|[0-9a-f]\\{64\\}|/s//|$zeros|/" "$tmp/tests/fixtures/tcpdump/manifest.tsv"
if timeout 30 python3 "$tmp/tests/pf-conformance/check_fixtures.py" --root "$tmp" >"$tmp/checker.out" 2>&1; then
    echo "FAIL checker accepted a corrupted pcap digest" >&2
    exit 1
fi
if ! grep -q 'sha256' "$tmp/checker.out"; then
    echo "FAIL checker failed without naming the bad sha256" >&2
    sed -n '1,20p' "$tmp/checker.out" >&2
    exit 1
fi
grep -m1 'FAIL ' "$tmp/checker.out"
echo "checker-negative: PASS (corrupted digest rejected)"

echo "-- mutation: expected packet verdict"
sed -i '0,/ pass  created/s// block created/' "$tmp/tests/pf-conformance/packets/stateful-tcp.tsv"
if CARGO_TARGET_DIR="$root/target/pf-conformance-harness" \
    timeout 60 cargo run --offline --quiet \
    --manifest-path "$here/harness/Cargo.toml" -- "$tmp/tests/pf-conformance" \
    >"$tmp/harness.out" 2>&1; then
    echo "FAIL harness accepted a corrupted expected verdict" >&2
    exit 1
fi
if ! grep -q 'FAIL stateful-tcp' "$tmp/harness.out"; then
    echo "FAIL harness failed without naming stateful-tcp" >&2
    sed -n '1,20p' "$tmp/harness.out" >&2
    exit 1
fi
grep -m1 'FAIL stateful-tcp' "$tmp/harness.out"
echo "harness-negative: PASS (corrupted verdict rejected)"
