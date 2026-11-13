// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `stdlib-netbsd`: the `random` family and `rand_r`.

#![cfg(feature = "oracle-stdlib-netbsd")]
#![allow(clippy::indexing_slicing, clippy::unwrap_used)]
#![allow(clippy::undocumented_unsafe_blocks)] // oracle calls follow the C prototypes; test buffers outlive each call
use core::ffi::{c_char, c_uint};

use raven_libc_oracle::decls::stdlib_netbsd::{
    oracle_initstate, oracle_rand_r, oracle_random, oracle_srandom,
};
use raven_libc_oracle::diff::Case;
use raven_libc_oracle::diff_case;
use raven_libc_oracle::rng::Rng;
use raven_libc_stdlib::rand::{RandomCtl, initstate, rand_r as next_rand_r, random as next_random};

fn sizes() -> [usize; 6] {
    [8, 32, 64, 128, 256, 1024]
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RandomCase {
    seed: u32,
    size: usize,
    count: usize,
}

impl Case for RandomCase {
    type Obs = Vec<u32>;

    fn shrink(&self) -> Vec<Self> {
        if self.count <= 1 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for count in [self.count / 2, 1] {
            if count < self.count {
                out.push(Self {
                    count,
                    ..self.clone()
                });
            }
        }
        out
    }

    fn to_toml(&self) -> String {
        format!(
            "seed = {}\nsize = {}\ncount = {}\n",
            self.seed, self.size, self.count
        )
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        let values: Vec<String> = obs.iter().map(|value| format!("{value:08x}")).collect();
        format!("values = [{}]", values.join(", "))
    }
}

thread_local! {
    /// `initstate` writes the old state word into the previous buffer before switching, so the
    /// previous buffer must outlive the next `initstate` call.
    static KEEP_ALIVE: std::cell::RefCell<Option<Vec<u8>>> = const { std::cell::RefCell::new(None) };
}

fn random_oracle(case: &RandomCase) -> Vec<u32> {
    let previous = KEEP_ALIVE.with(|cell| cell.borrow_mut().take());
    let mut buffer = vec![0u8; case.size];
    // Deviation D-1: the donor has no seed-zero rule; the subject maps seed 0 to 1.
    let oracle_seed = if case.seed == 0 { 1 } else { case.seed };
    unsafe {
        oracle_initstate(
            oracle_seed as c_uint,
            buffer.as_mut_ptr() as *mut c_char,
            case.size,
        );
    }
    KEEP_ALIVE.with(|cell| *cell.borrow_mut() = Some(buffer));
    drop(previous);
    unsafe {
        oracle_srandom(oracle_seed as c_uint);
        (0..case.count).map(|_| oracle_random() as u32).collect()
    }
}

fn random_subject(case: &RandomCase) -> Vec<u32> {
    let mut ctl = RandomCtl::new();
    let mut state = vec![0u32; case.size / 4];
    initstate(&mut ctl, &mut state, case.seed, case.size).unwrap();
    (0..case.count)
        .map(|_| next_random(&mut ctl, &mut state))
        .collect()
}

diff_case! {
    name: random,
    case: RandomCase,
    fixed: "stdlib/random/basic.toml",
    random: |rng: &mut Rng| RandomCase {
        seed: rng.next_u64() as u32,
        size: rng.pick(&sizes()),
        count: 1000,
    },
    exhaustive: (small, || {
        let mut cases = Vec::new();
        for seed in [0u32, 1, 2, 42, u32::MAX] {
            for size in sizes() {
                cases.push(RandomCase { seed, size, count: 1000 });
            }
        }
        Box::new(cases.into_iter())
    }),
    oracle: random_oracle,
    subject: random_subject,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RandRCase {
    seed: u32,
    count: usize,
}

impl Case for RandRCase {
    type Obs = Vec<u32>;

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
        let values: Vec<String> = obs.iter().map(|value| format!("{value:08x}")).collect();
        format!("values = [{}]", values.join(", "))
    }
}

fn rand_r_oracle(case: &RandRCase) -> Vec<u32> {
    let mut seed = case.seed as c_uint;
    let mut out = Vec::new();
    for _ in 0..case.count {
        out.push(unsafe { oracle_rand_r(&mut seed) } as u32);
    }
    out
}

fn rand_r_subject(case: &RandRCase) -> Vec<u32> {
    let mut seed = case.seed;
    let mut out = Vec::new();
    for _ in 0..case.count {
        out.push(next_rand_r(&mut seed));
    }
    out
}

diff_case! {
    name: rand_r,
    case: RandRCase,
    fixed: "stdlib/rand_r/basic.toml",
    random: |rng: &mut Rng| RandRCase {
        seed: rng.next_u64() as u32,
        count: 1000,
    },
    exhaustive: (small, || {
        let mut cases = Vec::new();
        for seed in [0u32, 1, 2, 42, u32::MAX] {
            cases.push(RandRCase { seed, count: 1000 });
        }
        Box::new(cases.into_iter())
    }),
    oracle: rand_r_oracle,
    subject: rand_r_subject,
}
