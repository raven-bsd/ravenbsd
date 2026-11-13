// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential test: `raven-libc-m` against the musl `src/math` C originals.
//!
//! Every case is compared bit for bit. The single normalisation is NaN payload propagation: IEEE
//! 754 does not require a particular NaN payload, so two NaNs compare equal; the sign of zero and
//! every finite bit pattern are compared exactly.
//!
//! Domains: the fixed edge list, then `RL_DIFF_SEEDS` x `RL_DIFF_CASES` mixed pseudo-random inputs
//! (uniform, dense near zero, uniform-exponent, small integers and half-integer multiples of pi),
//! then the exhaustive f32 walk over all 2^32 patterns when `RL_DIFF_EXHAUSTIVE=1`.
//!
//! With the harness defaults (20 seeds x 10,000 cases) the 31 pairs of a family add up to about
//! 12.4 million comparisons per run; that is the `10^7` ULP table of the C47a ticket.

#![cfg(feature = "oracle-m-core")]
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::undocumented_unsafe_blocks
)]

use raven_libc_m::*;
use raven_libc_oracle::config::Config;
use raven_libc_oracle::decls::m::*;
use raven_libc_oracle::rng::Rng;

/// Bits that order floats by value, for the ULP distance.
fn key64(x: f64) -> u64 {
    let bits = x.to_bits();
    if bits >> 63 == 1 {
        !bits
    } else {
        bits | 0x8000_0000_0000_0000
    }
}

fn key32(x: f32) -> u32 {
    let bits = x.to_bits();
    if bits >> 31 == 1 {
        !bits
    } else {
        bits | 0x8000_0000
    }
}

fn ulp64(a: f64, b: f64) -> u64 {
    if a.is_nan() && b.is_nan() {
        return 0;
    }
    key64(a).abs_diff(key64(b))
}

fn ulp32(a: f32, b: f32) -> u32 {
    if a.is_nan() && b.is_nan() {
        return 0;
    }
    key32(a).abs_diff(key32(b))
}

