// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_spin_init.c, pthread_spin_lock.c, pthread_spin_trylock.c, pthread_spin_unlock.c, pthread_spin_destroy.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! Spinlocks: one word, `0` free and `EBUSY` taken; no futex.

use core::sync::atomic::Ordering;

use crate::limits::{EBUSY, Errno, PTHREAD_PROCESS_SHARED};

/// `pthread_spinlock_t` (4 bytes).
#[repr(C)]
pub struct SpinLock {
    /// The lock word.
    pub word: core::sync::atomic::AtomicU32,
}

const _: () = assert!(core::mem::size_of::<SpinLock>() == 4);

impl Default for SpinLock {
    fn default() -> Self {
        Self::new()
    }
}

impl SpinLock {
    /// A statically initialised spinlock.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            word: core::sync::atomic::AtomicU32::new(0),
        }
    }
}

/// `pthread_spin_init`: `ENOTSUP` for a shared spinlock (PG-4).
///
/// # Errors
///
/// `ENOTSUP` when `shared` is `PTHREAD_PROCESS_SHARED`.
pub fn init(lock: &mut SpinLock, shared: i32) -> Result<(), Errno> {
    if shared == PTHREAD_PROCESS_SHARED {
        return Err(crate::limits::ENOTSUP);
    }
    *lock = SpinLock::new();
    Ok(())
}

/// `pthread_spin_destroy`.
pub fn destroy(_lock: &SpinLock) {}

/// `pthread_spin_lock`.
pub fn lock(lock: &SpinLock) {
    let word = &lock.word;
    while word.load(Ordering::Acquire) != 0
        || word
            .compare_exchange(0, EBUSY.0 as u32, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        core::hint::spin_loop();
    }
}

/// `pthread_spin_trylock`.
///
/// # Errors
///
/// `EBUSY` when already held.
pub fn trylock(lock: &SpinLock) -> Result<(), Errno> {
    let word = &lock.word;
    match word.compare_exchange(0, EBUSY.0 as u32, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => Ok(()),
        Err(_) => Err(EBUSY),
    }
}

/// `pthread_spin_unlock`.
pub fn unlock(lock: &SpinLock) {
    lock.word.store(0, Ordering::Release);
}

/// Whether the spinlock is held (diagnostics).
#[must_use]
pub fn is_locked(lock: &SpinLock) -> bool {
    lock.word.load(Ordering::Acquire) != 0
}

#[cfg(test)]
#[path = "spin_ut.rs"]
mod tests;
