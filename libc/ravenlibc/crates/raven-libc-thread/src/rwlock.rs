// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_rwlock_init.c, pthread_rwlock_rdlock.c, pthread_rwlock_timedrdlock.c, pthread_rwlock_tryrdlock.c, pthread_rwlock_wrlock.c, pthread_rwlock_timedwrlock.c, pthread_rwlock_trywrlock.c, pthread_rwlock_unlock.c, pthread_rwlock_destroy.c, pthread_rwlockattr_setpshared.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! Read-write locks (reader-preferring, OPEN-thr-3).
//!
//! One word holds `0` (free), `0x7fffffff` (write-locked) or `1..=0x7ffffffe` (readers), with
//! bit 31 used while a timed waiter sleeps. `_rw_waiters` counts timed waiters.

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use crate::kernel::ThreadKernel;
use crate::limits::{
    EAGAIN, EBUSY, EINVAL, ENOTSUP, Errno, PTHREAD_PROCESS_PRIVATE, PTHREAD_PROCESS_SHARED,
    Timespec, WAITERS_BIT,
};
use crate::raw;
use crate::sync::timedwait::timedwait;

/// `pthread_rwlock_t` (56 bytes).
#[repr(C, align(8))]
pub struct RwLock {
    /// The fourteen words of the C object.
    pub words: [AtomicU32; 14],
}

const _: () = assert!(core::mem::size_of::<RwLock>() == 56);
const _: () = assert!(core::mem::align_of::<RwLock>() == 8);

impl RwLock {
    /// A statically initialised read-write lock.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            words: [const { AtomicU32::new(0) }; 14],
        }
    }

    /// `_rw_lock`.
    #[must_use]
    pub fn lock_word(&self) -> &AtomicI32 {
        raw::as_i32(&self.words[0])
    }

    /// `_rw_waiters`.
    #[must_use]
    pub fn waiters_word(&self) -> &AtomicI32 {
        raw::as_i32(&self.words[1])
    }

    /// `_rw_shared`.
    #[must_use]
    pub fn is_shared(&self) -> bool {
        self.words[2].load(Ordering::Acquire) != 0
    }

    /// The current lock value.
    #[must_use]
    pub fn value(&self) -> i32 {
        self.lock_word().load(Ordering::Acquire)
    }
}

impl Default for RwLock {
    fn default() -> Self {
        Self::new()
    }
}

/// `pthread_rwlockattr_t` (8 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RwLockAttr {
    /// `__attr[0]`: private or shared.
    pub shared: u32,
    /// `__attr[1]`, unused.
    pub pad: u32,
}

const _: () = assert!(core::mem::size_of::<RwLockAttr>() == 8);

/// `pthread_rwlockattr_init`.
pub fn attr_init() -> RwLockAttr {
    RwLockAttr { shared: 0, pad: 0 }
}

/// `pthread_rwlockattr_destroy`.
pub fn attr_destroy(_attr: &RwLockAttr) {}

/// `pthread_rwlockattr_setpshared`.
///
/// # Errors
///
/// `EINVAL` outside the two values and `ENOTSUP` for shared without PG-4.
pub fn attr_setpshared<K: ThreadKernel>(
    kernel: &K,
    attr: &mut RwLockAttr,
    pshared: i32,
) -> Result<(), Errno> {
    if pshared != PTHREAD_PROCESS_PRIVATE && pshared != PTHREAD_PROCESS_SHARED {
        return Err(EINVAL);
    }
    if pshared == PTHREAD_PROCESS_SHARED && !kernel.caps().shared_futex {
        return Err(ENOTSUP);
    }
    attr.shared = pshared as u32;
    Ok(())
}

/// `pthread_rwlockattr_getpshared`.
#[must_use]
pub fn attr_getpshared(attr: &RwLockAttr) -> i32 {
    attr.shared as i32
}

/// `pthread_rwlockattr_setkind_np`: accepted and ignored (reader-preferring).
///
/// # Errors
///
/// `EINVAL` for a negative kind.
pub fn attr_setkind_np(attr: &mut RwLockAttr, kind: i32) -> Result<(), Errno> {
    if kind < 0 {
        return Err(EINVAL);
    }
    let _ = attr;
    Ok(())
}

/// `pthread_rwlockattr_getkind_np`: always the reader-preferring kind (`0`).
#[must_use]
pub fn attr_getkind_np(_attr: &RwLockAttr) -> i32 {
    0
}

/// `pthread_rwlock_init`.
pub fn init(lock: &mut RwLock, attr: Option<&RwLockAttr>) {
    *lock = RwLock::new();
    if let Some(attr) = attr {
        lock.words[2].store(attr.shared.wrapping_mul(128), Ordering::Release);
    }
}

/// `pthread_rwlock_destroy`.
pub fn destroy(_lock: &RwLock) {}

/// `pthread_rwlock_tryrdlock`.
///
/// # Errors
///
/// `EBUSY` while a writer holds the lock, `EAGAIN` at reader saturation.
pub fn tryrdlock(lock: &RwLock) -> Result<(), Errno> {
    loop {
        let value = lock.value();
        let count = (value as u32) & 0x7fff_ffff;
        if count == 0x7fff_ffff {
            return Err(EBUSY);
        }
        if count == 0x7fff_fffe {
            return Err(EAGAIN);
        }
        if lock
            .lock_word()
            .compare_exchange(
                value,
                value.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            return Ok(());
        }
    }
}

