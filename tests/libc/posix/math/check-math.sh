#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1b math behavior evidence:
#   * the raven-libc-m unit suite (the numeric kernels and the C23 additions);
#   * the C long-double ABI smoke test linked against the packaged libc.a;
#   * a defined-symbol check for every claimed math row.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo nextest run -p raven-libc-m

archive=build/x86_64/libc.a
[ -f "$archive" ] || { echo "posix math: $archive is missing" >&2; exit 1; }

cc=${CC:-cc}
command -v "$cc" >/dev/null 2>&1 || { echo "posix math: no C compiler" >&2; exit 1; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

"$cc" -O2 -static -nostdlib -nostartfiles -Wl,-e,_start \
    tests/libc/posix/math/ld_abi.c "$archive" "$("$cc" -print-libgcc-file-name)" \
    -o "$tmp/ld_abi"
"$tmp/ld_abi"
echo "posix math: long-double ABI PASS"

for symbol in \
    acoshl acosl acospi asinhl asinl \
    asinpi atan2l atan2pi atanhl atanl \
    atanpi cbrtl ceill compoundn copysignl \
    coshl cosl cospi erfcl erfl \
    exp10l exp10m1 exp2l exp2m1 expl \
    expm1l fabsl fdiml floorl fmal \
    fmaximum_mag_num fmaxl fmaxmag fminimum_mag_num fminl \
    fminmag fmodl fpclassify frexpl fromfp \
    fromfpx gammal hypotl ilogbl iscanonical \
    iseqsig isfinite isgreater isgreaterequal isless \
    islessequal islessgreater isnan isnormal issubnormal \
    isunordered iszero ldexpl lgammal lgammal_r \
    llogb llrint llrintf llrintl llround \
    llroundf llroundl log10l log10p1 log1pl \
    log2l log2p1 logb logbf logbl \
    logl lrint lrintf lrintl lround \
    lroundf lroundl modfl nan nanf \
    nanl nearbyint nearbyintf nearbyintl nextafter \
    nextafterf nextafterl nexttoward nexttowardf nexttowardl \
    pow10l powl pown powr remainder \
    remainderf remainderl remquo remquof remquol \
    rintl rootn roundl scalbln scalblnf \
    scalblnl scalbnl signbit sincosl sinhl \
    sinl sqrtl statvfs strfromd strfromf strtof \
    tanhl tanl tanpi tgammal truncl \
    ufromfp ufromfpx
do
    if ! nm -g --defined-only "$archive" 2>/dev/null | grep -Eq " $symbol$"; then
        echo "posix math: missing libc.a symbol $symbol" >&2
        exit 1
    fi
done

echo "posix math: PASS"
