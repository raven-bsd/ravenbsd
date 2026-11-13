// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The page size, allocation limits and backend layout constants.
//!
//! Every 4096 in the crate comes from [`PAGE_SIZE`]; the startup assertion in each backend checks
//! the platform page size against it (T-24).

/// The only page shift in the crate.
pub const PAGE_SHIFT: usize = 12;
/// The only page size in the crate.
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
/// Allocator alignment guaranteed by both backends.
pub const MALLOC_ALIGN: usize = 16;
/// Largest accepted allocation (`isize::MAX`, doc 08 4.7).
pub const MAX_ALLOC: usize = isize::MAX as usize;

/// Round `value` up to `align`; `None` on overflow or a non-power-of-two `align`.
#[must_use]
pub const fn align_up(value: usize, align: usize) -> Option<usize> {
    if align == 0 || !align.is_power_of_two() {
        return None;
    }
    let mask = align - 1;
    match value.checked_add(mask) {
        Some(sum) => Some(sum & !mask),
        None => None,
    }
}

/// Round `value` down to `align`; `None` for a non-power-of-two `align`.
#[must_use]
pub const fn align_down(value: usize, align: usize) -> Option<usize> {
    if align == 0 || !align.is_power_of_two() {
        return None;
    }
    Some(value & !(align - 1))
}

/// Whether `value` is a multiple of `align` (which must be a power of two).
#[must_use]
pub const fn is_aligned(value: usize, align: usize) -> bool {
    align.is_power_of_two() && value & (align - 1) == 0
}

#[cfg(test)]
#[path = "consts_ut.rs"]
mod tests;
