// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/support/float_traits.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (tests moved out of float_traits.rs; donor test bodies kept)

use super::*;

#[test]
#[cfg(f16_enabled)]
fn check_f16() {
    // Constants
    assert_eq!(f16::EXP_SAT, 0b11111);
    assert_eq!(f16::EXP_BIAS, 15);
    assert_eq!(f16::EXP_MAX, 15);
    assert_eq!(f16::EXP_MIN, -14);
    assert_eq!(f16::EXP_MIN_SUBNORM, -24);
    assert_biteq!(f16::NAN, <f16 as Float>::NAN);
    if MIPS_NAN {
        assert_biteq!(f16::NAN, f16::from_bits(0x7fbf));
        assert_biteq!(f16::SNAN, f16::from_bits(0x7fff));
    } else {
        assert_biteq!(f16::NAN, f16::from_bits(0x7e00));
        assert_biteq!(f16::SNAN, f16::from_bits(0x7d00));
    }
    assert!(f16::NAN.is_qnan());
    assert_eq!(f16::FRAC_PI_2.exp_unbiased(), 0);
    assert_eq!((1.0f16 / 2.0).exp_unbiased(), -1);
    assert_eq!(f16::MAX.exp_unbiased(), 15);
    assert_eq!(f16::MIN.exp_unbiased(), 15);
    assert_eq!(f16::MIN_POSITIVE.exp_unbiased(), -14);
    assert_eq!(f16::ZERO.exp_unbiased(), -15);
    assert_eq!(f16::from_bits(0x1).exp_unbiased(), -15);
    assert_eq!(f16::MIN_POSITIVE, f16::MIN_POSITIVE_NORMAL);
    assert_biteq!(f16::from_parts(true, f16::EXP_BIAS, 0), -1.0f16);
    assert_biteq!(f16::from_parts(false, 0, 1), f16::from_bits(0x1));
}

#[test]
fn check_f32() {
    assert_eq!(f32::EXP_SAT, 0b11111111);
    assert_eq!(f32::EXP_BIAS, 127);
    assert_eq!(f32::EXP_MAX, 127);
    assert_eq!(f32::EXP_MIN, -126);
    assert_eq!(f32::EXP_MIN_SUBNORM, -149);
    assert_biteq!(f32::NAN, <f32 as Float>::NAN);
    if MIPS_NAN {
        assert_biteq!(f32::NAN, f32::from_bits(0x7fbfffff));
        assert_biteq!(f32::SNAN, f32::from_bits(0x7fffffff));
    } else {
        assert_biteq!(f32::NAN, f32::from_bits(0x7fc00000));
        assert_biteq!(f32::SNAN, f32::from_bits(0x7fa00000));
    }
    assert!(f32::NAN.is_qnan());
    if !cfg!(x86_no_sse2) {
        assert!(f32::SNAN.is_snan());
    }
    assert_eq!(f32::FRAC_PI_2.exp_unbiased(), 0);
    assert_eq!((1.0f32 / 2.0).exp_unbiased(), -1);
    assert_eq!(f32::MAX.exp_unbiased(), 127);
    assert_eq!(f32::MIN.exp_unbiased(), 127);
    assert_eq!(f32::MIN_POSITIVE.exp_unbiased(), -126);
    assert_eq!(f32::ZERO.exp_unbiased(), -127);
    assert_eq!(f32::from_bits(0x1).exp_unbiased(), -127);
    assert_eq!(f32::MIN_POSITIVE, f32::MIN_POSITIVE_NORMAL);
    assert_biteq!(f32::from_parts(true, f32::EXP_BIAS, 0), -1.0f32);
    assert_biteq!(
        f32::from_parts(false, 10 + f32::EXP_BIAS, 0),
        hf32!("0x1p10")
    );
    assert_biteq!(f32::from_parts(false, 0, 1), f32::from_bits(0x1));
}

#[test]
fn check_f64() {
    assert_eq!(f64::EXP_SAT, 0b11111111111);
    assert_eq!(f64::EXP_BIAS, 1023);
    assert_eq!(f64::EXP_MAX, 1023);
    assert_eq!(f64::EXP_MIN, -1022);
    assert_eq!(f64::EXP_MIN_SUBNORM, -1074);
    assert_biteq!(f64::NAN, <f64 as Float>::NAN);
    if MIPS_NAN {
        assert_biteq!(f64::NAN, f64::from_bits(0x7ff7ffffffffffff));
        assert_biteq!(f64::SNAN, f64::from_bits(0x7fffffffffffffff));
    } else {
        assert_biteq!(f64::NAN, f64::from_bits(0x7ff8000000000000));
        assert_biteq!(f64::SNAN, f64::from_bits(0x7ff4000000000000));
    }
    assert!(f64::NAN.is_qnan());
    if !cfg!(x86_no_sse2) {
        assert!(f64::SNAN.is_snan());
    }
    assert_eq!(f64::FRAC_PI_2.exp_unbiased(), 0);
    assert_eq!((1.0f64 / 2.0).exp_unbiased(), -1);
    assert_eq!(f64::MAX.exp_unbiased(), 1023);
    assert_eq!(f64::MIN.exp_unbiased(), 1023);
    assert_eq!(f64::MIN_POSITIVE.exp_unbiased(), -1022);
    assert_eq!(f64::ZERO.exp_unbiased(), -1023);
    assert_eq!(f64::from_bits(0x1).exp_unbiased(), -1023);
    assert_eq!(f64::MIN_POSITIVE, f64::MIN_POSITIVE_NORMAL);
    assert_biteq!(f64::from_parts(true, f64::EXP_BIAS, 0), -1.0f64);
    assert_biteq!(
        f64::from_parts(false, 10 + f64::EXP_BIAS, 0),
        hf64!("0x1p10")
    );
    assert_biteq!(f64::from_parts(false, 0, 1), f64::from_bits(0x1));
}
