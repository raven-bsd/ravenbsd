#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host behaviour evidence for the bounded first-party `patch` utility.
# The forward/reverse cases generate their diffs with the first-party diff engine.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
target=$root/build/posix-utility/patch-host
if [ -z "${UTIL:-}" ] || [ -z "${DIFF:-}" ]; then
    printf '==> patch: host build (%s)\n' "$target"
    (
        cd "$root"
        CARGO_TARGET_DIR=$target cargo build --quiet --locked -p raven-patch --bins --example gen-diff
    ) || {
        printf 'patch test: host build failed\n' >&2
        exit 1
    }
    UTIL=$target/debug/raven-patch
    DIFF=$target/debug/examples/gen-diff
fi
if [ ! -x "$UTIL" ] || [ ! -x "$DIFF" ]; then
    printf 'patch test: missing host binaries\n' >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail() {
    printf 'FAIL: %s\n' "$1" >&2
    exit 1
}

check_file() {
    name=$1
    expected=$2
    actual=$3
    if ! cmp -s "$expected" "$actual"; then
        printf 'FAIL: %s\n' "$name" >&2
        printf '%s\n' '--- expected ---' >&2
        cat "$expected" >&2
        printf '%s\n' '--- actual ---' >&2
        cat "$actual" >&2
        exit 1
    fi
    printf 'ok: %s\n' "$name"
}

mkdir -p "$tmp/forward"
cd "$tmp/forward"
printf 'one\ntwo\nthree\n' >original
printf 'one\nTWO\nthree\nfour\n' >expected
"$DIFF" original expected >change.patch
cp original b
"$UTIL" -p0 -i change.patch
check_file 'first-party diff applies with -i' expected b

cp expected a
"$UTIL" -R -p0 -i change.patch
check_file 'reverse patch restores the original' original a

cp original b
"$UTIL" -p0 <change.patch
check_file 'patch reads standard input' expected b

cp original b
"$UTIL" -u -f -p0 -i change.patch
check_file '-u and -f are accepted' expected b

mkdir -p "$tmp/multi"
cd "$tmp/multi"
printf 'one\n' >one
printf 'ONE\n' >one.expected
printf 'two\n' >two
printf 'TWO\n' >two.expected
cat >multi.patch <<'EOF'
--- one
+++ one
@@ -1 +1 @@
-one
+ONE
--- two
+++ two
@@ -1 +1 @@
-two
+TWO
EOF
"$UTIL" -p0 -i multi.patch
check_file 'multiple file diffs in one patch' one.expected one
check_file 'multiple file diffs in one patch (second file)' two.expected two

printf 'one\n' >one
printf 'two\n' >two
cat >one.patch <<'EOF'
--- one
+++ one
@@ -1 +1 @@
-one
+ONE
EOF
cat >two.patch <<'EOF'
--- two
+++ two
@@ -1 +1 @@
-two
+TWO
EOF
"$UTIL" -p0 one.patch two.patch
check_file 'multiple patch operands' one.expected one
check_file 'multiple patch operands (second file)' two.expected two

mkdir -p "$tmp/strip/dir"
cd "$tmp/strip"
printf 'old\n' >dir/file
printf 'new\n' >expected
cat >strip.patch <<'EOF'
--- old/dir/file
+++ new/dir/file
@@ -1 +1 @@
-old
+new
EOF
"$UTIL" -p 1 -i strip.patch
check_file '-p strips one leading component' expected dir/file

mkdir -p "$tmp/create"
cd "$tmp/create"
: >empty
printf 'one\ntwo\n' >expected
"$DIFF" empty expected >create.patch
"$UTIL" -p0 -i create.patch
check_file 'first-party diff creates a missing file' expected b

cat >devnull-create.patch <<'EOF'
--- /dev/null
+++ created.txt
@@ -0,0 +1,2 @@
+one
+two
EOF
printf 'one\ntwo\n' >created.expected
"$UTIL" -p0 -i devnull-create.patch
check_file '/dev/null header creates a file' created.expected created.txt

mkdir -p "$tmp/delete"
cd "$tmp/delete"
printf 'one\ntwo\n' >doomed
cat >delete.patch <<'EOF'
--- doomed
+++ /dev/null
@@ -1,2 +0,0 @@
-one
-two
EOF
"$UTIL" -p0 -i delete.patch
if [ -e doomed ]; then
    fail '/dev/null header deletes a file'
fi
printf 'ok: /dev/null header deletes a file\n'

mkdir -p "$tmp/reject"
cd "$tmp/reject"
printf 'one\nWRONG\nthree\n' >b
printf 'one\nWRONG\nthree\n' >b.expected
cat >bad.patch <<'EOF'
--- a
+++ b
@@ -1,3 +1,3 @@
 one
-two
+TWO
 three
EOF
if "$UTIL" -p0 -i bad.patch >"$tmp/out" 2>"$tmp/err"; then
    fail 'mismatched hunk is rejected'
fi
if ! grep -q 'does not apply' "$tmp/err"; then
    fail 'mismatched hunk diagnostic'
fi
check_file 'mismatched hunk leaves the file unchanged' b.expected b

printf 'one\ntwo\nthree\nfour\nfive\n' >b
printf 'one\ntwo\nthree\nfour\nfive\n' >b.expected
cat >atomic.patch <<'EOF'
--- a
+++ b
@@ -1 +1 @@
-one
+ONE
@@ -3 +3 @@
-three
+THREE
EOF
printf 'one\ntwo\nWRONG\nfour\nfive\n' >b
printf 'one\ntwo\nWRONG\nfour\nfive\n' >b.expected
if "$UTIL" -p0 -i atomic.patch >"$tmp/out" 2>"$tmp/err"; then
    fail 'second bad hunk is rejected'
fi
check_file 'a failed later hunk leaves no partial change' b.expected b

printf 'old\n' >missing.expected
cat >missing.patch <<'EOF'
--- a
+++ missing
@@ -1 +1 @@
-old
+new
EOF
if "$UTIL" -p0 -i missing.patch >"$tmp/out" 2>"$tmp/err"; then
    fail 'missing target is rejected'
fi
printf 'ok: missing target is rejected\n'

set +e
"$UTIL" -Q >"$tmp/out" 2>"$tmp/err"
status=$?
set -e
if [ "$status" -ne 2 ]; then
    fail 'unknown option exits with status 2'
fi
printf 'ok: unknown option exits with status 2\n'
