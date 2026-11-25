#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85f host-side unit test for tests/fixtures/exec-canary/build.sh. The libc-test profile stages
# /bin/exec-canary into every image it builds, so an aarch64 libc-test image can only build when
# the fixture can be built for aarch64. The test copies the real build script into a scratch repo
# root, stubs cargo, and asserts the target triple, CRT/lib directory, linker script and code
# model selected for the default (x86_64) and RAVEN_QT_ARCH=aarch64 invocations.
#
# Run: timeout 30 sh tests/qtest/cases/smp-aarch64-exec-canary_ut.sh

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
BUILD="$ROOT/tests/fixtures/exec-canary/build.sh"

fail() {
    echo "smp-aarch64-exec-canary: FAIL $*" >&2
    exit 1
}

[ -f "$BUILD" ] || fail "missing fixture build script $BUILD"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
mkdir -p "$tmp/bin"

mkroot() {
    mkdir -p "$1/tests/fixtures/exec-canary" "$1/bin"
    cp "$BUILD" "$1/tests/fixtures/exec-canary/build.sh"
}

# Stub cargo: record the target and RUSTFLAGS, then create the artifact the script copies.
cat > "$tmp/bin/cargo" <<'STUB'
#!/bin/sh
target=""
target_dir=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "--target" ]; then
        target=$arg
    fi
    if [ "$prev" = "--target-dir" ]; then
        target_dir=$arg
    fi
    prev=$arg
done
[ -n "$target" ] || exit 1
[ -n "$target_dir" ] || exit 1
printf 'target=%s\nRUSTFLAGS=%s\n' "$target" "$RUSTFLAGS" >> "$SMP_EXEC_CAPTURE"
mkdir -p "$target_dir/$target/release"
: > "$target_dir/$target/release/exec-canary"
exit 0
STUB
chmod +x "$tmp/bin/cargo"

capture() {
    label=$1
    root=$tmp/$label
    mkroot "$root"
    if [ "$label" = "aarch64" ]; then
        env RAVEN_QT_ARCH=aarch64 SMP_EXEC_CAPTURE="$tmp/$label.txt" \
            PATH="$tmp/bin:$PATH" sh "$root/tests/fixtures/exec-canary/build.sh" \
            > "$tmp/$label.log" 2>&1 ||
            fail "$label build.sh failed: $(cat "$tmp/$label.log")"
    else
        env -u RAVEN_QT_ARCH SMP_EXEC_CAPTURE="$tmp/$label.txt" \
            PATH="$tmp/bin:$PATH" sh "$root/tests/fixtures/exec-canary/build.sh" \
            > "$tmp/$label.log" 2>&1 ||
            fail "$label build.sh failed: $(cat "$tmp/$label.log")"
    fi
    [ -s "$tmp/$label.txt" ] || fail "$label did not invoke cargo"
    [ -f "$root/build/fixtures/exec-canary/exec-canary" ] ||
        fail "$label did not stage build/fixtures/exec-canary/exec-canary"
}

capture default
grep -qx 'target=x86_64-unknown-ravenbsd' "$tmp/default.txt" ||
    fail "default target changed: $(head -1 "$tmp/default.txt")"
default_flags=$(sed -n 's/^RUSTFLAGS=//p' "$tmp/default.txt")
case "$default_flags" in
    *"-C code-model=small"*) ;;
    *) fail "x86_64 build lost code-model=small: $default_flags" ;;
esac
case "$default_flags" in
    *"-T$tmp/default/xtask/brush-x86_64.ld"*) ;;
    *) fail "x86_64 build lost brush-x86_64.ld: $default_flags" ;;
esac
case "$default_flags" in
    *"-L$tmp/default/build/x86_64"*) ;;
    *) fail "x86_64 build lost build/x86_64 lib dir: $default_flags" ;;
esac

capture aarch64
grep -qx 'target=aarch64-unknown-ravenbsd' "$tmp/aarch64.txt" ||
    fail "aarch64 target not selected: $(head -1 "$tmp/aarch64.txt")"
aarch64_flags=$(sed -n 's/^RUSTFLAGS=//p' "$tmp/aarch64.txt")
case "$aarch64_flags" in
    *"-T$tmp/aarch64/xtask/brush-aarch64.ld"*) ;;
    *) fail "aarch64 build did not use brush-aarch64.ld: $aarch64_flags" ;;
esac
case "$aarch64_flags" in
    *"-L$tmp/aarch64/build/aarch64"*) ;;
    *) fail "aarch64 build did not use build/aarch64 lib dir: $aarch64_flags" ;;
esac
case "$aarch64_flags" in
    *"-C code-model=small"*) fail "aarch64 build kept the x86_64 code model: $aarch64_flags" ;;
esac

echo "smp-aarch64-exec-canary: ok default=x86_64-unknown-ravenbsd aarch64=aarch64-unknown-ravenbsd"
