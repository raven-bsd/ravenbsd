#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# C47r aarch64 binary128 erf/gamma edge evidence.
#
# Builds the real `binary128.rs` as a host test binary and runs the edge matrix for the error
# function and the gamma family (zeros, infinities, NaN, poles, half-integers, reflection signs,
# the exact small integers and the tails), then checks the aarch64 tag for the six exports.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

rustc=${RUSTC:-rustc}
if ! command -v "$rustc" >/dev/null 2>&1; then
    echo "check-ld-special: $rustc not found; cannot run the kernel matrix" >&2
    exit 2
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/c47r-special.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

cp libc/ravenlibc/crates/raven-libc-m/src/binary128.rs "$tmp/binary128.rs"
# `binary128.rs` ends with `#[cfg(test)] mod tests;`; the empty module keeps this harness to the
# edge matrix below (the crate's own tests run under nextest).
: > "$tmp/binary128_ut.rs"

cat > "$tmp/harness.rs" <<'EOF'
mod consts {
    pub const FP_INFINITE: i32 = 0x01;
    pub const FP_NAN: i32 = 0x02;
    pub const FP_NORMAL: i32 = 0x04;
    pub const FP_SUBNORMAL: i32 = 0x08;
    pub const FP_ZERO: i32 = 0x10;
    pub const FP_ILOGB0: i32 = i32::MIN;
    pub const FP_ILOGBNAN: i32 = i32::MAX;
}

#[path = "binary128.rs"]
mod binary128;

use binary128::LongDouble128 as LD;

fn ld(value: f64) -> LD {
    LD::from_f64(value)
}

fn ulp(a: LD, b: LD) -> u128 {
    a.to_bits().abs_diff(b.to_bits())
}

fn f64_ulp(a: f64, b: f64) -> u64 {
    if a == b {
        return 0;
    }
    if a.is_nan() || b.is_nan() || a.is_sign_negative() != b.is_sign_negative() {
        return u64::MAX;
    }
    a.abs().to_bits().abs_diff(b.abs().to_bits())
}

#[test]
fn erf_zero_infinity_and_symmetry() {
    assert_eq!(ld(0.0).erf().to_bits(), 0);
    assert_eq!(ld(-0.0).erf().to_bits(), LD::from_bits(1u128 << 127).to_bits());
    assert_eq!(ld(0.0).erfc().to_bits(), LD::one().to_bits());
    assert_eq!(ld(f64::INFINITY).erf().to_f64(), 1.0);
    assert_eq!(ld(f64::NEG_INFINITY).erf().to_f64(), -1.0);
    assert_eq!(ld(f64::INFINITY).erfc().to_bits(), 0);
    assert_eq!(ld(f64::NEG_INFINITY).erfc().to_f64(), 2.0);
    assert!(ld(f64::NAN).erf().is_nan());
    assert!(ld(f64::NAN).erfc().is_nan());
    for value in [0.1f64, 0.5, 1.0, 2.5, 8.0, 30.0] {
        let x = ld(value);
        assert_eq!(
            ld(-value).erf().to_bits(),
            LD::zero(false).sub(x.erf()).to_bits()
        );
        assert!(ulp(x.erf().add(x.erfc()), LD::one()) <= 8, "erf+erfc {value}");
        assert!(ulp(ld(-value).erfc(), ld(2.0).sub(x.erfc())) <= 8);
    }
}

#[test]
fn erf_known_values_and_tails() {
    assert!(f64_ulp(ld(1.0).erf().to_f64(), 0.8427007929497149) <= 4);
    assert!(f64_ulp(ld(1.0).erfc().to_f64(), 0.15729920705028513) <= 4);
    let far = ld(40.0);
    assert!(far.erfc().is_finite() && !far.erfc().is_zero());
    assert!(far.erfc().to_f64() == 0.0, "erfc(40) is below the f64 range");
    assert_eq!(ld(120.0).erfc().to_bits(), 0);
    assert_eq!(ld(1.0e10).erfc().to_bits(), 0);
    assert_eq!(ld(1.0e10).erf().to_f64(), 1.0);
}

