#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Helpers for the POSIX file utility behaviour cases.
#
# The Raven target binaries need the Raven kernel, so the cases run the same vendored uutils
# source through a host build. Each case sets up a fresh temporary directory and the C locale.

file_fail() {
    printf 'FAIL: %s: %s\n' "${TOOL:-file}" "$*" >&2
    exit 1
}

file_ok() {
    printf 'ok: %s\n' "$*"
}

file_eq() {
    expected=$1
    actual=$2
    label=$3
    if [ "$expected" != "$actual" ]; then
        printf 'FAIL: %s: %s\n' "$TOOL" "$label" >&2
        printf '  expected: [%s]\n  actual:   [%s]\n' "$expected" "$actual" >&2
        exit 1
    fi
    file_ok "$label"
}

file_status() {
    expected=$1
    label=$2
    shift 2
    set +e
    "$@" >"$TMPDIR/status.out" 2>"$TMPDIR/status.err"
    actual=$?
    set -e
    if [ "$expected" = "nonzero" ]; then
        if [ "$actual" -eq 0 ]; then
            printf 'FAIL: %s: %s (exit 0, expected nonzero)\n' "$TOOL" "$label" >&2
            exit 1
        fi
    elif [ "$actual" -ne "$expected" ]; then
        printf 'FAIL: %s: %s (exit %s, expected %s)\n' \
            "$TOOL" "$label" "$actual" "$expected" >&2
        sed 's/^/  stderr: /' "$TMPDIR/status.err" >&2
        exit 1
    fi
    file_ok "$label"
}

file_file_eq() {
    expected=$1
    actual=$2
    label=$3
    if ! cmp -s "$expected" "$actual"; then
        printf 'FAIL: %s: %s\n' "$TOOL" "$label" >&2
        printf '  expected: ' >&2
        od -An -tx1 "$expected" >&2
        printf '  actual:   ' >&2
        od -An -tx1 "$actual" >&2
        exit 1
    fi
    file_ok "$label"
}

file_contains() {
    value=$1
    needle=$2
    label=$3
    case "$value" in
        *"$needle"*) file_ok "$label" ;;
        *)
            printf 'FAIL: %s: %s (missing [%s] in [%s])\n' \
                "$TOOL" "$label" "$needle" "$value" >&2
            exit 1
            ;;
    esac
}

file_not_contains() {
    value=$1
    needle=$2
    label=$3
    case "$value" in
        *"$needle"*)
            printf 'FAIL: %s: %s (unexpected [%s] in [%s])\n' \
                "$TOOL" "$label" "$needle" "$value" >&2
            exit 1
            ;;
        *) file_ok "$label" ;;
    esac
}

file_setup() {
    TOOL=$1
    export TOOL
    LC_ALL=C
    export LC_ALL
    TMPDIR=$(mktemp -d "${TMPDIR:-/tmp}/posix-file-$TOOL.XXXXXX")
    export TMPDIR
    trap 'rm -rf "$TMPDIR"' 0 1 2 15
    umask 022
    if [ -z "${UTIL:-}" ]; then
        root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../../.." && pwd)
        target=$root/build/posix-utility/uutils-host
        UTIL=$target/debug/$TOOL
        export UTIL
        if [ ! -x "$UTIL" ]; then
            (cd "$root" &&
                CARGO_TARGET_DIR=$target cargo build --quiet --locked \
                    -p "uu_$TOOL" --manifest-path userland/uutils/Cargo.toml) ||
                file_fail "host build failed"
        fi
    fi
    [ -x "$UTIL" ] || file_fail "no host build at $UTIL"
}
