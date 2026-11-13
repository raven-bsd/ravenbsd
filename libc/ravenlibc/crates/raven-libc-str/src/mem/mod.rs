// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Safe block-memory engines shared by the pointer shell and the C-ABI tests.
//!
//! The loop bodies of `memcpy`, `memmove`, `memset` and `memcmp` are **not** here: they are the
//! compiler-builtins adaptation in `raw::mem` (ticket 5.1), which has to be reachable without a
//! libcall. This module holds the slice-level helpers that the other entry points use.

pub mod memccpy;
pub mod memmem;
pub mod memrchr;

pub use memccpy::ccopy;
pub use memmem::find_sub;
pub use memrchr::rfind;

/// Compare two byte slices; the first differing byte decides, as `unsigned char` (C `memcmp`).
///
/// Only the common prefix is compared. A result of zero therefore means "equal over
/// `min(a.len(), b.len())`", which is exactly what `memcmp(a, b, n)` needs when both slices are
/// `n` bytes long. Tests compare the sign only (the C value is a byte difference, not -1/0/1).
#[must_use]
pub fn compare(a: &[u8], b: &[u8]) -> i32 {
    let len = if a.len() < b.len() { a.len() } else { b.len() };
    let mut index = 0usize;
    while index < len {
        let Some(&left) = a.get(index) else {
            break;
        };
        let Some(&right) = b.get(index) else {
            break;
        };
        if left != right {
            return i32::from(left).wrapping_sub(i32::from(right));
        }
        index = index.wrapping_add(1);
    }
    0
}

/// Index of the first byte of `h` equal to `c`, or `None` (`memchr` over a slice).
#[must_use]
pub fn find(h: &[u8], c: u8) -> Option<usize> {
    let mut index = 0usize;
    while index < h.len() {
        let Some(&byte) = h.get(index) else {
            break;
        };
        if byte == c {
            return Some(index);
        }
        index = index.wrapping_add(1);
    }
    None
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
