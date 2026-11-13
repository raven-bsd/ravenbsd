// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/__lock.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! The internal lock (`__lock`/`__unlock`).
//!
//! One `int` carries the sign-bit lock flag and the congestion count: `0` is unlocked and empty,
//! `x > 0` is unlocked with congestion `x`, `x < 0` is locked with congestion `x - INT_MIN`.
//! The state machine is kept exactly as the donor's: a ten-iteration spin, then a congestion
//! increment and park; `unlock` wakes only when the congestion was greater than one. When the
//! process is single-threaded (`need_locks == 0`) the lock is a no-op.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::kernel::ThreadKernel;
use crate::libc_state::LIBC;
use crate::raw::at_i32;
use crate::sync::wait::wait;
use crate::sync::wait::wait_forever;

const INT_MIN: i32 = i32::MIN;
const INT_MIN_PLUS_ONE: i32 = INT_MIN + 1;
const CONGESTION_STEP: i32 = -(INT_MIN + 1);

/// Acquire an internal lock.
pub fn lock<K: ThreadKernel>(kernel: &K, word: &AtomicI32) {
    let need_locks = LIBC.need_locks();
    if need_locks == 0 {
        return;
    }
    let mut current = word
        .compare_exchange(0, INT_MIN_PLUS_ONE, Ordering::AcqRel, Ordering::Acquire)
        .unwrap_or_else(|value| value);
    if need_locks < 0 {
        LIBC.set_need_locks(0);
    }
    if current == 0 {
        return;
    }
    let mut spins = 10u32;
    while spins > 0 {
        if current < 0 {
            current = current.wrapping_sub(INT_MIN_PLUS_ONE);
        }
        let wanted = INT_MIN.wrapping_add(current.wrapping_add(1));
        match word.compare_exchange(current, wanted, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return,
            Err(value) => current = value,
        }
        spins -= 1;
    }
    current = word.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
    loop {
        if current < 0 {
            wait_forever(kernel, word, current);
            current = current.wrapping_sub(INT_MIN_PLUS_ONE);
        }
        let wanted = INT_MIN.wrapping_add(current);
        match word.compare_exchange(current, wanted, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return,
            Err(value) => current = value,
        }
    }
}

/// Release an internal lock.
pub fn unlock<K: ThreadKernel>(kernel: &K, word: &AtomicI32) {
    if word.load(Ordering::Acquire) < 0 {
        let previous = word.fetch_add(CONGESTION_STEP, Ordering::AcqRel);
        if previous != INT_MIN_PLUS_ONE {
            kernel.futex_wake(crate::raw::as_u32(word), 1);
        }
    }
}

/// Whether the process is multi-threaded and internal locks must be taken.
#[must_use]
pub fn threaded() -> bool {
    LIBC.need_locks() != 0
}

/// Lock a raw word slice at `index`.
pub fn lock_word<K: ThreadKernel>(kernel: &K, words: &[u32], index: usize) {
    lock(kernel, at_i32(words, index));
}

/// Unlock a raw word slice at `index`.
pub fn unlock_word<K: ThreadKernel>(kernel: &K, words: &[u32], index: usize) {
    unlock(kernel, at_i32(words, index));
}

/// Park on `word` while it equals `value` (`__wait` in the donor).
pub fn wait_on<K: ThreadKernel>(kernel: &K, word: &AtomicI32, value: i32) {
    wait(kernel, word, None, value, true);
}

#[cfg(test)]
#[path = "lock_ut.rs"]
mod tests;
