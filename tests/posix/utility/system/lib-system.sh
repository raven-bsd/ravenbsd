#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Helpers for the P4d host behaviour cases. The Raven target binaries are not
# staged because the vendored uutils target build still fails in rustix; the
# cases run the same vendored uutils source through a host build. The target
# primitives are provided by the merged P3 ttyd/ptyd work.

system_fail() {
    printf 'FAIL: %s: %s\n' "${TOOL:-system}" "$*" >&2
    exit 1
}

system_ok() {
    printf 'ok: %s\n' "$*"
}

system_eq() {
    expected=$1
    actual=$2
    label=$3
    if [ "$expected" != "$actual" ]; then
        printf 'FAIL: %s: %s\n' "${TOOL:-system}" "$label" >&2
        printf '  expected: [%s]\n  actual:   [%s]\n' "$expected" "$actual" >&2
        exit 1
    fi
    system_ok "$label"
}

system_status() {
    expected=$1
    label=$2
    shift 2
    set +e
    "$@" >"$TMPDIR/status.out" 2>"$TMPDIR/status.err"
    actual=$?
    set -e
    if [ "$expected" = "nonzero" ]; then
        if [ "$actual" -eq 0 ]; then
            printf 'FAIL: %s: %s (exit 0, expected nonzero)\n' "${TOOL:-system}" "$label" >&2
            exit 1
        fi
    elif [ "$actual" -ne "$expected" ]; then
        printf 'FAIL: %s: %s (exit %s, expected %s)\n' \
            "${TOOL:-system}" "$label" "$actual" "$expected" >&2
        sed 's/^/  stderr: /' "$TMPDIR/status.err" >&2
        exit 1
    fi
    system_ok "$label"
}

system_pty() {
    script -qec "$1" /dev/null | tr -d '\r'
}

system_setup() {
    TOOL=$1
    export TOOL
    LC_ALL=C
    export LC_ALL
    TMPDIR=$(mktemp -d "${TMPDIR:-/tmp}/posix-system-$TOOL.XXXXXX")
    export TMPDIR
    trap 'rm -rf "$TMPDIR"' 0 1 2 15
    if [ -z "${UTIL:-}" ]; then
        root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../../.." && pwd)
        target=$root/build/posix-utility/uutils-host
        UTIL=$target/debug/$TOOL
        export UTIL
        if [ ! -x "$UTIL" ]; then
            (cd "$root" &&
                CARGO_TARGET_DIR=$target cargo build --quiet --locked \
                    -p "uu_$TOOL" --manifest-path userland/uutils/Cargo.toml) ||
                system_fail "host build failed"
        fi
    fi
    [ -x "$UTIL" ] || system_fail "no host build at $UTIL"
    command -v script >/dev/null 2>&1 || system_fail "script(1) is required for the pty case"
}
