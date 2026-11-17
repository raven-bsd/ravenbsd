#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host behaviour evidence for the bounded first-party `bc` core.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
target=$root/build/posix-utility/bc-host
if [ -z "${UTIL:-}" ]; then
    printf '==> bc: host build (%s)\n' "$target"
    (
        cd "$root"
        CARGO_TARGET_DIR=$target cargo build --quiet --locked -p raven-bc
    ) || {
        printf 'bc test: host build failed\n' >&2
        exit 1
    }
    UTIL=$target/debug/raven-bc
fi
if [ ! -x "$UTIL" ]; then
    printf 'bc test: missing host binary\n' >&2
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

cd "$tmp"

printf '2^100\n' >input
printf '1267650600228229401496703205376\n' >expected
"$UTIL" input >out 2>err
check_file 'arbitrary-precision integer power' expected out

printf 'scale=50\n1/7\nsqrt(2)\n' >input
cat >expected <<'EOF'
.14285714285714285714285714285714285714285714285714
1.41421356237309504880168872420969807856967187537694
EOF
"$UTIL" input >out 2>err
check_file 'arbitrary-precision division and square root' expected out

printf 'scale=5\n1/3\nscale(1.230)\nlength(.001)\n' >input
printf '.33333\n3\n3\n' >expected
"$UTIL" input >out 2>err
check_file 'scale, division truncation and length' expected out

printf 'if (0) 1 else 2\n' >input
printf '2\n' >expected
"$UTIL" input >out 2>err
check_file 'if/else' expected out

printf 'i=0\ns=0\nwhile (i<5) { i=i+1; s=s+i }\ns\n' >input
printf '15\n' >expected
"$UTIL" input >out 2>err
check_file 'while loop' expected out

printf 's=0\nfor (i=0; i<5; i=i+1) s=s+i\ns\n' >input
printf '10\n' >expected
"$UTIL" input >out 2>err
check_file 'for loop' expected out

cat >input <<'EOF'
define f(x) { return (x*x) }
f(7)
define fact(n) { if (n <= 1) return (1); return (n*fact(n-1)) }
fact(6)
define g(x) { auto y; y=x+1; return (y) }
g(4)
EOF
printf '49\n720\n5\n' >expected
"$UTIL" input >out 2>err
check_file 'user functions, recursion and auto variables' expected out

cat >input <<'EOF'
a[0]=10
a[1]=20
a[0]+a[1]
a[2]
EOF
printf '30\n0\n' >expected
"$UTIL" input >out 2>err
check_file 'arrays and default zero elements' expected out

cat >input <<'EOF'
print "x=", 3, "\n"
"hi"
4
EOF
printf 'x=3\nhi4\n' >expected
"$UTIL" input >out 2>err
check_file 'print, strings and expression output' expected out

cat >input <<'EOF'
/* block comment */ 1+1
# line comment
2+2
EOF
printf '2\n4\n' >expected
"$UTIL" input >out 2>err
check_file 'comments' expected out

printf 'scale=20\ne(1)\nl(2)\ns(1)\nc(1)\na(1)\n' >input
cat >expected <<'EOF'
2.71828182845904523536
.69314718055994530941
.84147098480789650665
.54030230586813971740
.78539816339744830961
EOF
"$UTIL" -l input >out 2>err
check_file '-l math library' expected out

printf 'scale\n' >input
printf '20\n' >expected
"$UTIL" -l input >out 2>err
check_file '-l sets scale to 20' expected out

printf 'x=5\n' >one.bc
printf 'x*3\n' >two.bc
printf '15\n' >expected
"$UTIL" one.bc two.bc >out 2>err
check_file 'file operands share state' expected out

printf '9+1\n' >input
printf '10\n' >expected
"$UTIL" <input >out 2>err
check_file 'standard input' expected out

printf 'quit\n99\n' >input
"$UTIL" input >out 2>err
if [ -s out ]; then
    fail 'quit stops processing'
fi
printf 'ok: quit stops processing\n'

expect_failure() {
    name=$1
    expected_status=$2
    shift 2
    set +e
    "$@" >"$tmp/out" 2>"$tmp/err"
    status=$?
    set -e
    if [ "$status" -ne "$expected_status" ]; then
        printf 'FAIL: %s (status %s, expected %s)\n' "$name" "$status" "$expected_status" >&2
        cat "$tmp/err" >&2
        exit 1
    fi
    printf 'ok: %s\n' "$name"
}

printf '1/0\n' >input
expect_failure 'division by zero' 1 "$UTIL" input

printf 'sqrt(-1)\n' >input
expect_failure 'square root of a negative number' 1 "$UTIL" input

printf 'missing(1)\n' >input
expect_failure 'undefined function' 1 "$UTIL" input

printf 'obase=16\n' >input
expect_failure 'non-decimal obase is unsupported' 1 "$UTIL" input

printf 'e(1)\n' >input
expect_failure 'math function without -l' 1 "$UTIL" input

printf 'if (1)\n' >input
expect_failure 'parse error' 1 "$UTIL" input

expect_failure 'missing file operand' 1 "$UTIL" does-not-exist.bc

expect_failure 'unknown option' 2 "$UTIL" -Q
