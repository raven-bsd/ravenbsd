// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_create.c (__tl_lock/__tl_unlock/__tl_sync) @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! The thread-list lock (rank 9) and `__tl_sync`.
//!
//! The lock word holds the owner's tid, so the same thread can re-enter it. `__tl_sync` is the
//! race fix of `pthread_join`: the kernel must have cleared the exiting thread's tid before a
//! joiner unmaps its stack. On the host the exiting thread releases the lock only after its last
//! user instruction and the fake marks it dead at `exit`; the joiner waits here.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::kernel::ThreadKernel;
use crate::raw;
use crate::sync::wait::wait;
use crate::thread::current_tid;

static LIST_LOCK: AtomicI32 = AtomicI32::new(0);
static LIST_WAITERS: AtomicI32 = AtomicI32::new(0);

/// `__tl_lock`.
pub fn lock<K: ThreadKernel>(kernel: &K) {
    let tid = current_tid(kernel);
    loop {
        let value = LIST_LOCK.load(Ordering::Acquire);
        if value == tid && tid != 0 {
            return;
        }
        if LIST_LOCK
            .compare_exchange(0, tid, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return;
        }
        wait(kernel, &LIST_LOCK, Some(&LIST_WAITERS), value, true);
    }
}

/// `__tl_unlock`.
pub fn unlock<K: ThreadKernel>(kernel: &K) {
    LIST_LOCK.store(0, Ordering::Release);
    if LIST_WAITERS.load(Ordering::Acquire) != 0 {
        kernel.futex_wake(raw::as_u32(&LIST_LOCK), 1);
    }
}

/// `__tl_sync`: wait for a departing thread to release the list lock.
pub fn sync<K: ThreadKernel>(kernel: &K) {
    core::sync::atomic::fence(Ordering::SeqCst);
    let value = LIST_LOCK.load(Ordering::Acquire);
    if value == 0 {
        return;
    }
    wait(kernel, &LIST_LOCK, Some(&LIST_WAITERS), value, false);
    if LIST_WAITERS.load(Ordering::Acquire) != 0 {
        kernel.futex_wake(raw::as_u32(&LIST_LOCK), 1);
    }
}

/// The fork child stores 0 into the lock and forgets the recursion depth (A-thr-9).
pub fn reset_for_fork_child() {
    LIST_LOCK.store(0, Ordering::Release);
    LIST_WAITERS.store(0, Ordering::Release);
}

/// The current list-lock owner tid, for diagnostics.
#[must_use]
pub fn owner() -> i32 {
    LIST_LOCK.load(Ordering::Acquire)
}

#[cfg(test)]
#[path = "list_ut.rs"]
mod tests;
