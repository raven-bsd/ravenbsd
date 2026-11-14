#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Smoke mode: 60 seconds per target. Exits 77 when cargo-fuzz or nightly is absent.

set -u

ROOT=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
SECS=${SMOKE_SECS:-60}
TMP=${TMPDIR:-/tmp}/c70-fuzz-smoke
ART=$TMP/artifacts
mkdir -p "$TMP/corpus" "$ART"
# libFuzzer's ASan quarantine can exceed the 512 MiB RSS limit before the body is at fault.
ASAN_OPTIONS="detect_odr_violation=0:quarantine_size_mb=64${ASAN_OPTIONS:+:$ASAN_OPTIONS}"
export ASAN_OPTIONS

if ! command -v cargo-fuzz >/dev/null 2>&1; then
    echo "skipped: cargo-fuzz or nightly not installed"
    exit 77
fi
if ! rustup run nightly rustc --version >/dev/null 2>&1; then
    echo "skipped: cargo-fuzz or nightly not installed"
    exit 77
fi

if [ "$#" -gt 0 ]; then
    TARGETS=$*
else
    TARGETS=$(rustup run nightly cargo fuzz list --fuzz-dir "$ROOT/targets")
fi

status=0
ok=0
crash=0
for target in $TARGETS; do
    rm -rf "$TMP/corpus/$target"
    cp -R "$ROOT/corpus/$target" "$TMP/corpus/$target"
    mkdir -p "$ART/$target"
    if rustup run nightly cargo fuzz run --fuzz-dir "$ROOT/targets" "$target" "$TMP/corpus/$target" -- \
        -max_total_time="$SECS" -max_len=4096 -rss_limit_mb=512 -timeout=10 \
        -artifact_prefix="$ART/$target/" >"$TMP/$target.log" 2>&1; then
        echo "smoke $target: ok ($SECS s)"
        ok=$((ok + 1))
    else
        echo "smoke $target: CRASH"
        crash=$((crash + 1))
        status=1
    fi
done
echo "smoke: $ok ok, $crash crashes"
exit "$status"
