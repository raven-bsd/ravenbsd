#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K297 acceptance: pf.conf -> pfctl -> raven-pf -> tcpdump end to end.
# Builds the two product tools, runs the K277 matrix through the harness, then
# proves the harness catches a mutated verdict and a corrupted capture.
# Artifacts: build/pf-e2e/{log.txt,report.json,*.pcap,hashes.txt}.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../.." && pwd)
out="$root/build/pf-e2e"
harness="$here/harness/Cargo.toml"
rm -rf "$out"
mkdir -p "$out"

echo "== build the product tools =="
(cd "$root" && timeout 120 cargo build -q -p raven-pfctl -p raven-tcpdump)

echo "== pfctl binary load path =="
"$root/target/debug/raven-pfctl" -nf "$root/tests/fixtures/pfctl/pf.conf" \
    >"$out/pfctl-valid.txt" 2>&1
if "$root/target/debug/raven-pfctl" -nf \
    "$root/tests/fixtures/pfctl/invalid/bad-address.conf" \
    >"$out/pfctl-invalid.txt" 2>&1; then
    echo "FAIL: pfctl accepted the invalid fixture" >&2
    exit 1
fi
echo "pfctl-binary: PASS"

echo "== full pf.conf -> pfctl -> engine -> tcpdump path =="
if ! CARGO_TARGET_DIR="$root/target/pf-e2e" \
    timeout 180 cargo run --offline --quiet --manifest-path "$harness" -- \
    "$root" >"$out/log.txt" 2>&1; then
    cat "$out/log.txt" >&2
    echo "FAIL: e2e harness" >&2
    exit 1
fi
tail -5 "$out/log.txt"

echo "== mutation: wrong expected verdict =="
if ! CARGO_TARGET_DIR="$root/target/pf-e2e" \
    timeout 180 cargo run --offline --quiet --manifest-path "$harness" -- \
    --mutate-verdict "$root" >"$out/log-mutate-verdict.txt" 2>&1; then
    cat "$out/log-mutate-verdict.txt" >&2
    echo "FAIL: the harness did not catch the verdict mutation" >&2
    exit 1
fi
grep -m1 'mutation caught' "$out/log-mutate-verdict.txt"

echo "== mutation: corrupted capture =="
if ! CARGO_TARGET_DIR="$root/target/pf-e2e" \
    timeout 180 cargo run --offline --quiet --manifest-path "$harness" -- \
    --mutate-pcap "$root" >"$out/log-mutate-pcap.txt" 2>&1; then
    cat "$out/log-mutate-pcap.txt" >&2
    echo "FAIL: the harness did not catch the capture mutation" >&2
    exit 1
fi
grep -m1 'mutation caught' "$out/log-mutate-pcap.txt"

echo "== evidence hashes =="
{
    echo "# K297 e2e artifact hashes"
    sha256sum \
        "$root/tests/pf-conformance/manifest.tsv" \
        "$root/tests/pf-conformance/semantics.tsv" \
        "$out/report.json"
    for pcap in "$out"/*.pcap; do
        sha256sum "$pcap"
    done
} >"$out/hashes.txt"
cat "$out/hashes.txt"

echo "pf-e2e: PASS"
