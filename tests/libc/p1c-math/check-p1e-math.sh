#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1e math/complex/fenv declaration evidence.
#
# The POSIX gate probes every interface row by taking the address of its name
# under the strict POSIX/XSI feature selects. This script does the same for the
# three P1e headers, covering the classification predicates (macros that need
# an addressable declaration), the legacy names that were hidden behind
# __BSD_VISIBLE, the full C99 complex set and the fenv interfaces. A missing
# declaration or a compile error is a failure.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

cc=${CC:-cc}
command -v "$cc" >/dev/null 2>&1 || { echo "p1e-math: no C compiler" >&2; exit 1; }

inc="-I libc/ravenlibc/include -I libc/ravenlibc/arch/x86_64/include"
flags="-fsyntax-only -std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800 -Wno-unused-value"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

fail=0
probe() {
    hdr=$1
    name=$2
    printf '#include <%s>\nstatic void *const probe = (void *)&%s;\nint main(void) { return 0; }\n' \
        "$hdr" "$name" > "$tmp/probe.c"
    if ! $cc $flags $inc "$tmp/probe.c" 2>"$tmp/probe.err"; then
        echo "p1e-math: $hdr $name does not compile" >&2
        sed -n '1,3p' "$tmp/probe.err" >&2
        fail=1
    fi
}

# math.h: classification predicates (addressable declarations) and the legacy
# names that were previously hidden behind __BSD_VISIBLE.
for name in \
    fpclassify isinf isnan isnormal isfinite signbit isunordered isless \
    islessequal islessgreater isgreater isgreaterequal iscanonical iseqsig \
    issignaling issubnormal iszero \
    drem dremf finite finitef gamma gammaf gammal scalb scalbf \
    significand significandf sincos sincosf sincosl exp10 exp10f exp10l \
    pow10 pow10f pow10l lgammal_r lgamma_r lgammaf_r j0f j1f jnf y0f y1f
do
    probe math.h "$name"
done

# complex.h: every C99 interface for double, float and long double.
for name in \
    cabs cabsf cabsl cacos cacosf cacosl cacosh cacoshf cacoshl carg cargf cargl \
    casin casinf casinl casinh casinhf casinhl catan catanf catanl catanh catanhf catanhl \
    ccos ccosf ccosl ccosh ccoshf ccoshl cexp cexpf cexpl cimag cimagf cimagl \
    clog clogf clogl conj conjf conjl cpow cpowf cpowl cproj cprojf cprojl creal crealf creall \
    csin csinf csinl csinh csinhf csinhl csqrt csqrtf csqrtl ctan ctanf ctanl ctanh ctanhf ctanhl
do
    probe complex.h "$name"
done

# fenv.h: C99 and C23 interfaces the header declares.
for name in \
    feclearexcept fegetexceptflag feraiseexcept fesetexcept fesetexceptflag \
    fetestexcept fetestexceptflag fegetround fesetround fegetenv feholdexcept \
    fesetenv feupdateenv
do
    probe fenv.h "$name"
done

if [ "$fail" -ne 0 ]; then
    echo "p1e-math: FAIL" >&2
    exit 1
fi
echo "p1e-math: declarations PASS"