/// `pthread_rwlock_rdlock`.
///
/// # Errors
///
/// See [`timedrdlock`].
pub fn rdlock<K: ThreadKernel>(kernel: &K, lock: &RwLock) -> Result<(), Errno> {
    timedrdlock(kernel, lock, None)
}

/// `pthread_rwlock_timedrdlock`.
///
/// # Errors
///
/// `ETIMEDOUT` for a passed deadline, plus [`tryrdlock`]'s errors.
pub fn timedrdlock<K: ThreadKernel>(
    kernel: &K,
    lock: &RwLock,
    at: Option<Timespec>,
) -> Result<(), Errno> {
    let mut outcome = tryrdlock(lock);
    if outcome != Err(EBUSY) {
        return outcome;
    }
    let mut spins = 100u32;
    while spins > 0 && lock.value() != 0 && lock.waiters_word().load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
        spins -= 1;
    }
    loop {
        outcome = tryrdlock(lock);
        if outcome != Err(EBUSY) {
            break;
        }
        let value = lock.value();
        if value == 0 || ((value as u32) & 0x7fff_ffff) != 0x7fff_ffff {
            continue;
        }
        let waited = value | (WAITERS_BIT as i32);
        lock.waiters_word().fetch_add(1, Ordering::AcqRel);
        let _ =
            lock.lock_word()
                .compare_exchange(value, waited, Ordering::AcqRel, Ordering::Acquire);
        outcome = timedwait(
            kernel,
            lock.lock_word(),
            waited,
            crate::limits::CLOCK_REALTIME,
            at,
            !lock.is_shared(),
        );
        lock.waiters_word().fetch_sub(1, Ordering::AcqRel);
        if let Err(error) = outcome
            && error != crate::limits::EINTR
        {
            return Err(error);
        }
    }
    outcome
}

/// `pthread_rwlock_trywrlock`.
///
/// # Errors
///
/// `EBUSY` when held by a reader or writer.
pub fn trywrlock(lock: &RwLock) -> Result<(), Errno> {
    match lock
        .lock_word()
        .compare_exchange(0, 0x7fff_ffff, Ordering::AcqRel, Ordering::Acquire)
    {
        Ok(_) => Ok(()),
        Err(_) => Err(EBUSY),
    }
}

/// `pthread_rwlock_wrlock`.
///
/// # Errors
///
/// See [`timedwrlock`].
pub fn wrlock<K: ThreadKernel>(kernel: &K, lock: &RwLock) -> Result<(), Errno> {
    timedwrlock(kernel, lock, None)
}

/// `pthread_rwlock_timedwrlock`.
///
/// # Errors
///
/// `ETIMEDOUT` for a passed deadline, plus [`trywrlock`]'s errors.
pub fn timedwrlock<K: ThreadKernel>(
    kernel: &K,
    lock: &RwLock,
    at: Option<Timespec>,
) -> Result<(), Errno> {
    let mut outcome = trywrlock(lock);
    if outcome != Err(EBUSY) {
        return outcome;
    }
    let mut spins = 100u32;
    while spins > 0 && lock.value() != 0 && lock.waiters_word().load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
        spins -= 1;
    }
    loop {
        outcome = trywrlock(lock);
        if outcome != Err(EBUSY) {
            break;
        }
        let value = lock.value();
        if value == 0 {
            continue;
        }
        let waited = value | (WAITERS_BIT as i32);
        lock.waiters_word().fetch_add(1, Ordering::AcqRel);
        let _ =
            lock.lock_word()
                .compare_exchange(value, waited, Ordering::AcqRel, Ordering::Acquire);
        outcome = timedwait(
            kernel,
            lock.lock_word(),
            waited,
            crate::limits::CLOCK_REALTIME,
            at,
            !lock.is_shared(),
        );
        lock.waiters_word().fetch_sub(1, Ordering::AcqRel);
        if let Err(error) = outcome
            && error != crate::limits::EINTR
        {
            return Err(error);
        }
    }
    outcome
}

/// `pthread_rwlock_unlock`.
///
/// # Errors
///
/// Never.
pub fn unlock<K: ThreadKernel>(kernel: &K, lock: &RwLock) -> Result<(), Errno> {
    loop {
        let value = lock.value();
        let count = (value as u32) & 0x7fff_ffff;
        let waiters = lock.waiters_word().load(Ordering::Acquire);
        let new = if count == 0x7fff_ffff || count == 1 {
            0
        } else {
            value.wrapping_sub(1)
        };
        if lock
            .lock_word()
            .compare_exchange(value, new, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            if new == 0 && (waiters != 0 || value < 0) {
                kernel.futex_wake(raw::as_u32(lock.lock_word()), count);
            }
            return Ok(());
        }
    }
}

/// The current reader count (for tests and diagnostics).
#[must_use]
pub fn reader_count(lock: &RwLock) -> u32 {
    (lock.value() as u32) & 0x7fff_ffff
}

#[cfg(test)]
#[path = "rwlock_ut.rs"]
mod tests;
