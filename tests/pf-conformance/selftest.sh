#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K277 acceptance: deterministic pf/tooling fixtures.
# Runs the schema/pcap checker, regenerates the pcaps and compares them
# byte-for-byte, then drives the supported packet matrix through raven-pf.
# Donor-only scenarios print SKIP with their recorded reason.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/pf-conformance.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

echo "== K277 fixture integrity =="
cd "$root"
timeout 30 python3 "$here/check_fixtures.py" --root "$root"

echo "== pcap determinism =="
timeout 30 python3 "$root/tests/fixtures/tcpdump/generate.py" --out "$tmp/pcaps" >/dev/null
for pcap in "$root"/tests/fixtures/tcpdump/*.pcap; do
    name=$(basename "$pcap")
    if ! cmp -s "$pcap" "$tmp/pcaps/$name"; then
        echo "FAIL pcap not reproducible: $name" >&2
        exit 1
    fi
done
echo "pcap-determinism: PASS"

echo "== raven-pf packet matrix =="
CARGO_TARGET_DIR="$root/target/pf-conformance-harness" \
    timeout 60 cargo run --offline --quiet \
    --manifest-path "$here/harness/Cargo.toml" -- "$here"

echo "== negative check =="
timeout 60 sh "$here/negative_check.sh"

echo "selftest: PASS"
