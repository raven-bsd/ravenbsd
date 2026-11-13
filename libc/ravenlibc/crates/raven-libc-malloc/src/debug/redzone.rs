// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Redzone fill and check helpers.

use crate::debug::consts::{REDZONE, REDZONE_WORD};
use crate::raw;

/// Fill the 16 bytes before and after `pointer`.
///
/// The caller must pass a live debug allocation with `REDZONE` bytes on both sides.
pub fn fill(pointer: usize, size: usize) {
    let mut index = 0usize;
    while index < REDZONE {
        raw::write_u64(
            pointer.wrapping_sub(REDZONE).wrapping_add(index),
            REDZONE_WORD,
        );
        raw::write_u64(pointer.wrapping_add(size).wrapping_add(index), REDZONE_WORD);
        index = index.wrapping_add(8);
    }
}

/// Check both redzones; returns `(side, offset)` on the first mismatch.
///
/// The caller must pass a live debug allocation with `REDZONE` bytes on both sides.
#[must_use]
pub fn check(pointer: usize, size: usize) -> Option<(&'static str, usize)> {
    let mut index = 0usize;
    while index < REDZONE {
        let before = raw::read_u64(pointer.wrapping_sub(REDZONE).wrapping_add(index));
        if before != REDZONE_WORD {
            return Some(("underrun", index));
        }
        index = index.wrapping_add(8);
    }
    let mut index = 0usize;
    while index < REDZONE {
        let after = raw::read_u64(pointer.wrapping_add(size).wrapping_add(index));
        if after != REDZONE_WORD {
            return Some(("overrun", index));
        }
        index = index.wrapping_add(8);
    }
    None
}
