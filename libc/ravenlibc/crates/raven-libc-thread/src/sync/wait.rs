// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/__wait.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `__wait`: a 100-iteration spin and then a futex park, with an optional waiter count.
//!
//! The waiter counter is incremented before the first futex call and decremented after the word
//! changes, so a signaller can decide whether a wake is needed.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::kernel::ThreadKernel;

/// Park while `word == value`, optionally counting waiters in `waiters`.
pub fn wait<K: ThreadKernel>(
    kernel: &K,
    word: &AtomicI32,
    waiters: Option<&AtomicI32>,
    value: i32,
    _private: bool,
) {
    let mut spins = 100u32;
    while spins > 0 {
        if waiters.is_some_and(|count| count.load(Ordering::Acquire) != 0) {
            break;
        }
        if word.load(Ordering::Acquire) != value {
            return;
        }
        core::hint::spin_loop();
        spins -= 1;
    }
    if let Some(count) = waiters {
        count.fetch_add(1, Ordering::AcqRel);
    }
    while word.load(Ordering::Acquire) == value {
        // A spurious `EINTR` is ignored: the caller re-tests the word, exactly as `__wait` does.
        let _ = kernel.futex_wait(crate::raw::as_u32(word), value as u32, None);
    }
    if let Some(count) = waiters {
        count.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Park forever on `word` while it equals `value`.
pub fn wait_forever<K: ThreadKernel>(kernel: &K, word: &AtomicI32, value: i32) {
    while word.load(Ordering::Acquire) == value {
        let _ = kernel.futex_wait(crate::raw::as_u32(word), value as u32, None);
    }
}

/// Wake up to `n` waiters on `word`.
pub fn wake<K: ThreadKernel>(kernel: &K, word: &AtomicI32, n: u32) -> u32 {
    kernel.futex_wake(crate::raw::as_u32(word), n)
}

#[cfg(test)]
#[path = "wait_ut.rs"]
mod tests;
