// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic chi-square checks for `arc4random_uniform` and the byte stream.

use std::cell::Cell;

use raven_libc_malloc::Entropy;
use raven_libc_rand::{Arc4Random, RandPlatform};

struct Fixed {
    state: Cell<u64>,
}

impl Fixed {
    fn new(seed: u64) -> Self {
        Self {
            state: Cell::new(seed | 1),
        }
    }
}

impl Entropy for Fixed {
    fn fill(&self, buf: &mut [u8]) -> bool {
        let mut value = self.state.get();
        for byte in buf {
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            *byte = value as u8;
        }
        self.state.set(value);
        true
    }
}

struct Fatal;

impl RandPlatform for Fatal {
    fn no_entropy(&self) -> ! {
        panic!("no entropy")
    }
}

fn critical(df: usize) -> f64 {
    // Wilson-Hilferty approximation of the 0.001 upper critical value.
    let df = df as f64;
    df * (1.0 - 2.0 / (9.0 * df) + 3.0902 * (2.0 / (9.0 * df)).sqrt()).powi(3)
}

fn chi_square(counts: &[u64], total: u64) -> f64 {
    let expected = total as f64 / counts.len() as f64;
    counts
        .iter()
        .map(|count| {
            let difference = *count as f64 - expected;
            difference * difference / expected
        })
        .sum()
}

#[test]
fn uniform_chi_square_for_fixed_seeds() {
    for seed in [1u64, 2, 3] {
        for bound in [2u32, 3, 10, 100, 1000] {
            let entropy = Fixed::new(seed);
            let mut random = Arc4Random::new();
            let mut counts = std::vec![0u64; bound as usize];
            let total = 1_000_000u64;
            for _ in 0..total {
                let value = random.uniform(&entropy, &Fatal, bound);
                counts[value as usize] += 1;
            }
            let chi = chi_square(&counts, total);
            let limit = critical(counts.len() - 1);
            assert!(
                chi < limit,
                "seed {seed} bound {bound} chi {chi} crit {limit} counts {counts:?}"
            );
        }
    }
}

#[test]
fn byte_distribution_and_bit_balance() {
    for seed in [1u64, 2, 3] {
        let entropy = Fixed::new(seed);
        let mut random = Arc4Random::new();
        let mut bytes = std::vec![0u8; 1_000_000];
        random.random_buf(&entropy, &Fatal, &mut bytes);
        let mut counts = [0u64; 256];
        let mut ones = [0u64; 8];
        for byte in &bytes {
            counts[*byte as usize] += 1;
            for (bit, count) in ones.iter_mut().enumerate() {
                *count += u64::from((byte >> bit) & 1);
            }
        }
        let chi = chi_square(&counts, bytes.len() as u64);
        let limit = critical(255);
        assert!(chi < limit, "seed {seed} byte chi {chi} crit {limit}");
        for (bit, count) in ones.iter().enumerate() {
            let frequency = *count as f64 / bytes.len() as f64;
            assert!(
                (frequency - 0.5).abs() <= 0.002,
                "seed {seed} bit {bit} frequency {frequency}"
            );
        }
    }
}
