// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/block.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! The mask words of A-sig-1: blocking is a user-space atomic store, never a syscall.
//!
//! musl's `__block_all_sigs`/`__block_app_sigs`/`__restore_sigs` are `rt_sigprocmask` calls
//! (`block.c:31-44`); here the three functions are two atomic operations on
//! `Pthread.sigmask` (bit `n-1` is signal `n`). Rule S2 uses `block_app` around internal locks
//! and `restore` afterwards; there is no kernel mask.

use crate::SigKernel;
use crate::raw;
use crate::sigset::{ALL_MASK, APP_MASK, SigSet};

/// Block every signal, including the internal 32..34; returns the previous mask.
pub fn block_all<K: SigKernel>(kernel: &K) -> u64 {
    raw::mask_fetch_or(kernel.thread_pointer(), ALL_MASK)
}

/// Block every application signal; returns the previous mask.
pub fn block_app<K: SigKernel>(kernel: &K) -> u64 {
    raw::mask_fetch_or(kernel.thread_pointer(), APP_MASK)
}

/// Restore a mask returned by [`block_all`] or [`block_app`].
pub fn restore<K: SigKernel>(kernel: &K, old: u64) {
    raw::mask_store(kernel.thread_pointer(), old);
}

/// The live mask word.
#[must_use]
pub fn current_mask<K: SigKernel>(kernel: &K) -> u64 {
    raw::mask_load(kernel.thread_pointer())
}

/// Replace the live mask word.
pub fn set_mask<K: SigKernel>(kernel: &K, word: u64) {
    raw::mask_store(kernel.thread_pointer(), word & APP_MASK);
}

/// Swap the live mask word, returning the old one.
pub fn swap_mask<K: SigKernel>(kernel: &K, word: u64) -> u64 {
    raw::mask_swap(kernel.thread_pointer(), word & APP_MASK)
}

/// The mask a handler runs with: `old | sa_mask | (sig unless SA_NODEFER)`, internal bits clear.
#[must_use]
pub fn handler_mask(old: u64, sa_mask: u64, sig: i32, flags: u32) -> u64 {
    let mut mask = old | sa_mask;
    if flags & crate::dispositions::SA_NODEFER == 0 && (1..=64).contains(&sig) {
        mask |= crate::critical::signal_bit(sig);
    }
    mask & APP_MASK
}

/// The bits of `pending` that are deliverable under `mask`.
#[must_use]
pub fn deliverable(pending: u64, mask: u64) -> u64 {
    pending & !mask & APP_MASK
}

/// Whether `sig` is blocked by `mask`.
#[must_use]
pub fn is_blocked(mask: u64, sig: i32) -> bool {
    if !(1..=64).contains(&sig) {
        return false;
    }
    let bit = crate::critical::signal_bit(sig);
    mask & bit != 0
}

/// A `SigSet` view of a mask word.
#[must_use]
pub fn set_from_word(word: u64) -> SigSet {
    let mut set = SigSet::empty();
    if let Some(slot) = set.0.first_mut() {
        *slot = word & APP_MASK;
    }
    set
}

#[cfg(test)]
#[path = "mask_ut.rs"]
mod mask_ut;