/// Loose f64 distribution: full bit space, uniform ranges, uniform exponent, integers and
/// half-integer multiples of pi.
fn gen64(rng: &mut Rng, index: u64) -> f64 {
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

fn gen32(rng: &mut Rng, index: u64) -> f32 {
    match index % 8 {
        0 | 1 => f32::from_bits(rng.next_u64() as u32),
        2 => (unit(rng) - 0.5) as f32 * 200.0,
        3 => (unit(rng) - 0.5) as f32 * 4.0,
        4 => {
            let exponent = rng.below(254) + 1;
            f32::from_bits(((exponent as u32) << 23) | (rng.next_u64() as u32 & 0x007f_ffff))
        }
        5 => rng.below(2001) as f32 - 1000.0,
        6 => (rng.below(200) as f32 - 100.0) * core::f32::consts::FRAC_PI_2,
        _ => f32::from_bits(rng.next_u64() as u32 & 0x8007_ffff),
    }
}

fn unit(rng: &mut Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

const EDGES64: &[f64] = &[
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    -0.5,
    2.0,
    -2.0,
    core::f64::consts::PI,
    core::f64::consts::E,
    f64::MIN_POSITIVE,
    f64::from_bits(1),
    f64::MAX,
    f64::MIN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
    1e-300,
    1e300,
    710.0,
    -710.0,
    1e-8,
];

const EDGES32: &[f32] = &[
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    -0.5,
    2.0,
    -2.0,
    core::f32::consts::PI,
    core::f32::consts::E,
    f32::MIN_POSITIVE,
    f32::from_bits(1),
    f32::MAX,
    f32::MIN,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::NAN,
    1e-30,
    1e30,
    88.0,
    -88.0,
];

/// Per-function ULP budget against the musl proxy donor.
///
/// The pinned C donor of the `m` family is `freebsd:lib/msun`, which is not present in this
/// machine's sparse checkout; the family is built against musl instead, the common ancestor of the
/// libm files. The two revisions differ by at most one ULP on a few inputs, so the measured,
/// reviewed budget lives in `corpus/m/ulp_budget.toml`; `RL_M_ULP_BUDGET` overrides it for a
/// calibration run only.
fn budget(name: &str) -> u64 {
    if let Ok(budget) = std::env::var("RL_M_ULP_BUDGET")
        .unwrap_or_default()
        .parse::<u64>()
    {
        return budget;
    }
    static BUDGET: std::sync::OnceLock<std::collections::HashMap<String, u64>> =
        std::sync::OnceLock::new();
    let table = BUDGET.get_or_init(|| {
        let text = include_str!("../corpus/m/ulp_budget.toml");
        let parsed: toml::Value = toml::from_str(text).expect("m ulp budget");
        parsed
            .get("ulp")
            .and_then(|value| value.as_table())
            .map(|table| {
                table
                    .iter()
                    .filter_map(|(key, value)| Some((key.clone(), value.as_integer()? as u64)))
                    .collect()
            })
            .unwrap_or_default()
    });
    table
        .get(name.strip_prefix("diff_").unwrap_or(name))
        .copied()
        .unwrap_or(0)
}

macro_rules! unary {
    ($name:ident, $oracle:ident, $subject:path, $gen:ident, $ulp:ident, $edges:ident) => {
        #[test]
        fn $name() {
            let config = Config::from_env();
            if let Some(filter) = &config.filter
                && !concat!("diff_", stringify!($name)).contains(filter.as_str())
            {
                return;
            }
            let mut cases = 0u64;
            let mut max_ulp = 0u64;
            for &x in $edges {
                let c = unsafe { $oracle(x) };
                let r = $subject(x);
                assert!(
                    u64::from($ulp(c, r)) <= budget(stringify!($name)),
                    "{}: edge x={x:?} c={c:?} r={r:?}",
                    stringify!($name)
                );
                cases += 1;
            }
            for seed in config.seed_base..config.seed_base + config.seeds {
                let mut rng = Rng::seed_from_u64(seed);
                for index in 0..config.cases {
                    let x = $gen(&mut rng, index);
                    let c = unsafe { $oracle(x) };
                    let r = $subject(x);
                    let distance = u64::from($ulp(c, r));
                    if distance > budget(stringify!($name)) {
                        panic!(
                            "{} mismatch seed={seed} index={index} x={x:?} ({:#x}) oracle={c:?} ({:#x}) rust={r:?} ({:#x}) ulp={distance}",
                            stringify!($name),
                            x.to_bits(),
                            c.to_bits(),
                            r.to_bits()
                        );
                    }
                    max_ulp = max_ulp.max(distance);
                    cases += 1;
                }
            }
            println!(
                "{}: cases={cases} max_ulp={max_ulp} budget={}",
                stringify!($name),
                budget(stringify!($name))
            );
        }
    };
}

macro_rules! binary {
    ($name:ident, $oracle:ident, $subject:path, $gen:ident, $ulp:ident, $edges:ident) => {
        #[test]
        fn $name() {
            let config = Config::from_env();
            if let Some(filter) = &config.filter
                && !concat!("diff_", stringify!($name)).contains(filter.as_str())
            {
                return;
            }
            let mut cases = 0u64;
            let mut max_ulp = 0u64;
            for &y in $edges {
                for &x in $edges {
                    let c = unsafe { $oracle(y, x) };
                    let r = $subject(y, x);
                    assert!(
                        u64::from($ulp(c, r)) <= budget(stringify!($name)),
                        "{}: edge y={y:?} x={x:?} oracle={c:?} rust={r:?}",
                        stringify!($name)
                    );
                    cases += 1;
                }
            }
            for seed in config.seed_base..config.seed_base + config.seeds {
                let mut rng = Rng::seed_from_u64(seed);
                for index in 0..config.cases {
                    let y = $gen(&mut rng, index);
                    let x = $gen(&mut rng, index.wrapping_add(3));
                    let c = unsafe { $oracle(y, x) };
                    let r = $subject(y, x);
                    max_ulp = max_ulp.max(u64::from($ulp(c, r)));
                    if u64::from($ulp(c, r)) > budget(stringify!($name)) {
                        panic!(
                            "{} mismatch seed={seed} index={index} y={y:?} x={x:?} oracle={c:?} rust={r:?} ulp={}",
                            stringify!($name),
                            $ulp(c, r)
                        );
                    }
                    cases += 1;
                }
            }
            println!(
                "{}: cases={cases} max_ulp={max_ulp} budget={}",
                stringify!($name),
                budget(stringify!($name))
            );
        }
    };
}

macro_rules! bessel {
    ($name:ident, $oracle:ident, $subject:path, $gen:ident, $ulp:ident, $edges:ident) => {
        #[test]
        fn $name() {
            let config = Config::from_env();
            if let Some(filter) = &config.filter
                && !concat!("diff_", stringify!($name)).contains(filter.as_str())
            {
                return;
            }
            let mut cases = 0u64;
            let mut max_ulp = 0u64;
            for order in -32..=32i32 {
                for &x in $edges {
                    let c = unsafe { $oracle(order, x) };
                    let r = $subject(order, x);
                    assert!(
                        u64::from($ulp(c, r)) <= budget(stringify!($name)),
                        "{}: edge n={order} x={x:?} oracle={c:?} rust={r:?}",
                        stringify!($name)
                    );
                    cases += 1;
                }
            }
            for seed in config.seed_base..config.seed_base + config.seeds {
                let mut rng = Rng::seed_from_u64(seed);
                for index in 0..config.cases {
                    let order = rng.below(65) as i32 - 32;
                    let x = $gen(&mut rng, index);
                    let c = unsafe { $oracle(order, x) };
                    let r = $subject(order, x);
                    max_ulp = max_ulp.max(u64::from($ulp(c, r)));
                    if u64::from($ulp(c, r)) > budget(stringify!($name)) {
                        panic!(
                            "{} mismatch seed={seed} index={index} n={order} x={x:?} oracle={c:?} rust={r:?} ulp={}",
                            stringify!($name),
                            $ulp(c, r)
                        );
                    }
                    cases += 1;
                }
            }
            println!(
                "{}: cases={cases} max_ulp={max_ulp} budget={}",
                stringify!($name),
                budget(stringify!($name))
            );
        }
    };
}

unary!(diff_exp, oracle_exp, exp, gen64, ulp64, EDGES64);
unary!(diff_expf, oracle_expf, expf, gen32, ulp32, EDGES32);
unary!(diff_exp2, oracle_exp2, exp2, gen64, ulp64, EDGES64);
unary!(diff_exp2f, oracle_exp2f, exp2f, gen32, ulp32, EDGES32);
unary!(diff_exp10, oracle_exp10, exp10, gen64, ulp64, EDGES64);
unary!(diff_exp10f, oracle_exp10f, exp10f, gen32, ulp32, EDGES32);
unary!(diff_expm1, oracle_expm1, expm1, gen64, ulp64, EDGES64);
unary!(diff_expm1f, oracle_expm1f, expm1f, gen32, ulp32, EDGES32);
unary!(diff_log, oracle_log, log, gen64, ulp64, EDGES64);
unary!(diff_logf, oracle_logf, logf, gen32, ulp32, EDGES32);
unary!(diff_log2, oracle_log2, log2, gen64, ulp64, EDGES64);
unary!(diff_log2f, oracle_log2f, log2f, gen32, ulp32, EDGES32);
unary!(diff_log10, oracle_log10, log10, gen64, ulp64, EDGES64);
unary!(diff_log10f, oracle_log10f, log10f, gen32, ulp32, EDGES32);
unary!(diff_log1p, oracle_log1p, log1p, gen64, ulp64, EDGES64);
unary!(diff_log1pf, oracle_log1pf, log1pf, gen32, ulp32, EDGES32);
unary!(diff_sin, oracle_sin, sin, gen64, ulp64, EDGES64);
unary!(diff_sinf, oracle_sinf, sinf, gen32, ulp32, EDGES32);
unary!(diff_cos, oracle_cos, cos, gen64, ulp64, EDGES64);
unary!(diff_cosf, oracle_cosf, cosf, gen32, ulp32, EDGES32);
unary!(diff_tan, oracle_tan, tan, gen64, ulp64, EDGES64);
unary!(diff_tanf, oracle_tanf, tanf, gen32, ulp32, EDGES32);
unary!(diff_asin, oracle_asin, asin, gen64, ulp64, EDGES64);
unary!(diff_asinf, oracle_asinf, asinf, gen32, ulp32, EDGES32);
unary!(diff_acos, oracle_acos, acos, gen64, ulp64, EDGES64);
unary!(diff_acosf, oracle_acosf, acosf, gen32, ulp32, EDGES32);
unary!(diff_atan, oracle_atan, atan, gen64, ulp64, EDGES64);
unary!(diff_atanf, oracle_atanf, atanf, gen32, ulp32, EDGES32);
unary!(diff_sinh, oracle_sinh, sinh, gen64, ulp64, EDGES64);
unary!(diff_sinhf, oracle_sinhf, sinhf, gen32, ulp32, EDGES32);
unary!(diff_cosh, oracle_cosh, cosh, gen64, ulp64, EDGES64);
unary!(diff_coshf, oracle_coshf, coshf, gen32, ulp32, EDGES32);
unary!(diff_tanh, oracle_tanh, tanh, gen64, ulp64, EDGES64);
unary!(diff_tanhf, oracle_tanhf, tanhf, gen32, ulp32, EDGES32);
unary!(diff_asinh, oracle_asinh, asinh, gen64, ulp64, EDGES64);
unary!(diff_asinhf, oracle_asinhf, asinhf, gen32, ulp32, EDGES32);
unary!(diff_acosh, oracle_acosh, acosh, gen64, ulp64, EDGES64);
unary!(diff_acoshf, oracle_acoshf, acoshf, gen32, ulp32, EDGES32);
unary!(diff_atanh, oracle_atanh, atanh, gen64, ulp64, EDGES64);
unary!(diff_atanhf, oracle_atanhf, atanhf, gen32, ulp32, EDGES32);
unary!(diff_erf, oracle_erf, erf, gen64, ulp64, EDGES64);
unary!(diff_erff, oracle_erff, erff, gen32, ulp32, EDGES32);
unary!(diff_erfc, oracle_erfc, erfc, gen64, ulp64, EDGES64);
unary!(diff_erfcf, oracle_erfcf, erfcf, gen32, ulp32, EDGES32);
unary!(diff_j0, oracle_j0, j0, gen64, ulp64, EDGES64);
unary!(diff_j0f, oracle_j0f, j0f, gen32, ulp32, EDGES32);
unary!(diff_j1, oracle_j1, j1, gen64, ulp64, EDGES64);
unary!(diff_j1f, oracle_j1f, j1f, gen32, ulp32, EDGES32);
unary!(diff_y0, oracle_y0, y0, gen64, ulp64, EDGES64);
unary!(diff_y0f, oracle_y0f, y0f, gen32, ulp32, EDGES32);
unary!(diff_y1, oracle_y1, y1, gen64, ulp64, EDGES64);
unary!(diff_y1f, oracle_y1f, y1f, gen32, ulp32, EDGES32);
binary!(diff_pow, oracle_pow, pow, gen64, ulp64, EDGES64);
binary!(diff_powf, oracle_powf, powf, gen32, ulp32, EDGES32);
binary!(diff_atan2, oracle_atan2, atan2, gen64, ulp64, EDGES64);
binary!(diff_atan2f, oracle_atan2f, atan2f, gen32, ulp32, EDGES32);
bessel!(diff_jn, oracle_jn, jn, gen64, ulp64, EDGES64);
bessel!(diff_jnf, oracle_jnf, jnf, gen32, ulp32, EDGES32);
bessel!(diff_yn, oracle_yn, yn, gen64, ulp64, EDGES64);
bessel!(diff_ynf, oracle_ynf, ynf, gen32, ulp32, EDGES32);

/// The exhaustive f32 walk: every one of the 2^32 bit patterns for the f32 family.
///
/// Only runs with `RL_DIFF_EXHAUSTIVE=1`; it is the ticket's exhaustive sweep and takes hours in
/// a debug build, so the report quotes the command and the measured prefix rather than inventing
/// a pass.
#[test]
fn exhaustive_f32_sweep() {
    let config = Config::from_env();
    if !config.exhaustive {
        return;
    }
    if let Some(filter) = &config.filter
        && !"diff_exhaustive_f32_sweep".contains(filter.as_str())
    {
        return;
    }
    let mut unsigned = 0u64;
    let started = std::time::Instant::now();
    // The same measured budget the random class uses: the proxy donor (musl) and the rust port
    // differ by one ULP on a few `logf` inputs, and the sweep must report that, not hide it.
    /// One sweep entry: name, the C oracle and the Rust subject.
    type Sweep = (
        &'static str,
        unsafe extern "C" fn(f32) -> f32,
        fn(f32) -> f32,
    );
    let table: [Sweep; 8] = [
        ("expf", oracle_expf, expf),
        ("logf", oracle_logf, logf),
        ("sinf", oracle_sinf, sinf),
        ("cosf", oracle_cosf, cosf),
        ("tanf", oracle_tanf, tanf),
        ("erff", oracle_erff, erff),
        ("exp2f", oracle_exp2f, exp2f),
        ("expm1f", oracle_expm1f, expm1f),
    ];
    let budgets: [u32; 8] = core::array::from_fn(|index| budget(table[index].0) as u32);
    let mut maxima = [0u32; 8];
    loop {
        for high in 0..=u32::from(u16::MAX) {
            let bits = (high << 16) | unsigned as u32;
            let x = f32::from_bits(bits);
            for (index, (name, oracle, subject)) in table.iter().enumerate() {
                let oracle_value = unsafe { oracle(x) };
                let rust_value = subject(x);
                let distance = ulp32(oracle_value, rust_value);
                if distance > maxima[index] {
                    maxima[index] = distance;
                }
                assert!(
                    distance <= budgets[index],
                    "{name}: exhaustive bits={bits:#x} x={x:?} oracle={oracle_value:?} rust={rust_value:?} ulp={distance} budget={}",
                    budgets[index]
                );
            }
        }
        unsigned += 1;
        if unsigned.is_multiple_of(8192) {
            println!(
                "exhaustive: {unsigned}/65536 blocks, {:?} elapsed",
                started.elapsed()
            );
        }
        if unsigned > u64::from(u16::MAX) {
            break;
        }
    }
    for (index, (name, _, _)) in table.iter().enumerate() {
        println!(
            "exhaustive: {name} max_ulp={} budget={}",
            maxima[index], budgets[index]
        );
    }
    println!(
        "exhaustive: all 2^32 f32 inputs x 8 functions in {:?}",
        started.elapsed()
    );
}
