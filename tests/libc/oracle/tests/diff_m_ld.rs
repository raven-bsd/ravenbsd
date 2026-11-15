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

use raven_libc_m::{LongDouble, expl};
use raven_libc_oracle::config::Config;
use raven_libc_oracle::decls::m_ld::oracle_rl_expl;
use raven_libc_oracle::rng::Rng;

/// The ULP budget of the default run (measured, see the C47e report).
const EXPL_BUDGET: u128 = 4;

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

#[test]
fn diff_expl() {
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
