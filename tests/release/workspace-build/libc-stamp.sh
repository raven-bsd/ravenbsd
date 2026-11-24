#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K441 stale-artifact regression. Proves the libc stamp catches a source edit without a rebuild
# (the false-red class) and a set mismatch (the tiny-set archive the Solaris test tripped over),
# then proves the documented rebuild makes --verify green. The edited source is restored on every
# exit path. A DSO stamp is checked too when one exists (the false-green class).

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
source=libc/ravenlibc/crates/raven-libc/src/lib.rs
[ -f "$source" ] || {
    echo "libc-stamp: missing $source" >&2
    exit 2
}
backup=$(mktemp "${TMPDIR:-/tmp}/libc-stamp.XXXXXX")
cp "$source" "$backup"
err=$(mktemp "${TMPDIR:-/tmp}/libc-stamp-err.XXXXXX")
restore() {
    cp "$backup" "$source"
    rm -f "$backup" "$err"
}
trap restore EXIT HUP INT TERM

echo "libc-stamp: build the fresh artifact"
timeout 300 cargo xtask libc-pack --arch "$arch" > /dev/null
timeout 300 cargo xtask libc-pack --arch "$arch" --verify

echo "libc-stamp: source edit without a rebuild must be stale"
printf '\n// K441 stale-artifact regression\n' >> "$source"
if timeout 300 cargo xtask libc-pack --arch "$arch" --verify 2> "$err"; then
    echo "libc-stamp: expected the stale error after the source edit" >&2
    exit 1
fi
grep -q 'artifact stale: run cargo xtask libc-pack' "$err" || {
    echo "libc-stamp: wrong stale message:" >&2
    cat "$err" >&2
    exit 1
}
echo "libc-stamp: source edit -> artifact stale"

for fixture in socketpair-fixture itimer-fixture; do
    if timeout 300 sh "tests/qtest/cases/$fixture/build.sh" > /dev/null 2> "$err"; then
        echo "libc-stamp: expected $fixture to fail closed on the stale artifact" >&2
        exit 1
    fi
    grep -q 'artifact stale: run cargo xtask libc-pack' "$err" || {
        echo "libc-stamp: $fixture did not print the stale message:" >&2
        cat "$err" >&2
        exit 1
    }
    echo "libc-stamp: $fixture -> artifact stale"
done

if [ -f "build/$arch/libc-dso.stamp" ]; then
    if timeout 300 cargo xtask libc-link --arch "$arch" --verify 2> "$err"; then
        echo "libc-stamp: expected the DSO stale error after the source edit" >&2
        exit 1
    fi
    grep -q 'artifact stale: run cargo xtask libc-link' "$err" || {
        echo "libc-stamp: wrong DSO stale message:" >&2
        cat "$err" >&2
        exit 1
    }
    echo "libc-stamp: source edit -> DSO artifact stale"
else
    echo "libc-stamp: DSO stamp absent; run cargo xtask libc-link to cover it"
fi

restore
trap - EXIT HUP INT TERM
trap 'rm -f "$backup" "$err"' EXIT HUP INT TERM

echo "libc-stamp: tiny-set archive must not satisfy the server stamp"
timeout 300 cargo xtask libc-pack --arch "$arch" --set tiny > /dev/null
if timeout 300 cargo xtask libc-pack --arch "$arch" --verify 2> "$err"; then
    echo "libc-stamp: expected the set mismatch to be stale" >&2
    exit 1
fi
grep -q 'artifact stale: run cargo xtask libc-pack' "$err" || {
    echo "libc-stamp: wrong set-mismatch message:" >&2
    cat "$err" >&2
    exit 1
}
echo "libc-stamp: tiny set -> artifact stale"

echo "libc-stamp: documented rebuild returns the tree to green"
timeout 300 cargo xtask libc-pack --arch "$arch" > /dev/null
timeout 300 cargo xtask libc-pack --arch "$arch" --verify
timeout 300 cargo nextest run -p raven-libc --test archive_exports

echo "libc-stamp: ok"
