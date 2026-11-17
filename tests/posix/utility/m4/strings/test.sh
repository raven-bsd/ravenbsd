#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# substr/index/translit behaviour cases.

set -eu
: "${UTIL:?UTIL must point to the host raven-m4 binary}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

check() {
    name=$1
    expected=$2
    script=$3
    actual=$("$UTIL" "$script")
    if [ "$actual" != "$expected" ]; then
        printf 'FAIL: %s\nexpected: [%s]\nactual:   [%s]\n' "$name" "$expected" "$actual" >&2
        exit 1
    fi
    printf 'ok: %s\n' "$name"
}

cat >"$tmp/substr.m4" <<'EOF'
substr(`abcdef',2,3) substr(`abcdef',2) substr(`abcdef',0,2)
EOF
check 'substr zero-offset start and length' 'cde cdef ab' "$tmp/substr.m4"

cat >"$tmp/index.m4" <<'EOF'
index(`abcabc',`ca') index(`abc',`zz') index(`abc',`')
EOF
check 'index zero-offset and not-found' '2 -1 0' "$tmp/index.m4"

cat >"$tmp/translit.m4" <<'EOF'
translit(`abcabc',`abc',`xy')
EOF
check 'translit replacement' 'xyxy' "$tmp/translit.m4"

cat >"$tmp/delete.m4" <<'EOF'
x:translit(`abc',`abc',):y
EOF
check 'translit deletes when the replacement is shorter' 'x::y' "$tmp/delete.m4"

cat >"$tmp/partial.m4" <<'EOF'
translit(`abcd',`abcd',`xy')
EOF
check 'translit deletes unmatched trailing characters' 'xy' "$tmp/partial.m4"

cat >"$tmp/unicode.m4" <<'EOF'
substr(`héllo',2,3)
EOF
check 'substr operates on characters' 'llo' "$tmp/unicode.m4"
