#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# C47m aarch64 binary128 `long double` evidence.
#
# The host C compiler cannot target aarch64, so this uses the cross compiler when it exists and
# checks three things that do not need a board or a user-mode emulator:
#
#   1. the aarch64 headers declare every exported `*l` name and `long double` is 16 bytes;
#   2. the AAPCS64 passes and returns a `long double` in a single 128-bit SIMD register (`q0`),
#      which is the contract the naked stubs in `math/m70.rs` implement;
#   3. when the aarch64 archive has been packed, every exported name is a defined function there.
#
# A missing cross compiler is a hard failure (no silent pass); a missing archive is reported as a
# skip for check 3 only.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

cc=${AARCH64_CC:-aarch64-linux-gnu-gcc}
if ! command -v "$cc" >/dev/null 2>&1; then
    echo "check-ld-abi: $cc not found; cannot check the aarch64 ABI" >&2
    exit 2
fi

include=$root/libc/ravenlibc/include
arch_include=$root/libc/ravenlibc/arch/aarch64/include
tmp=$(mktemp -d "${TMPDIR:-/tmp}/c47m-ld.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

names='fabsl floorl ceill truncl roundl rintl nearbyintl logbl significandl copysignl fdiml
fmaxl fminl fmodl remainderl nextafterl nexttowardl scalbnl ldexpl scalblnl frexpl modfl
remquol ilogbl llogbl lrintl llrintl lroundl llroundl nanl sqrtl cbrtl hypotl
expl logl exp2l log2l log10l sinl cosl tanl sincosl sinhl coshl tanhl asinl acosl atanl
atan2l asinhl acoshl atanhl erfl erfcl gammal lgammal lgammal_r tgammal fmal'

flags="-std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800 -Werror -Wno-unused-value -I $include -I $arch_include"

# 1. Declarations and the width of long double.
{
    echo '#include <math.h>'
    echo '_Static_assert(sizeof(long double) == 16, "aarch64 long double is 16 bytes");'
    echo 'static void *const probes[] = {'
    for name in $names; do
        echo "    (void *)&$name,"
    done
    echo '};'
    echo 'int main(void) { return probes[0] == 0; }'
} > "$tmp/decls.c"
if ! $cc -fsyntax-only $flags "$tmp/decls.c"; then
    echo "check-ld-abi: FAIL declarations or long-double width" >&2
    exit 1
fi
echo "check-ld-abi: ok declarations ($(printf '%s\n' $names | wc -l) names, sizeof(long double)=16)"

# 2. AAPCS64 register contract: a long double arrives and leaves in q0.
cat > "$tmp/abi.c" <<'EOF'
/* A volatile round trip forces the compiler to spill and reload the argument/result, which shows
 * the register the AAPCS64 uses for a `long double` value. */
long double c47m_keep(long double x) {
    volatile long double sink = x;
    return sink;
}
EOF
if ! $cc -O2 -S -o "$tmp/abi.s" $flags "$tmp/abi.c"; then
    echo "check-ld-abi: FAIL ABI probe compile" >&2
    exit 1
fi
if ! grep -q 'str[[:space:]]*q0' "$tmp/abi.s" || ! grep -q 'ldr[[:space:]]*q0' "$tmp/abi.s"; then
    echo "check-ld-abi: FAIL long double is not passed/returned in q0" >&2
    sed -n '1,40p' "$tmp/abi.s" >&2
    exit 1
fi
echo "check-ld-abi: ok AAPCS64 long double argument and result are in q0"

# 2b. The ternary contract used by `fmal`: q0/q1/q2 in, q0 out.
cat > "$tmp/ternary.c" <<'EOF'
long double c74al_ternary(long double x, long double y, long double z) {
    volatile long double a = x;
    volatile long double b = y;
    volatile long double c = z;
    return a + b * c;
}
EOF
if ! $cc -O2 -S -o "$tmp/ternary.s" $flags "$tmp/ternary.c"; then
    echo "check-ld-abi: FAIL ternary probe compile" >&2
    exit 1
fi
if ! grep -q 'str[[:space:]]*q0' "$tmp/ternary.s" ||
    ! grep -q 'str[[:space:]]*q1' "$tmp/ternary.s" ||
    ! grep -q 'str[[:space:]]*q2' "$tmp/ternary.s" ||
    ! grep -q 'ldr[[:space:]]*q0' "$tmp/ternary.s"; then
    echo "check-ld-abi: FAIL ternary long double arguments are not q0/q1/q2" >&2
    sed -n '1,60p' "$tmp/ternary.s" >&2
    exit 1
fi
echo "check-ld-abi: ok AAPCS64 ternary long double arguments are q0/q1/q2"

# 3. Archive symbols, when the aarch64 sysroot has been packed.
archive=$root/build/aarch64/libc.a
if [ ! -f "$archive" ]; then
    echo "check-ld-abi: skip archive symbols (build/aarch64/libc.a not packed)"
    exit 0
fi
missing=''
for name in $names; do
    if ! aarch64-linux-gnu-nm --defined-only "$archive" 2>/dev/null |
        awk '{print $3}' | grep -qx "$name"; then
        missing="$missing $name"
    fi
done
if [ -n "$missing" ]; then
    echo "check-ld-abi: FAIL archive is missing:$missing" >&2
    exit 1
fi
echo "check-ld-abi: ok all $(printf '%s\n' $names | wc -l) long-double symbols are defined in build/aarch64/libc.a"
