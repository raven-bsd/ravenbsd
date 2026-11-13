// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_sigmask.c, src/thread/pthread_create.c (__block_app_sigs/__restore_sigs) @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! The internal signal-mask helpers: `block_app_sigs` and `restore_sigs` are atomic stores on
//! the TCB mask word (A-sig-1).
//!
//! The public `pthread_sigmask` (the `EINVAL` checks and the self-kick when a signal becomes
//! deliverable) belongs to C58. Bits for signals 32, 33 and 34 are libc-internal and are never
//! set by these helpers (A-sig-2: `SIGTIMER`, `SIGCANCEL`, `SIGSYNCCALL`).

use core::sync::atomic::Ordering;

use crate::kernel::ThreadKernel;

/// The zero-based bits of the three libc-internal signals (32, 33, 34).
pub const INTERNAL_SIGNAL_BITS: u64 = 0b111 << 31;

/// `__block_app_sigs`: block every application signal and return the previous word.
pub fn block_app_sigs<K: ThreadKernel>(kernel: &K) -> u64 {
    let pthread = crate::thread::current_pthread(kernel);
    let previous = pthread.sigmask.load(Ordering::Acquire);
    pthread
        .sigmask
        .store(!INTERNAL_SIGNAL_BITS, Ordering::Release);
    previous
}

/// `__block_all_sigs`: block application and internal signals.
pub fn block_all_sigs<K: ThreadKernel>(kernel: &K) -> u64 {
    let pthread = crate::thread::current_pthread(kernel);
    let previous = pthread.sigmask.load(Ordering::Acquire);
    pthread.sigmask.store(u64::MAX, Ordering::Release);
    previous
}

/// `__restore_sigs`.
pub fn restore_sigs<K: ThreadKernel>(kernel: &K, old: u64) {
    let pthread = crate::thread::current_pthread(kernel);
    pthread.sigmask.store(old, Ordering::Release);
}

/// The current mask word.
#[must_use]
pub fn current_mask<K: ThreadKernel>(kernel: &K) -> u64 {
    crate::thread::current_pthread(kernel)
        .sigmask
        .load(Ordering::Acquire)
}

/// The pending thread-directed bits.
#[must_use]
pub fn pending<K: ThreadKernel>(kernel: &K) -> u64 {
    crate::thread::current_pthread(kernel)
        .sigpending
        .load(Ordering::Acquire)
}

#[cfg(test)]
#[path = "sigmask_ut.rs"]
mod tests;
