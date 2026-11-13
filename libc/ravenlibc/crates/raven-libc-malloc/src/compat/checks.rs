// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Overflow and alignment checks from POSIX and C17 7.22.3.

pub use crate::consts::MAX_ALLOC;

/// `a * b` when it cannot overflow.
#[must_use]
pub const fn mul_overflows(a: usize, b: usize) -> Option<usize> {
    a.checked_mul(b)
}

/// The OpenBSD `reallocarray` half-word threshold: `sqrt(SIZE_MAX + 1)`.
#[must_use]
pub const fn array_threshold() -> usize {
    1usize << (usize::BITS / 2)
}

/// The `reallocarray` check, kept in the donor's condition order.
#[must_use]
pub fn array_size(n: usize, m: usize) -> Option<usize> {
    let threshold = array_threshold();
    if (n >= threshold || m >= threshold) && n != 0 && m > usize::MAX / n {
        None
    } else {
        n.checked_mul(m)
    }
}

/// An allocator alignment: a power of two, at least one.
#[must_use]
pub const fn align_ok(align: usize) -> bool {
    align != 0 && align.is_power_of_two()
}

/// A `posix_memalign` alignment: a power of two and a multiple of `size_of::<usize>()`.
#[must_use]
pub const fn posix_align_ok(align: usize) -> bool {
    align_ok(align) && align.is_multiple_of(core::mem::size_of::<usize>())
}

/// `alignment > usize::MAX - size` would wrap (T-25).
#[must_use]
pub const fn alignment_overflows(alignment: usize, size: usize) -> bool {
    alignment > usize::MAX - size
}

#[cfg(test)]
#[path = "checks_ut.rs"]
mod tests;
