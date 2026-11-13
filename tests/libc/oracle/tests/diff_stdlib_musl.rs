// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `stdlib-musl`: the rand48 family against musl.

#![cfg(feature = "oracle-stdlib-musl")]
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects
)]
#![allow(clippy::undocumented_unsafe_blocks)] // oracle calls follow the C prototypes; test buffers outlive each call

use core::ffi::{c_long, c_ushort};

use raven_libc_oracle::decls::stdlib_musl::{
    oracle_drand48, oracle_jrand48, oracle_lrand48, oracle_mrand48, oracle_nrand48, oracle_seed48,
    oracle_srand48,
};
use raven_libc_oracle::diff::Case;
use raven_libc_oracle::diff_case;
use raven_libc_oracle::rng::Rng;
use std::sync::Mutex;

use raven_libc_stdlib::rand::{
    Rand48, drand48, jrand48, lrand48, mrand48, nrand48, seed48, srand48, x_to_double,
};

/// The musl oracle keeps its state in process globals; the two tests must not interleave.
static C_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Rand48Case {
    seed: u32,
    count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Rand48Obs {
    l: Vec<i64>,
    m: Vec<i64>,
    d: Vec<u64>,
}

impl Case for Rand48Case {
    type Obs = Rand48Obs;

    fn shrink(&self) -> Vec<Self> {
        if self.count <= 1 {
            return Vec::new();
        }
        vec![Self {
            count: self.count / 2,
            ..self.clone()
        }]
    }

    fn to_toml(&self) -> String {
        format!("seed = {}\ncount = {}\n", self.seed, self.count)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("l = {:?}\nm = {:?}\nd = {:?}\n", obs.l, obs.m, obs.d)
    }
}

fn oracle_obs(case: &Rand48Case) -> Rand48Obs {
    let _guard = C_LOCK.lock().unwrap();
    unsafe { oracle_srand48(case.seed as c_long) };
    let mut l = Vec::new();
    let mut m = Vec::new();
    let mut d = Vec::new();
    for _ in 0..case.count {
        l.push(unsafe { oracle_lrand48() } as i64);
        m.push(unsafe { oracle_mrand48() } as i64);
        d.push(unsafe { oracle_drand48() }.to_bits());
    }
    Rand48Obs { l, m, d }
}

fn subject_obs(case: &Rand48Case) -> Rand48Obs {
    let mut generator = Rand48::new();
    srand48(&mut generator, case.seed as i32);
    let mut l = Vec::new();
    let mut m = Vec::new();
    let mut d = Vec::new();
    for _ in 0..case.count {
        l.push(lrand48(&mut generator));
        m.push(mrand48(&mut generator));
        d.push(drand48(&mut generator).to_bits());
    }
    Rand48Obs { l, m, d }
}

diff_case! {
    name: rand48,
    case: Rand48Case,
    fixed: "stdlib/rand48/basic.toml",
    random: |rng: &mut Rng| Rand48Case {
        seed: rng.next_u64() as u32,
        count: 16,
    },
    exhaustive: (small, || {
        Box::new((0u32..(1 << 20)).map(|seed| Rand48Case { seed, count: 16 }))
    }),
    oracle: oracle_obs,
    subject: subject_obs,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Rand48CallerCase {
    seed: u16,
    count: usize,
}

impl Case for Rand48CallerCase {
    type Obs = (Vec<u16>, Vec<i64>, Vec<i64>);

    fn to_toml(&self) -> String {
        format!("seed = {}\ncount = {}\n", self.seed, self.count)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("previous = {:?}\nn = {:?}\nj = {:?}\n", obs.0, obs.1, obs.2)
    }
}

fn caller_oracle(case: &Rand48CallerCase) -> (Vec<u16>, Vec<i64>, Vec<i64>) {
    let _guard = C_LOCK.lock().unwrap();
    unsafe { oracle_srand48(0) };
    let mut words = [0x330e as c_ushort, case.seed, 0x1234];
    let previous = unsafe { oracle_seed48(words.as_mut_ptr()) };
    let previous = unsafe { core::slice::from_raw_parts(previous, 3).to_vec() };
    let mut n = Vec::new();
    let mut j = Vec::new();
    for _ in 0..case.count {
        n.push(unsafe { oracle_nrand48(words.as_mut_ptr()) } as i64);
        j.push(unsafe { oracle_jrand48(words.as_mut_ptr()) } as i64);
    }
    (previous, n, j)
}

fn caller_subject(case: &Rand48CallerCase) -> (Vec<u16>, Vec<i64>, Vec<i64>) {
    let mut generator = Rand48::new();
    srand48(&mut generator, 0);
    let mut words = [0x330e, case.seed, 0x1234];
    let previous = seed48(&mut generator, &words);
    let mut n = Vec::new();
    let mut j = Vec::new();
    for _ in 0..case.count {
        n.push(nrand48(&mut generator, &mut words));
        j.push(jrand48(&mut generator, &mut words));
    }
    (previous.to_vec(), n, j)
}

diff_case! {
    name: rand48_caller_seed,
    case: Rand48CallerCase,
    fixed: "stdlib/rand48_caller/basic.toml",
    random: |rng: &mut Rng| Rand48CallerCase {
        seed: rng.next_u64() as u16,
        count: 16,
    },
    exhaustive: (small, || {
        Box::new((0u16..=255).map(|seed| Rand48CallerCase { seed, count: 16 }).collect::<Vec<_>>().into_iter())
    }),
    oracle: caller_oracle,
    subject: caller_subject,
}

#[test]
fn drand48_bit_conversion_is_exact() {
    for value in 0u64..1000 {
        let bits = x_to_double(value).to_bits();
        let expected = ((value as f64) / 2f64.powi(48)).to_bits();
        assert_eq!(bits, expected);
    }
}
