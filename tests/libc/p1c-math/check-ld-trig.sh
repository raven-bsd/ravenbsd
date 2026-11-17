#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# C47q aarch64 binary128 trig/hyperbolic edge evidence.
#
# The host cannot execute aarch64 `long double` code, but the kernel source is plain Rust and
# compiles for the host, so this builds `binary128.rs` as a host test binary (the same source the
# target crate compiles) and runs the edge matrix the ticket asks for: +-0, +-inf, NaN, huge
# arguments, the argument-reduction boundaries, the inverse-function domains and the
# sin^2+cos^2 / sincos / exp identities. It also checks that the 14 new names carry the aarch64
# tag in Symbol.map.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

rustc=${RUSTC:-rustc}
if ! command -v "$rustc" >/dev/null 2>&1; then
    echo "check-ld-trig: $rustc not found; cannot run the kernel matrix" >&2
    exit 2
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/c47q-trig.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

cp libc/ravenlibc/crates/raven-libc-m/src/binary128.rs "$tmp/binary128.rs"
# `binary128.rs` ends with `#[cfg(test)] mod tests;`; give that module an empty body so this
# harness runs only the edge matrix below (the crate's own 186 tests run under nextest).
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

#[test]
fn zeros_keep_their_signs_and_exact_values() {
    let zero = ld(0.0);
    let negative_zero = ld(-0.0);
    assert_eq!(zero.sin().to_bits(), 0);
    assert_eq!(negative_zero.sin().to_bits(), LD::from_bits(1u128 << 127).to_bits());
    assert_eq!(zero.cos().to_bits(), LD::one().to_bits());
    assert_eq!(negative_zero.cos().to_bits(), LD::one().to_bits());
    assert_eq!(zero.tan().to_bits(), 0);
    assert_eq!(
        negative_zero.tan().to_bits(),
        LD::from_bits(1u128 << 127).to_bits()
    );
    assert_eq!(zero.sinh().to_bits(), 0);
    assert_eq!(zero.cosh().to_bits(), LD::one().to_bits());
    assert_eq!(zero.tanh().to_bits(), 0);
    assert_eq!(zero.asin().to_bits(), 0);
    assert_eq!(zero.atanh().to_bits(), 0);
    assert_eq!(ld(1.0).acosh().to_bits(), 0);
    assert_eq!(ld(1.0).acos().to_bits(), 0);
    assert_eq!(
        ld(1.0).asin().to_bits(),
        LD::pio2_hi().add(LD::pio2_lo()).to_bits()
    );
    let quarter = LD::pi_hi().add(LD::pi_lo()).mul(ld(0.25));
    assert!(ulp(ld(1.0).atan(), quarter) <= 4);
}

#[test]
fn nan_and_infinities_follow_the_math_contract() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let x = ld(value);
        assert!(x.sin().is_nan());
        assert!(x.cos().is_nan());
        assert!(x.tan().is_nan());
        assert!(x.asin().is_nan());
        assert!(x.acos().is_nan());
        assert!(x.atanh().is_nan());
        let (sine, cosine) = x.sincos();
        assert!(sine.is_nan() && cosine.is_nan());
    }
    assert_eq!(ld(f64::INFINITY).sinh().to_f64(), f64::INFINITY);
    assert_eq!(ld(f64::NEG_INFINITY).sinh().to_f64(), f64::NEG_INFINITY);
    assert_eq!(ld(f64::NEG_INFINITY).cosh().to_f64(), f64::INFINITY);
    assert_eq!(ld(f64::INFINITY).tanh().to_f64(), 1.0);
    assert_eq!(ld(f64::NEG_INFINITY).tanh().to_f64(), -1.0);
    assert!(ld(f64::INFINITY).atan().is_finite());
    assert!(ld(f64::NEG_INFINITY).atan().is_finite());
    assert!(ld(f64::INFINITY).asinh().is_infinite());
}

#[test]
fn huge_arguments_stay_inside_the_documented_domain() {
    // The quadrant index fits an i64 below 2^63*pi/2, so 2^62 must still reduce.
    let inside = ld(2f64.powi(62));
    assert!(inside.sin().is_finite());
    assert!(inside.cos().is_finite());
    assert!(inside.sin().abs().to_f64() <= 1.0);
    // Beyond the i64 index (about 1.45e19) there is no Payne-Hanek path: NaN.
    let outside = ld(2.0e19);
    assert!(outside.sin().is_nan());
    assert!(outside.cos().is_nan());
    assert!(outside.tan().is_nan());
    // Inverse/hyperbolic functions have no such limit.
    assert!(ld(1.0e300).atan().is_finite());
    assert!(ld(1.0e300).asinh().is_finite());
    assert!(ld(1.0e300).acosh().is_finite());
}

