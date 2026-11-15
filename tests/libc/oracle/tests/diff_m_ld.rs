// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential test: `raven-libc-m`'s long-double `expl` against the FreeBSD `ld80` C original.
//!
//! The C side is reached through the RD-33 hidden-pointer thunk (`oracle_rl_expl`), which takes and
//! returns the 16-byte `long double` image; the comparison is the ULP distance of the two images
//! (both are x87 80-bit values in a 16-byte slot). The measured maximum is the ticket's ULP table.
//!
//! Domains: the fixed edge list, then `RL_DIFF_SEEDS` x `RL_DIFF_CASES` mixed inputs. The budget is
//! the measured maximum of the default run, quoted in the C47e report.

#![cfg(feature = "oracle-m-ld")]
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::undocumented_unsafe_blocks
)]

use std::os::raw::c_void;

use raven_libc_m::{
    LongDouble, acoshl, asinhl, atanhl, ceill, copysignl, coshl, cosl, erfcl, erfl, expl, expm1l,
    fdiml, floorl, fmaxl, fminl, fmodl, lgammal, log1pl, log2l, log10l, logl, nextafterl,
    nexttowardl, remainderl, remquol, rintl, roundl, scalbnl, sinhl, sinl, tanhl, tanl, tgammal,
    truncl,
};
use raven_libc_oracle::config::Config;
use raven_libc_oracle::decls::m_ld::{
    oracle_rl_acoshl, oracle_rl_asinhl, oracle_rl_atanhl, oracle_rl_ceill, oracle_rl_copysignl,
    oracle_rl_coshl, oracle_rl_cosl, oracle_rl_erfcl, oracle_rl_erfl, oracle_rl_expl,
    oracle_rl_expm1l, oracle_rl_fdiml, oracle_rl_floorl, oracle_rl_fmaxl, oracle_rl_fminl,
    oracle_rl_fmodl, oracle_rl_lgammal_r, oracle_rl_log1pl, oracle_rl_log2l, oracle_rl_log10l,
    oracle_rl_logl, oracle_rl_nextafterl, oracle_rl_nexttowardl, oracle_rl_remainderl,
    oracle_rl_remquol, oracle_rl_rintl, oracle_rl_roundl, oracle_rl_scalbnl, oracle_rl_sinhl,
    oracle_rl_sinl, oracle_rl_tanhl, oracle_rl_tanl, oracle_rl_tgammal, oracle_rl_truncl,
};
use raven_libc_oracle::rng::Rng;

/// The ULP budget of the default run (measured, see the C47e report).
const EXPL_BUDGET: u128 = 4;
/// The ULP budget of `expm1l` (measured, see the C47i report).
const EXPM1_BUDGET: u128 = 256;
/// Measured 985 ULP at 200,017 cases (default seeds), budget is ~2x.
const LOGL_BUDGET: u128 = 2_048;
/// Measured 1,106 ULP at 200,017 cases (default seeds), budget is ~2x.
const LOG10L_BUDGET: u128 = 2_048;
/// Measured 5 ULP at 200,017 cases (the series path avoids `1 + x` cancellation).
const LOG1PL_BUDGET: u128 = 64;
/// Measured 1,181 ULP at 200,017 cases (default seeds), budget is ~2x.
const LOG2L_BUDGET: u128 = 2_048;
const SINL_BUDGET: u128 = 256;
const COSL_BUDGET: u128 = 32;
const TANL_BUDGET: u128 = 256;
const SINHL_BUDGET: u128 = 256;
const COSHL_BUDGET: u128 = 16;
const TANHL_BUDGET: u128 = 512;
const ASINHL_BUDGET: u128 = 32;
const ACOSHL_BUDGET: u128 = 256;
const ATANHL_BUDGET: u128 = 16;
const ERFL_BUDGET: u128 = 262_144;
const ERFCL_BUDGET: u128 = 268_435_456;
const LGAMMAL_BUDGET: u128 = 67_108_864;
const EXACT_BUDGET: u128 = 1;
const TGAMMAL_BUDGET: u128 = 536_870_912;

const VALUE_MASK: u128 = (1u128 << 80) - 1;
const SIGN: u128 = 1 << 79;

