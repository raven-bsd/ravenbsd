#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Soak mode: `soak.sh <target> [hours]` (default 1 hour). Writes only under $TMPDIR.

set -u

if [ "$#" -lt 1 ]; then
    echo "usage: soak.sh <target> [hours]" >&2
    exit 2
fi
ROOT=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
TARGET=$1
HOURS=${2:-1}
SECS=$((HOURS * 3600))
TMP=${TMPDIR:-/tmp}/c70-fuzz-soak
ART=$TMP/artifacts
mkdir -p "$TMP/corpus" "$ART/$TARGET"
# Keep the ASan quarantine below the 512 MiB RSS smoke/soak limit.
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

rm -rf "$TMP/corpus/$TARGET"
cp -R "$ROOT/corpus/$TARGET" "$TMP/corpus/$TARGET"
mkdir -p "$TMP/$TARGET"
# `-jobs=1` writes `fuzz-0.log` in the current directory; one directory per target keeps
# parallel soak runs from sharing it.
cd "$TMP/$TARGET" || exit 1
exec rustup run nightly cargo fuzz run --fuzz-dir "$ROOT/targets" "$TARGET" "$TMP/corpus/$TARGET" -- \
    -max_total_time="$SECS" -max_len=4096 -rss_limit_mb=512 -timeout=10 -jobs=1 \
    -artifact_prefix="$ART/$TARGET/"