#[test]
fn lgamma_poles_zeros_and_signs() {
    assert_eq!(ld(1.0).lgamma().to_bits(), 0);
    assert_eq!(ld(2.0).lgamma().to_bits(), 0);
    for pole in [0.0f64, -0.0, -1.0, -2.0, -7.0] {
        let value = ld(pole).lgamma();
        assert!(value.is_infinite() && !value.sign_bit(), "lgamma({pole})");
    }
    assert!(ld(f64::INFINITY).lgamma().is_infinite());
    assert!(ld(f64::NEG_INFINITY).lgamma().is_infinite());
    assert!(ld(f64::NAN).lgamma().is_nan());
    // Reflection signs: Gamma(-1/2) < 0, Gamma(-3/2) > 0, Gamma(-5/2) < 0.
    assert_eq!(ld(-0.5).lgamma_r().1, -1);
    assert_eq!(ld(-1.5).lgamma_r().1, 1);
    assert_eq!(ld(-2.5).lgamma_r().1, -1);
    assert_eq!(ld(7.5).lgamma_r().1, 1);
    // lgamma(x+1) = lgamma(x) + ln(x).
    for value in [0.75f64, 1.1, 2.5, 10.0, 100.0] {
        let x = ld(value);
        let difference = x.add(LD::one()).lgamma().sub(x.lgamma());
        assert!(
            (difference.to_f64() - value.ln()).abs() <= 1e-24 * value.ln().abs().max(1.0),
            "lgamma recurrence at {value}"
        );
    }
}

#[test]
fn gamma_edges_exact_values_and_recurrence() {
    assert_eq!(ld(1.0).gamma().to_f64(), 1.0);
    assert_eq!(ld(2.0).gamma().to_f64(), 1.0);
    assert_eq!(ld(6.0).gamma().to_bits(), ld(120.0).to_bits());
    assert_eq!(ld(10.0).gamma().to_bits(), ld(362_880.0).to_bits());
    assert!(f64_ulp(ld(0.5).gamma().to_f64(), core::f64::consts::PI.sqrt()) <= 4);
    assert!(f64_ulp(
        ld(1.5).gamma().to_f64(),
        0.5 * core::f64::consts::PI.sqrt()
    ) <= 4);
    assert!(f64_ulp(
        ld(-0.5).gamma().to_f64(),
        -2.0 * core::f64::consts::PI.sqrt()
    ) <= 8);
    let positive_zero = ld(0.0).gamma();
    assert!(positive_zero.is_infinite() && !positive_zero.sign_bit());
    let negative_zero = ld(-0.0).gamma();
    assert!(negative_zero.is_infinite() && negative_zero.sign_bit());
    assert!(ld(-1.0).gamma().is_nan());
    assert!(ld(-2.0).gamma().is_nan());
    assert_eq!(ld(f64::INFINITY).gamma().to_f64(), f64::INFINITY);
    assert!(ld(f64::NEG_INFINITY).gamma().is_nan());
    assert!(ld(f64::NAN).gamma().is_nan());
    for value in [0.75f64, 1.3, 2.5, 5.0, 42.0, 900.0] {
        let x = ld(value);
        let left = x.add(LD::one()).gamma();
        let right = x.mul(x.gamma());
        let relative = left.sub(right).div(right).abs().to_f64();
        assert!(relative <= 1e-28, "Gamma recurrence at {value}: {relative:e}");
    }
}
EOF

if ! "$rustc" --edition 2021 --test -O -o "$tmp/harness" "$tmp/harness.rs" 2>"$tmp/build.err"; then
    echo "check-ld-special: FAIL harness build" >&2
    sed -n '1,30p' "$tmp/build.err" >&2
    exit 1
fi
if ! "$tmp/harness" >"$tmp/run.log" 2>&1; then
    echo "check-ld-special: FAIL edge matrix" >&2
    sed -n '1,40p' "$tmp/run.log" >&2
    exit 1
fi
echo "check-ld-special: ok $(grep -c '^test .* ok$' "$tmp/run.log") edge tests"

map=libc/ravenlibc/crates/raven-libc/Symbol.map
missing=''
for name in erfl erfcl gammal lgammal lgammal_r tgammal; do
    if ! grep -qE "^[[:space:]]*$name; # x86_64 aarch64$" "$map"; then
        missing="$missing $name"
    fi
done
if [ -n "$missing" ]; then
    echo "check-ld-special: FAIL Symbol.map rows missing aarch64:$missing" >&2
    exit 1
fi
echo "check-ld-special: ok Symbol.map carries the aarch64 tag for all 6 names"
echo "check-ld-special: PASS"
