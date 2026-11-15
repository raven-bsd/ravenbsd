// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Half-open physical address windows.

use core::fmt;

/// A half-open physical address window `base .. base + size`.
///
/// The fields are private because [`Region::new`] enforces the two invariants the rest of the
/// crate relies on: the length is non-zero and the one-past-the-end address does not overflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    base: u64,
    size: u64,
}

impl Region {
    /// Build a window, rejecting an empty or overflowing one.
    #[must_use]
    pub const fn new(base: u64, size: u64) -> Option<Self> {
        if size == 0 || base.checked_add(size).is_none() {
            return None;
        }
        Some(Self { base, size })
    }

    /// First address in the window.
    #[must_use]
    pub const fn base(self) -> u64 {
        self.base
    }

    /// Window length in bytes; never zero.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }

    /// One-past-the-end address.
    ///
    /// [`Region::new`] rejects overflow, so the addition cannot wrap.
    #[must_use]
    pub const fn end(self) -> u64 {
        self.base + self.size
    }

    /// True when `address` is inside the window.
    #[must_use]
    pub const fn contains(self, address: u64) -> bool {
        address >= self.base && address - self.base < self.size
    }

    /// True when `other` lies completely inside this window.
    #[must_use]
    pub const fn contains_region(self, other: Self) -> bool {
        other.base >= self.base && other.end() <= self.end()
    }

    /// Offset of `address` from the window base, or `None` when it is outside.
    #[must_use]
    pub const fn offset_of(self, address: u64) -> Option<u64> {
        if address >= self.base && address - self.base < self.size {
            Some(address - self.base)
        } else {
            None
        }
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}+{:#x}", self.base, self.size)
    }
}

#[cfg(test)]
#[path = "region_ut.rs"]
mod tests;
