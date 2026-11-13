// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/internal/pthread_impl.h, src/thread/pthread_mutex_lock.c, src/thread/pthread_mutex_unlock.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor file has no per-file header; covered by musl/COPYRIGHT
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! Atomic views of the integer words inside the public lock wrappers.
//!
//! A `pthread_mutex_t` is `[u32; 10]` in ravenlibc; the caller may hand us any 4-byte-aligned
//! storage, including a statically initialised all-zero object. These helpers turn one word into
//! an `&AtomicU32`/`&AtomicI32` without taking ownership; all other access to the wrapper goes
//! through the same atomic references.

use core::sync::atomic::{AtomicI32, AtomicU32};

/// A shared `&AtomicU32` view of word `index`.
///
/// The word must stay alive and no non-atomic write may race with the returned view. The wrapper
/// types guarantee 4-byte alignment and the index is checked in a debug build.
#[must_use]
pub fn at_u32(words: &[u32], index: usize) -> &AtomicU32 {
    debug_assert!(index < words.len());
    // SAFETY: `words` is 4-byte aligned and the index is inside the slice; `AtomicU32` has the
    // same layout and alignment as `u32`. Shared atomic access is sound while no plain write
    // races (the wrapper API is the only writer).
    unsafe { &*(words.as_ptr().add(index) as *const AtomicU32) }
}

/// A shared `&AtomicI32` view of word `index`.
#[must_use]
pub fn at_i32(words: &[u32], index: usize) -> &AtomicI32 {
    debug_assert!(index < words.len());
    // SAFETY: see [`at_u32`]; `AtomicI32` and `u32` have the same size and alignment.
    unsafe { &*(words.as_ptr().add(index) as *const AtomicI32) }
}

/// Load a `usize` stored in two consecutive words (little-endian, all three targets).
#[must_use]
pub fn load_usize(words: &[u32], index: usize) -> usize {
    let low = u64::from(words.get(index).copied().unwrap_or(0));
    let high = u64::from(words.get(index.wrapping_add(1)).copied().unwrap_or(0));
    (low | (high << 32)) as usize
}

/// Store a `usize` in two consecutive words.
pub fn store_usize(words: &mut [u32], index: usize, value: usize) {
    let bits = value as u64;
    if let Some(slot) = words.get_mut(index) {
        *slot = bits as u32;
    }
    if let Some(slot) = words.get_mut(index.wrapping_add(1)) {
        *slot = (bits >> 32) as u32;
    }
}

#[cfg(test)]
#[path = "futex_ut.rs"]
mod tests;

/// Reinterpret an `&AtomicI32` futex word as `&AtomicU32` (same size, same alignment).
#[must_use]
pub fn as_u32(word: &AtomicI32) -> &AtomicU32 {
    // SAFETY: both atomics are `#[repr(transparent)]` over their integer and have identical size
    // and alignment; the bit pattern is the value, and futex operations are type-agnostic.
    unsafe { &*(word as *const AtomicI32 as *const AtomicU32) }
}
