// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_atfork.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `pthread_atfork`: a fixed table of 128 handlers (no allocation, A-thr-8).
//!
//! Prepare handlers run in reverse registration order; parent and child handlers in
//! registration order. The handler-list lock is held across `fork`, and the child runs its
//! handlers after the lock registry has been reset (T-thr-10).

use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use crate::kernel::ThreadKernel;
use crate::limits::{ATFORK_MAX, ENOMEM, Errno};
use crate::sync::lock;
use crate::sync::rank;

/// A handler address; the three kinds are stored in parallel arrays.
static PREPARE: [AtomicUsize; ATFORK_MAX] = [const { AtomicUsize::new(0) }; ATFORK_MAX];
static PARENT: [AtomicUsize; ATFORK_MAX] = [const { AtomicUsize::new(0) }; ATFORK_MAX];
static CHILD: [AtomicUsize; ATFORK_MAX] = [const { AtomicUsize::new(0) }; ATFORK_MAX];
static COUNT: AtomicUsize = AtomicUsize::new(0);
static LOCK: AtomicI32 = AtomicI32::new(0);

/// `pthread_atfork`.
///
/// # Errors
///
/// `ENOMEM` when the fixed table is full.
pub fn pthread_atfork(
    prepare: Option<fn()>,
    parent: Option<fn()>,
    child: Option<fn()>,
) -> Result<(), Errno> {
    let index = COUNT.load(Ordering::Acquire);
    if index >= ATFORK_MAX {
        return Err(ENOMEM);
    }
    if let (Some(p), Some(pa), Some(c)) = (PREPARE.get(index), PARENT.get(index), CHILD.get(index))
    {
        p.store(
            prepare.map_or(0, |function| function as usize),
            Ordering::Release,
        );
        pa.store(
            parent.map_or(0, |function| function as usize),
            Ordering::Release,
        );
        c.store(
            child.map_or(0, |function| function as usize),
            Ordering::Release,
        );
    }
    COUNT.fetch_add(1, Ordering::AcqRel);
    Ok(())
}

/// Clear the handler table (fork-child repair and serialised host tests).
pub fn clear() {
    COUNT.store(0, Ordering::Release);
    LOCK.store(0, Ordering::Release);
}

/// The number of registered handlers.
#[must_use]
pub fn count() -> usize {
    COUNT.load(Ordering::Acquire)
}

/// Take the handler-list lock (rank 1).
pub fn lock_handlers<K: ThreadKernel>(kernel: &K) {
    lock::lock(kernel, &LOCK);
}

/// Release the handler-list lock.
pub fn unlock_handlers<K: ThreadKernel>(kernel: &K) {
    lock::unlock(kernel, &LOCK);
}

/// Run the prepare handlers in reverse order.
pub fn run_prepare() {
    let mut index = COUNT.load(Ordering::Acquire).min(ATFORK_MAX);
    while index > 0 {
        index -= 1;
        if let Some(slot) = PREPARE.get(index) {
            let address = slot.load(Ordering::Acquire);
            if address != 0 {
                call(address);
            }
        }
    }
}

/// Run the parent or child handlers in registration order.
pub fn run_after(child: bool) {
    let count = COUNT.load(Ordering::Acquire).min(ATFORK_MAX);
    for index in 0..count {
        let slot = if child {
            CHILD.get(index)
        } else {
            PARENT.get(index)
        };
        if let Some(slot) = slot {
            let address = slot.load(Ordering::Acquire);
            if address != 0 {
                call(address);
            }
        }
    }
}

fn call(address: usize) {
    crate::raw::call_handler(address);
}

/// The rank of the atfork list lock, for the lock-order test.
pub const RANK: u8 = rank::ATFORK;

#[cfg(test)]
#[path = "atfork_ut.rs"]
mod tests;
