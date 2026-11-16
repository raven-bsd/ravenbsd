#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Helpers for the P4f misc vendored-uutils behaviour cases.
#
# The Raven target binaries need the Raven kernel, so the cases run the same
# vendored uutils source through a host build. Each case script sets up a
# fresh temporary directory and a deterministic C locale. The runner may set
# UTIL to a prebuilt host binary; otherwise misc_setup builds just that tool.

misc_fail() {
    printf 'FAIL: %s: %s\n' "${TOOL:-misc}" "$*" >&2
    exit 1
}

misc_ok() {
    printf 'ok: %s\n' "$*"
}

misc_eq() {
    expected=$1
    actual=$2
    label=$3
    if [ "$expected" != "$actual" ]; then
        printf 'FAIL: %s: %s\n' "$TOOL" "$label" >&2
        printf '  expected: [%s]\n  actual:   [%s]\n' "$expected" "$actual" >&2
        exit 1
    fi
    misc_ok "$label"
}

misc_status() {
    expected=$1
    label=$2
    shift 2
    set +e
    "$@" >"$TMPDIR/status.out" 2>"$TMPDIR/status.err"
    actual=$?
    set -e
    if [ "$expected" = "nonzero" ]; then
        if [ "$actual" -eq 0 ]; then
            printf 'FAIL: %s: %s (exit 0, expected nonzero)\n' \
                "$TOOL" "$label" >&2
            exit 1
        fi
    elif [ "$actual" -ne "$expected" ]; then
        printf 'FAIL: %s: %s (exit %s, expected %s)\n' \
            "$TOOL" "$label" "$actual" "$expected" >&2
        sed 's/^/  stderr: /' "$TMPDIR/status.err" >&2
        exit 1
    fi
    misc_ok "$label"
}

misc_match() {
    pattern=$1
    value=$2
    label=$3
    if ! printf '%s\n' "$value" | grep -Eq "$pattern"; then
        printf 'FAIL: %s: %s (no match for %s)\n' "$TOOL" "$label" "$pattern" >&2
        printf '  actual: [%s]\n' "$value" >&2
        exit 1
    fi
    misc_ok "$label"
}

misc_nonempty() {
    value=$1
    label=$2
    [ -n "$value" ] || misc_fail "$label (empty output)"
    misc_ok "$label"
}

misc_file_eq() {
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
    misc_ok "$label"
}

misc_setup() {
    TOOL=$1
    export TOOL
    LC_ALL=C
    export LC_ALL
    TMPDIR=$(mktemp -d "${TMPDIR:-/tmp}/posix-misc-$TOOL.XXXXXX")
    export TMPDIR
    trap 'rm -rf "$TMPDIR"' 0 1 2 15
    if [ -z "${UTIL:-}" ]; then
        root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../../.." && pwd)
        target=$root/build/posix-utility/misc-host
        UTIL=$target/debug/$TOOL
        export UTIL
        if [ ! -x "$UTIL" ]; then
            (cd "$root" &&
                CARGO_TARGET_DIR=$target cargo build --quiet --locked \
                    -p "uu_$TOOL" --manifest-path userland/uutils/Cargo.toml) ||
                misc_fail "host build failed"
        fi
    fi
    [ -x "$UTIL" ] || misc_fail "no host build at $UTIL"
}