#[test]
fn reduction_boundaries_keep_the_pythagorean_identity() {
    let one = LD::one();
    // Walk across several half-pi multiples using the kernel's own constant.
    for k in -24i64..=24 {
        let x = LD::from_i64(k).mul(LD::pio2_hi());
        let (sine, cosine) = x.sincos();
        assert_eq!(sine.to_bits(), x.sin().to_bits(), "sincos sin at k={k}");
        assert_eq!(cosine.to_bits(), x.cos().to_bits(), "sincos cos at k={k}");
        let norm = sine.mul(sine).add(cosine.mul(cosine));
        assert!(ulp(norm, one) <= 8, "sin^2+cos^2 at k={k}");
        // The nearest long double to k*pi/2 has a tiny sine or cosine; the kernel must keep it
        // finite and consistent rather than collapsing to a stub value.
        assert!(sine.is_finite() && cosine.is_finite(), "boundary k={k}");
    }
}

#[test]
fn inverse_domains_are_enforced() {
    assert!(ld(1.0000001).asin().is_nan());
    assert!(ld(-1.0000001).acos().is_nan());
    assert!(ld(0.999).acosh().is_nan());
    assert!(ld(1.0001).atanh().is_nan());
    assert_eq!(ld(1.0).atanh().to_f64(), f64::INFINITY);
    assert_eq!(ld(-1.0).atanh().to_f64(), f64::NEG_INFINITY);
    // atan2 quadrants and the infinity table.
    let zero = ld(0.0);
    assert_eq!(zero.atan2(ld(1.0)).to_bits(), 0);
    assert!(zero.atan2(ld(-1.0)).is_finite());
    assert!(zero.atan2(ld(-1.0)).to_f64() > 3.0);
    assert!(ld(1.0).atan2(zero).to_f64() > 1.5);
    assert!(ld(f64::INFINITY).atan2(ld(f64::INFINITY)).is_finite());
    assert!(ld(f64::NAN).atan2(ld(1.0)).is_nan());
}

#[test]
fn hyperbolic_overflow_edge_is_the_hyperbolic_one() {
    // exp overflows at ln(binary128 max) ~ 11356.5; sinh/cosh must survive a little longer.
    let edge = ld(11357.0);
    assert!(edge.exp().is_infinite());
    assert!(edge.sinh().is_finite());
    assert!(edge.cosh().is_finite());
    assert!(ld(12000.0).sinh().is_infinite());
    assert_eq!(ld(12000.0).tanh().to_f64(), 1.0);
    // The shifted exponential keeps the last binade finite but still rounds to inf past it.
    assert!(ld(1.0e5).sinh().is_infinite());
}
EOF

if ! "$rustc" --edition 2021 --test -O -o "$tmp/harness" "$tmp/harness.rs" 2>"$tmp/build.err"; then
    echo "check-ld-trig: FAIL harness build" >&2
    sed -n '1,30p' "$tmp/build.err" >&2
    exit 1
fi
if ! "$tmp/harness" >"$tmp/run.log" 2>&1; then
    echo "check-ld-trig: FAIL edge matrix" >&2
    sed -n '1,40p' "$tmp/run.log" >&2
    exit 1
fi
echo "check-ld-trig: ok $(grep -c '^test .* ok$' "$tmp/run.log") edge tests"

# The map rows for the 14 exports must carry the aarch64 tag.
map=libc/ravenlibc/crates/raven-libc/Symbol.map
missing=''
for name in sinl cosl tanl sincosl sinhl coshl tanhl asinl acosl atanl atan2l asinhl acoshl atanhl; do
    if ! grep -qE "^[[:space:]]*$name; # x86_64 aarch64$" "$map"; then
        missing="$missing $name"
    fi
done
if [ -n "$missing" ]; then
    echo "check-ld-trig: FAIL Symbol.map rows missing aarch64:$missing" >&2
    exit 1
fi
echo "check-ld-trig: ok Symbol.map carries the aarch64 tag for all 14 names"
echo "check-ld-trig: PASS"