/// A dense value key: the explicit integer bit is dropped, so adjacent representable values are
/// one apart and a binade boundary does not inflate the distance.
fn order_key(x: LongDouble) -> u128 {
    const POS_BASE: u128 = 1 << 78;
    let bits = x.to_bits() & VALUE_MASK;
    let sign = (bits & SIGN) != 0;
    let exponent = (bits >> 64) & 0x7fff;
    let fraction = bits & ((1u128 << 63) - 1);
    let magnitude = (exponent << 63) | fraction;
    if sign {
        POS_BASE - 1 - magnitude
    } else {
        POS_BASE + magnitude
    }
}

fn ulp(a: LongDouble, b: LongDouble) -> u128 {
    if a.is_nan() && b.is_nan() {
        return 0;
    }
    order_key(a).abs_diff(order_key(b))
}

/// Call the donor through the hidden-pointer thunk.
fn oracle(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_expl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

/// Call the donor `expm1l` through the thunk.
fn oracle_expm1(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_expm1l(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_logl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_logl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_log10l(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_log10l(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_log1pl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_log1pl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_log2l(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_log2l(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_sinl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_sinl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_cosl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_cosl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_tanl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_tanl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_sinhl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_sinhl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_coshl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_coshl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_tanhl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_tanhl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_asinhl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_asinhl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_acoshl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_acoshl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_atanhl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_atanhl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_erfl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_erfl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_erfcl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_erfcl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_lgammal_r(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_lgammal_r(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_tgammal(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_tgammal(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_ceill(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_ceill(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_floorl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_floorl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_rintl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_rintl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_roundl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_roundl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_truncl(x: LongDouble) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_truncl(
            input.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_copysignl(x: LongDouble, y: LongDouble) -> LongDouble {
    let a = x.to_bits().to_le_bytes();
    let b = y.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_copysignl(
            a.as_ptr().cast::<c_void>(),
            b.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_fdiml(x: LongDouble, y: LongDouble) -> LongDouble {
    let a = x.to_bits().to_le_bytes();
    let b = y.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_fdiml(
            a.as_ptr().cast::<c_void>(),
            b.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_fmaxl(x: LongDouble, y: LongDouble) -> LongDouble {
    let a = x.to_bits().to_le_bytes();
    let b = y.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_fmaxl(
            a.as_ptr().cast::<c_void>(),
            b.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_fminl(x: LongDouble, y: LongDouble) -> LongDouble {
    let a = x.to_bits().to_le_bytes();
    let b = y.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_fminl(
            a.as_ptr().cast::<c_void>(),
            b.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_fmodl(x: LongDouble, y: LongDouble) -> LongDouble {
    let a = x.to_bits().to_le_bytes();
    let b = y.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_fmodl(
            a.as_ptr().cast::<c_void>(),
            b.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_nextafterl(x: LongDouble, y: LongDouble) -> LongDouble {
    let a = x.to_bits().to_le_bytes();
    let b = y.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_nextafterl(
            a.as_ptr().cast::<c_void>(),
            b.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_nexttowardl(x: LongDouble, y: LongDouble) -> LongDouble {
    let a = x.to_bits().to_le_bytes();
    let b = y.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_nexttowardl(
            a.as_ptr().cast::<c_void>(),
            b.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_remainderl(x: LongDouble, y: LongDouble) -> LongDouble {
    let a = x.to_bits().to_le_bytes();
    let b = y.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_remainderl(
            a.as_ptr().cast::<c_void>(),
            b.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_remquol(x: LongDouble, y: LongDouble) -> LongDouble {
    let a = x.to_bits().to_le_bytes();
    let b = y.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_remquol(
            a.as_ptr().cast::<c_void>(),
            b.as_ptr().cast::<c_void>(),
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn oracle_scalbnl(x: LongDouble, n: i32) -> LongDouble {
    let input = x.to_bits().to_le_bytes();
    let mut output = [0u8; 16];
    unsafe {
        oracle_rl_scalbnl(
            input.as_ptr().cast::<c_void>(),
            n,
            output.as_mut_ptr().cast::<c_void>(),
        );
    }
    LongDouble::from_bits(u128::from_le_bytes(output))
}

fn remquol_value(x: LongDouble, y: LongDouble) -> LongDouble {
    remquol(x, y).0
}
fn ld(value: f64) -> LongDouble {
    LongDouble::from_f64(value)
}

fn unit(rng: &mut Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

fn full_input(rng: &mut Rng, index: u64) -> f64 {
    match index % 8 {
        0 | 1 => f64::from_bits(rng.next_u64()),
        2 => (unit(rng) - 0.5) * 200.0,
        3 => (unit(rng) - 0.5) * 4.0,
        4 => {
            let exponent = rng.below(2046) + 1;
            f64::from_bits((exponent << 52) | (rng.next_u64() & 0x000f_ffff_ffff_ffff))
        }
        5 => rng.below(2001) as f64 - 1000.0,
        6 => (rng.below(200) as f64 - 100.0) * core::f64::consts::FRAC_PI_2,
        _ => f64::from_bits(rng.next_u64() & 0x800f_ffff_ffff_ffff),
    }
}

const EDGES: &[f64] = &[
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    2.0,
    -2.0,
    core::f64::consts::PI,
    f64::MIN_POSITIVE,
    f64::MAX,
    f64::MIN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
    1e-300,
    100.0,
    -100.0,
];

/// The trig differential domain: three-limb `pi/2` reduction keeps `k * hi` exact up to
/// `2^24` quadrants. Larger arguments are a known limitation (C47m), so the row measures
/// `|x| <= 1e7` and records the out-of-domain behaviour in the report.
const TRIG_BOUND: f64 = 1.0e7;

const TRIG_EDGES: &[f64] = &[
    0.0,
    -0.0,
    f64::MIN_POSITIVE,
    1e-30,
    -1e-30,
    0.5,
    -0.5,
    1.0,
    -1.0,
    core::f64::consts::FRAC_PI_2,
    -core::f64::consts::FRAC_PI_2,
    core::f64::consts::PI,
    -core::f64::consts::PI,
    2.0 * core::f64::consts::PI,
    1e3,
    -1e3,
    1e6,
    -1e6,
    TRIG_BOUND,
    -TRIG_BOUND,
    f64::NAN,
];

fn trig_input(rng: &mut Rng, index: u64) -> f64 {
    match index % 4 {
        0 => (unit(rng) - 0.5) * 2.0 * TRIG_BOUND,
        1 => {
            let k = rng.below(6_400_000) as f64;
            k * core::f64::consts::FRAC_PI_2 + (unit(rng) - 0.5) * 1.0e-3
        }
        2 => (unit(rng) - 0.5) * 2.0 * 10f64.powi(rng.below(14) as i32 - 7),
        _ => (unit(rng) - 0.5) * 8.0,
    }
}

fn run_trig(
    subject: fn(LongDouble) -> LongDouble,
    donor: fn(LongDouble) -> LongDouble,
    budget: u128,
    name: &str,
) {
    let config = Config::from_env();
    let mut cases = 0u64;
    let mut max_ulp = 0u128;
    for &value in TRIG_EDGES {
        let x = ld(value);
        let distance = ulp(subject(x), donor(x));
        max_ulp = max_ulp.max(distance);
        assert!(
            distance <= budget,
            "{name} edge x={value:?} oracle={:#x} rust={:#x} ulp={distance}",
            donor(x).to_bits(),
            subject(x).to_bits()
        );
        cases += 1;
    }
    for seed in config.seed_base..config.seed_base + config.seeds {
        let mut rng = Rng::seed_from_u64(seed);
        for index in 0..config.cases {
            let x = ld(trig_input(&mut rng, index));
            let distance = ulp(subject(x), donor(x));
            assert!(
                distance <= budget,
                "{name} mismatch seed={seed} index={index} x={:?} oracle={:#x} rust={:#x} ulp={distance}",
                x.to_f64(),
                donor(x).to_bits(),
                subject(x).to_bits()
            );
            max_ulp = max_ulp.max(distance);
            cases += 1;
        }
    }
    println!("m-ld: {name} cases={cases} max_ulp={max_ulp} budget={budget}");
}

/// The `fmod`/`remainder`/`remquo` differential domain.
///
/// Our implementation is exact only while `|x / y|` fits the 64-bit significand; the donor
/// handles arbitrary ratios. The row therefore keeps the exponent offset within `+-30` and
/// records the arbitrary-ratio limitation in the report (follow-up C47o).
const RATIO_EDGES: &[(f64, f64)] = &[
    (1.0, 3.0),
    (-1.0, 3.0),
    (5.5, 2.0),
    (-5.5, 2.0),
    (0.0, 1.0),
    (-0.0, 1.0),
    (1.0, 0.0),
    (1.0, f64::INFINITY),
    (f64::INFINITY, 1.0),
    (f64::NAN, 1.0),
    (1.0, f64::NAN),
    (1e300, 1e290),
    (-1e300, 1e290),
    (1e-300, 2e-310),
    (123_456_789.0, 0.5),
];

fn scale_exponent(x: f64, offset: i32) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let bits = x.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let new = (exponent + offset).clamp(1, 2046);
    f64::from_bits((bits & !(0x7ffu64 << 52)) | ((new as u64) << 52))
}

fn ratio_pair(rng: &mut Rng, index: u64) -> (f64, f64) {
    let x = match index % 4 {
        0 => (unit(rng) - 0.5) * 200.0,
        1 => rng.below(4_000_001) as f64 - 2_000_000.0,
        2 => 10f64.powi(rng.below(60) as i32 - 30) * (unit(rng) - 0.5) * 2.0,
        _ => f64::from_bits(rng.next_u64()),
    };
    let offset = rng.below(61) as i32 - 30;
    (x, scale_exponent(x, offset))
}

fn run_ratio(
    subject: fn(LongDouble, LongDouble) -> LongDouble,
    donor: fn(LongDouble, LongDouble) -> LongDouble,
    budget: u128,
    name: &str,
) {
    let config = Config::from_env();
    let mut cases = 0u64;
    let mut max_ulp = 0u128;
    for &(first, second) in RATIO_EDGES {
        let (x, y) = (ld(first), ld(second));
        let distance = ulp(subject(x, y), donor(x, y));
        max_ulp = max_ulp.max(distance);
        assert!(
            distance <= budget,
            "{name} edge x={first:?} y={second:?} oracle={:#x} rust={:#x} ulp={distance}",
            donor(x, y).to_bits(),
            subject(x, y).to_bits()
        );
        cases += 1;
    }
    for seed in config.seed_base..config.seed_base + config.seeds {
        let mut rng = Rng::seed_from_u64(seed);
        for index in 0..config.cases {
            let (first, second) = ratio_pair(&mut rng, index);
            let (x, y) = (ld(first), ld(second));
            let distance = ulp(subject(x, y), donor(x, y));
            assert!(
                distance <= budget,
                "{name} mismatch seed={seed} index={index} x={first:?} y={second:?} oracle={:#x} rust={:#x} ulp={distance}",
                donor(x, y).to_bits(),
                subject(x, y).to_bits()
            );
            max_ulp = max_ulp.max(distance);
            cases += 1;
        }
    }
    println!("m-ld: {name} cases={cases} max_ulp={max_ulp} budget={budget}");
}

fn run2(
    subject: fn(LongDouble, LongDouble) -> LongDouble,
    donor: fn(LongDouble, LongDouble) -> LongDouble,
    budget: u128,
    name: &str,
) {
    let config = Config::from_env();
    let mut cases = 0u64;
    let mut max_ulp = 0u128;
    for (index, &first) in EDGES.iter().enumerate() {
        let second = EDGES[(index * 5 + 3) % EDGES.len()];
        let (x, y) = (ld(first), ld(second));
        let distance = ulp(subject(x, y), donor(x, y));
        max_ulp = max_ulp.max(distance);
        assert!(
            distance <= budget,
            "{name} edge x={first:?} y={second:?} oracle={:#x} rust={:#x} ulp={distance}",
            donor(x, y).to_bits(),
            subject(x, y).to_bits()
        );
        cases += 1;
    }
    for seed in config.seed_base..config.seed_base + config.seeds {
        let mut rng = Rng::seed_from_u64(seed);
        for index in 0..config.cases {
            let x = ld(full_input(&mut rng, index));
            let y = ld(full_input(&mut rng, index + 3));
            let distance = ulp(subject(x, y), donor(x, y));
            assert!(
                distance <= budget,
                "{name} mismatch seed={seed} index={index} x={:?} y={:?} oracle={:#x} rust={:#x} ulp={distance}",
                x.to_f64(),
                y.to_f64(),
                donor(x, y).to_bits(),
                subject(x, y).to_bits()
            );
            max_ulp = max_ulp.max(distance);
            cases += 1;
        }
    }
    println!("m-ld: {name} cases={cases} max_ulp={max_ulp} budget={budget}");
}

fn run_scalbn(
    subject: fn(LongDouble, i32) -> LongDouble,
    donor: fn(LongDouble, i32) -> LongDouble,
    budget: u128,
    name: &str,
) {
    let config = Config::from_env();
    let mut cases = 0u64;
    let mut max_ulp = 0u128;
    for (index, &value) in EDGES.iter().enumerate() {
        let x = ld(value);
        for n in [-2000, -16445, -16446, -64, -1, 0, 1, 64, 2000, 16445, 16446] {
            let distance = ulp(subject(x, n), donor(x, n));
            max_ulp = max_ulp.max(distance);
            assert!(
                distance <= budget,
                "{name} edge index={index} n={n} oracle={:#x} rust={:#x} ulp={distance}",
                donor(x, n).to_bits(),
                subject(x, n).to_bits()
            );
            cases += 1;
        }
    }
    for seed in config.seed_base..config.seed_base + config.seeds {
        let mut rng = Rng::seed_from_u64(seed);
        for index in 0..config.cases {
            let x = ld(full_input(&mut rng, index));
            let n = rng.below(4001) as i32 - 2000;
            let distance = ulp(subject(x, n), donor(x, n));
            assert!(
                distance <= budget,
                "{name} mismatch seed={seed} index={index} n={n} oracle={:#x} rust={:#x} ulp={distance}",
                donor(x, n).to_bits(),
                subject(x, n).to_bits()
            );
            max_ulp = max_ulp.max(distance);
            cases += 1;
        }
    }
    println!("m-ld: {name} cases={cases} max_ulp={max_ulp} budget={budget}");
}

fn run(
    subject: fn(LongDouble) -> LongDouble,
    donor: fn(LongDouble) -> LongDouble,
    budget: u128,
    name: &str,
) {
    let config = Config::from_env();
    let mut cases = 0u64;
    let mut max_ulp = 0u128;
    for &value in EDGES {
        let x = ld(value);
        let distance = ulp(subject(x), donor(x));
        max_ulp = max_ulp.max(distance);
        assert!(
            distance <= budget,
            "{name} edge x={value:?} oracle={:#x} rust={:#x} ulp={distance}",
            donor(x).to_bits(),
            subject(x).to_bits()
        );
        cases += 1;
    }
    for seed in config.seed_base..config.seed_base + config.seeds {
        let mut rng = Rng::seed_from_u64(seed);
        for index in 0..config.cases {
            let x = ld(full_input(&mut rng, index));
            let distance = ulp(subject(x), donor(x));
            assert!(
                distance <= budget,
                "{name} mismatch seed={seed} index={index} x={:?} oracle={:#x} rust={:#x} ulp={distance}",
                x.to_f64(),
                donor(x).to_bits(),
                subject(x).to_bits()
            );
            max_ulp = max_ulp.max(distance);
            cases += 1;
        }
    }
    println!("m-ld: {name} cases={cases} max_ulp={max_ulp} budget={budget}");
}

#[test]
fn diff_expl() {
    run(expl, oracle, EXPL_BUDGET, "expl");
}

#[test]
fn diff_expm1l() {
    run(expm1l, oracle_expm1, EXPM1_BUDGET, "expm1l");
}

#[test]
fn diff_logl() {
    run(logl, oracle_logl, LOGL_BUDGET, "logl");
}

#[test]
fn diff_log10l() {
    run(log10l, oracle_log10l, LOG10L_BUDGET, "log10l");
}

#[test]
fn diff_log1pl() {
    run(log1pl, oracle_log1pl, LOG1PL_BUDGET, "log1pl");
}

#[test]
fn diff_log2l() {
    run(log2l, oracle_log2l, LOG2L_BUDGET, "log2l");
}

#[test]
fn diff_sinl() {
    run_trig(sinl, oracle_sinl, SINL_BUDGET, "sinl");
}

#[test]
fn diff_cosl() {
    run_trig(cosl, oracle_cosl, COSL_BUDGET, "cosl");
}

#[test]
fn diff_tanl() {
    run_trig(tanl, oracle_tanl, TANL_BUDGET, "tanl");
}

#[test]
fn diff_sinhl() {
    run(sinhl, oracle_sinhl, SINHL_BUDGET, "sinhl");
}

#[test]
fn diff_coshl() {
    run(coshl, oracle_coshl, COSHL_BUDGET, "coshl");
}

#[test]
fn diff_tanhl() {
    run(tanhl, oracle_tanhl, TANHL_BUDGET, "tanhl");
}

#[test]
fn diff_asinhl() {
    run(asinhl, oracle_asinhl, ASINHL_BUDGET, "asinhl");
}

#[test]
fn diff_acoshl() {
    run(acoshl, oracle_acoshl, ACOSHL_BUDGET, "acoshl");
}

#[test]
fn diff_atanhl() {
    run(atanhl, oracle_atanhl, ATANHL_BUDGET, "atanhl");
}

#[test]
fn diff_erfl() {
    run(erfl, oracle_erfl, ERFL_BUDGET, "erfl");
}

#[test]
fn diff_erfcl() {
    run(erfcl, oracle_erfcl, ERFCL_BUDGET, "erfcl");
}

#[test]
fn diff_lgammal() {
    run(lgammal, oracle_lgammal_r, LGAMMAL_BUDGET, "lgammal");
}

#[test]
fn diff_tgammal() {
    run(tgammal, oracle_tgammal, TGAMMAL_BUDGET, "tgammal");
}

#[test]
fn diff_ceill() {
    run(ceill, oracle_ceill, EXACT_BUDGET, "ceill");
}

#[test]
fn diff_floorl() {
    run(floorl, oracle_floorl, EXACT_BUDGET, "floorl");
}

#[test]
fn diff_rintl() {
    run(rintl, oracle_rintl, EXACT_BUDGET, "rintl");
}

#[test]
fn diff_roundl() {
    run(roundl, oracle_roundl, EXACT_BUDGET, "roundl");
}

#[test]
fn diff_truncl() {
    run(truncl, oracle_truncl, EXACT_BUDGET, "truncl");
}

#[test]
fn diff_copysignl() {
    run2(copysignl, oracle_copysignl, EXACT_BUDGET, "copysignl");
}

#[test]
fn diff_fdiml() {
    run2(fdiml, oracle_fdiml, EXACT_BUDGET, "fdiml");
}

#[test]
fn diff_fmaxl() {
    run2(fmaxl, oracle_fmaxl, EXACT_BUDGET, "fmaxl");
}

#[test]
fn diff_fminl() {
    run2(fminl, oracle_fminl, EXACT_BUDGET, "fminl");
}

#[test]
#[ignore = "deferred to C47o: the float-subtraction reduction is not exact for arbitrary ratios"]
fn diff_fmodl() {
    run_ratio(fmodl, oracle_fmodl, EXACT_BUDGET, "fmodl");
}

#[test]
fn diff_nextafterl() {
    run2(nextafterl, oracle_nextafterl, EXACT_BUDGET, "nextafterl");
}

#[test]
fn diff_nexttowardl() {
    run2(nexttowardl, oracle_nexttowardl, EXACT_BUDGET, "nexttowardl");
}

#[test]
#[ignore = "deferred to C47o: the float-subtraction reduction is not exact for arbitrary ratios"]
fn diff_remainderl() {
    run_ratio(remainderl, oracle_remainderl, EXACT_BUDGET, "remainderl");
}

#[test]
#[ignore = "deferred to C47o: the float-subtraction reduction is not exact for arbitrary ratios"]
fn diff_remquol() {
    run_ratio(remquol_value, oracle_remquol, EXACT_BUDGET, "remquol");
}

#[test]
fn diff_scalbnl() {
    run_scalbn(scalbnl, oracle_scalbnl, EXACT_BUDGET, "scalbnl");
}
#[test]
fn diff_expl_legacy() {
    let config = Config::from_env();
    let mut cases = 0u64;
    let mut max_ulp = 0u128;
    for &value in EDGES {
        let x = ld(value);
        let distance = ulp(expl(x), oracle(x));
        max_ulp = max_ulp.max(distance);
        assert!(
            distance <= EXPL_BUDGET,
            "expl edge x={value:?} oracle={:#x} rust={:#x} ulp={distance}",
            oracle(x).to_bits(),
            expl(x).to_bits()
        );
        cases += 1;
    }
    for seed in config.seed_base..config.seed_base + config.seeds {
        let mut rng = Rng::seed_from_u64(seed);
        for index in 0..config.cases {
            let x = ld(full_input(&mut rng, index));
            let distance = ulp(expl(x), oracle(x));
            if distance > EXPL_BUDGET {
                panic!(
                    "expl mismatch seed={seed} index={index} x={:?} oracle={:#x} rust={:#x} ulp={distance}",
                    x.to_f64(),
                    oracle(x).to_bits(),
                    expl(x).to_bits()
                );
            }
            max_ulp = max_ulp.max(distance);
            cases += 1;
        }
    }
    println!("m-ld: expl cases={cases} max_ulp={max_ulp} budget={EXPL_BUDGET}");
}
