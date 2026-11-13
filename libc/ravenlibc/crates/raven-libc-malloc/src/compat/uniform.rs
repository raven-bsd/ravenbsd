// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Uniform random values in `[0, n)`, written from the definition (not from `arc4random.c`).

use crate::provider::Entropy;

/// Return a uniformly distributed value below `n`, or `None` when `n == 0` or entropy fails.
///
/// Values at or above `2^32 - (2^32 mod n)` are discarded, which removes the modulo bias.
pub fn uniform<E: Entropy>(entropy: &E, n: u32) -> Option<u32> {
    if n == 0 {
        return None;
    }
    if n == 1 {
        return Some(0);
    }
    let limit = u32::MAX - (u32::MAX % n);
    let mut attempts = 0usize;
    while attempts < 64 {
        let mut bytes = [0u8; 4];
        if !entropy.fill(&mut bytes) {
            return None;
        }
        let value = u32::from_le_bytes(bytes);
        if value < limit {
            return Some(value % n);
        }
        attempts = attempts.wrapping_add(1);
    }
    None
}

#[cfg(test)]
#[path = "uniform_ut.rs"]
mod tests;
