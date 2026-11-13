// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `rsqrt` special values and accuracy against the host.

use super::{rsqrt, rsqrtf};

fn ulps(got: f64, want: f64) -> u64 {
    fn key(x: f64) -> u64 {
        let bits = x.to_bits();
        if bits >> 63 == 1 {
            !bits
        } else {
            bits | 0x8000_0000_0000_0000
        }
    }
    if got.is_nan() && want.is_nan() {
        return 0;
    }
    key(got).abs_diff(key(want))
}

#[test]
fn special_values_follow_the_donor() {
    assert_eq!(rsqrt(0.0), f64::INFINITY);
    assert_eq!(rsqrt(-0.0), f64::NEG_INFINITY);
    assert!(rsqrt(-1.0).is_nan());
    assert_eq!(rsqrt(f64::INFINITY), 0.0);
    assert!(rsqrt(f64::NEG_INFINITY).is_nan());
    assert!(rsqrt(f64::NAN).is_nan());
    assert_eq!(rsqrt(4.0), 0.5);
    assert_eq!(rsqrtf(4.0), 0.5);
    assert_eq!(rsqrtf(0.0), f32::INFINITY);
}

#[test]
fn accuracy_is_within_one_ulp() {
    for index in 1..20_000u64 {
        let x = (index as f64) * 0.001;
        let want = 1.0 / x.sqrt();
        assert!(ulps(rsqrt(x), want) <= 1, "rsqrt({x})");
    }
    let tiny = f64::from_bits(1);
    assert!(ulps(rsqrt(tiny), 1.0 / tiny.sqrt()) <= 1);
    for index in 1..20_000u32 {
        let x = (index as f32) * 0.001;
        let want = 1.0 / x.sqrt();
        assert!(
            (rsqrtf(x) - want).abs() <= f32::EPSILON * want * 2.0,
            "rsqrtf({x})"
        );
    }
}
