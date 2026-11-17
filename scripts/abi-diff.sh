#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# abi-diff.sh — generate, diff and self-check the native ABI table.
#
#   scripts/abi-diff.sh --self-test   regenerate the table from the current source and compare it
#                                     to the committed snapshot tests/abi/abi-v1.txt (exit 0 on
#                                     match, 1 on a stale table).
#   scripts/abi-diff.sh regenerate    rewrite the committed snapshot from the current source.
#   scripts/abi-diff.sh BASE..HEAD    print ABI additions/removals/layout changes between two git
#                                     revisions, and exit non-zero on a removal or layout change
#                                     unless a commit in the range carries `abi-accepted: <reason>`.
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

case "${1:-}" in
    --self-test)
        actual=$(mktemp)
        trap 'rm -f "$actual"' EXIT
        cargo run -q -p abi-diff -- generate > "$actual"
        if diff -u tests/abi/abi-v1.txt "$actual" >&2; then
            echo "abi-diff: ok (table current)"
        else
            echo "abi-diff: FAIL (table stale; run scripts/abi-diff.sh regenerate)" >&2
            exit 1
        fi
        ;;
    regenerate)
        cargo run -q -p abi-diff -- generate > tests/abi/abi-v1.txt
        echo "abi-diff: regenerated tests/abi/abi-v1.txt"
        ;;
    "")
        echo "usage: abi-diff.sh --self-test | regenerate | BASE..HEAD" >&2
        exit 2
        ;;
    *)
        range="$1"
        case "$range" in
            *..*) ;;
            *) echo "abi-diff: expected BASE..HEAD, got $range" >&2; exit 2 ;;
        esac
        base=${range%%..*}
        tmp=$(mktemp -d)
        trap 'rm -rf "$tmp"' EXIT
        for f in crates/raven-abi/src/*.rs; do
            name=${f##*/}
            git show "$base:crates/raven-abi/src/$name" > "$tmp/$name" 2>/dev/null || true
        done
        cargo run -q -p abi-diff -- generate --source "$tmp" > "$tmp/base.txt"
        cargo run -q -p abi-diff -- generate > "$tmp/head.txt"
        if cargo run -q -p abi-diff -- diff "$tmp/base.txt" "$tmp/head.txt"; then
            exit 0
        fi
        # A removal or layout change: allowed only with an explicit `abi-accepted` note in the range.
        if git log --format=%B "$range" 2>/dev/null | grep -q "abi-accepted"; then
            echo "abi-diff: breaking change accepted (abi-accepted present in $range)" >&2
        else
            echo "abi-diff: FAIL (breaking ABI change without abi-accepted in $range)" >&2
            exit 1
        fi
        ;;
esac
