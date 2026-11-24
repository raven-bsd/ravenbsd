#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K299 tcpdump conformance against the K277 fixture oracle. The harness builds the merged
# K281 `raven-tcpdump`, verifies the committed pcap hashes, exact-diffs every reference
# decode, runs the filter matrix and checks the malformed/negative fixtures.
#
# Usage: sh tests/tcpdump-conformance/run.sh
# Artifacts: stdout (checks + hashes), temporary files under $TMPDIR on failure.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
fixtures="$root/tests/fixtures/tcpdump"
target_dir=${CARGO_TARGET_DIR:-"$root/target"}
bin="$target_dir/debug/raven-tcpdump"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/tcpdump-conformance.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

checks=0
failures=0

pass() {
    checks=$((checks + 1))
}

fail() {
    checks=$((checks + 1))
    failures=$((failures + 1))
    printf 'FAIL: %s\n' "$*" >&2
}

printf 'tcpdump-conformance: building raven-tcpdump\n'
if ! (cd "$root" && cargo build -p raven-tcpdump --quiet); then
    printf 'FAIL: cargo build -p raven-tcpdump\n' >&2
    exit 1
fi
if [ ! -x "$bin" ]; then
    printf 'FAIL: missing binary %s\n' "$bin" >&2
    exit 1
fi

# 1. Manifest: sha256, link type and packet count.
while IFS='|' read -r file packets linktype sha decoder note; do
    case "$file" in
        ''|'#'*) continue ;;
    esac
    path="$fixtures/$file"
    if [ ! -f "$path" ]; then
        fail "manifest: missing $file"
        continue
    fi
    actual=$(sha256sum "$path" | awk '{print $1}')
    if [ "$actual" = "$sha" ]; then
        pass
        printf 'sha256 ok %s %s\n' "$actual" "$file"
    else
        fail "manifest: sha256 $file: got $actual, want $sha"
    fi
    if [ "$linktype" = "ethernet" ]; then
        network=$(od -An -tu4 -j20 -N4 "$path" | tr -d ' ')
        if [ "$network" = "1" ]; then
            pass
        else
            fail "manifest: link type $file: got $network, want 1"
        fi
    fi
    verbose=""
    case "$decoder" in
        *" -v"*) verbose="-v" ;;
    esac
    out="$tmp/${file%.pcap}.out"
    err="$tmp/${file%.pcap}.err"
    if "$bin" -nn -tt $verbose -r "$path" >"$out" 2>"$err"; then
        pass
    else
        fail "decode: $file exited non-zero: $(cat "$err")"
        continue
    fi
    decoded=$(grep -cE '^[0-9]+\.[0-9]{6} ' "$out" || true)
    if [ "$decoded" = "$packets" ]; then
        pass
    else
        fail "decode: $file packet count: got $decoded, want $packets"
    fi
    reference="$fixtures/${file%.pcap}.tcpdump"
    if diff -u "$reference" "$out" >"$tmp/${file%.pcap}.diff"; then
        pass
    else
        fail "decode: $file differs from $reference"
        sed -n '1,20p' "$tmp/${file%.pcap}.diff" >&2
    fi
done <"$fixtures/manifest.tsv"

# 2. Filters used by the K277 pf scenarios.
while IFS='|' read -r file filter expected; do
    case "$file" in
        ''|'#'*) continue ;;
    esac
    out="$tmp/filter.out"
    err="$tmp/filter.err"
    if "$bin" -nn -tt -r "$fixtures/$file" "$filter" >"$out" 2>"$err"; then
        actual=$(wc -l <"$out" | tr -d ' ')
        if [ "$actual" = "$expected" ]; then
            pass
            printf 'filter ok %s %s -> %s\n' "$file" "$filter" "$actual"
        else
            fail "filter: $file '$filter': got $actual packets, want $expected"
        fi
    else
        fail "filter: $file '$filter' exited non-zero: $(cat "$err")"
    fi
done <"$fixtures/filters.tsv"

# 3. Malformed captures must fail with the declared reason (K277 oracle).
while IFS='|' read -r file expected; do
    case "$file" in
        ''|'#'*) continue ;;
    esac
    out="$tmp/malformed.out"
    err="$tmp/malformed.err"
    set +e
    "$bin" -nn -tt -r "$fixtures/$file" >"$out" 2>"$err"
    status=$?
    set -e
    if [ "$status" -eq 0 ]; then
        fail "malformed: $file decoded successfully, want rejection"
    elif grep -qF "$expected" "$err"; then
        pass
        printf 'malformed ok %s (%s)\n' "$file" "$expected"
    else
        fail "malformed: $file: stderr does not contain '$expected': $(cat "$err")"
    fi
done <"$fixtures/malformed.tsv"

# 4. K281 negative additions: unknown link type and bad IPv4 checksum.
while IFS='|' read -r file mode expected; do
    case "$file" in
        ''|'#'*) continue ;;
    esac
    path="$fixtures/negative/$file"
    out="$tmp/negative.out"
    err="$tmp/negative.err"
    set +e
    "$bin" -nn -tt -v -r "$path" >"$out" 2>"$err"
    status=$?
    set -e
    if [ "$mode" = "error" ]; then
        if [ "$status" -eq 0 ]; then
            fail "negative: $file decoded successfully, want error"
        elif grep -qF "$expected" "$err"; then
            pass
            printf 'negative ok %s (%s)\n' "$file" "$expected"
        else
            fail "negative: $file: stderr does not contain '$expected': $(cat "$err")"
        fi
    else
        if [ "$status" -ne 0 ]; then
            fail "negative: $file exited non-zero: $(cat "$err")"
        elif grep -qF "$expected" "$out"; then
            pass
            printf 'negative ok %s (%s)\n' "$file" "$expected"
        else
            fail "negative: $file: stdout does not contain '$expected'"
        fi
    fi
done <"$fixtures/negative/negatives.tsv"

# 5. Count limit and a filter that must reject.
out="$tmp/count.out"
if "$bin" -nn -tt -c 3 -r "$fixtures/ipv4-tcp-handshake.pcap" >"$out" 2>"$tmp/count.err"; then
    actual=$(wc -l <"$out" | tr -d ' ')
    if [ "$actual" = "3" ]; then
        pass
    else
        fail "count: -c 3 produced $actual packets"
    fi
else
    fail "count: -c 3 exited non-zero"
fi
out="$tmp/reject.out"
if "$bin" -nn -tt -r "$fixtures/ipv4-tcp-handshake.pcap" 'tcp port 9999' >"$out" 2>"$tmp/reject.err"; then
    actual=$(wc -l <"$out" | tr -d ' ')
    if [ "$actual" = "0" ]; then
        pass
    else
        fail "reject: 'tcp port 9999' produced $actual packets"
    fi
else
    fail "reject: 'tcp port 9999' exited non-zero"
fi

if [ "$failures" -ne 0 ]; then
    printf 'tcpdump-conformance: FAILED (%s checks, %s failures)\n' "$checks" "$failures" >&2
    exit 1
fi
printf 'tcpdump-conformance: PASS (%s checks)\n' "$checks"
