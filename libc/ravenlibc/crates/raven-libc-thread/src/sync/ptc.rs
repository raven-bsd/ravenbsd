// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/lock_ptc.c, src/thread/vmlock.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor files have no per-file header; covered by musl/COPYRIGHT
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

//! The thread-creation inhibit lock (`ptc`) and the unmap lock (`vmlock`).
//!
//! `fork` takes `ptc` for writing, `pthread_create` for reading. `vmlock` serialises the unmaps
//! of a detached exiting thread against the mapping code. Both are process-private.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::kernel::ThreadKernel;
use crate::sync::wait::{wait, wake};

static VMLOCK: [AtomicI32; 2] = [AtomicI32::new(0), AtomicI32::new(0)];

/// `__inhibit_ptc`: stop new thread creation (fork).
pub fn inhibit_ptc<K: ThreadKernel>(kernel: &K) {
    let _ = crate::rwlock::wrlock(kernel, crate::raw::ptc_lock());
}

/// `__acquire_ptc`: let the caller create while other creators may run too.
pub fn acquire_ptc<K: ThreadKernel>(kernel: &K) {
    let _ = crate::rwlock::rdlock(kernel, crate::raw::ptc_lock());
}

/// `__release_ptc`.
pub fn release_ptc<K: ThreadKernel>(kernel: &K) {
    let _ = crate::rwlock::unlock(kernel, crate::raw::ptc_lock());
}

/// `__vm_wait`: wait until no thread holds `vmlock`.
pub fn vm_wait<K: ThreadKernel>(kernel: &K) {
    loop {
        let current = VMLOCK[0].load(Ordering::Acquire);
        if current == 0 {
            return;
        }
        wait(kernel, &VMLOCK[0], Some(&VMLOCK[1]), current, true);
    }
}

/// `__vm_lock`.
pub fn vm_lock() {
    VMLOCK[0].fetch_add(1, Ordering::AcqRel);
}

/// `__vm_unlock`; wakes a waiter only when the last holder leaves and somebody waited.
pub fn vm_unlock<K: ThreadKernel>(kernel: &K) {
    if VMLOCK[0].fetch_add(-1, Ordering::AcqRel) == 1 && VMLOCK[1].load(Ordering::Acquire) != 0 {
        let _ = wake(kernel, &VMLOCK[0], u32::MAX);
    }
}

/// The current `vmlock` congestion, for tests.
#[must_use]
pub fn vm_depth() -> i32 {
    VMLOCK[0].load(Ordering::Acquire)
}

#[cfg(test)]
#[path = "ptc_ut.rs"]
mod tests;
