// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/sem_init.c, sem_post.c, sem_wait.c, sem_timedwait.c, sem_trywait.c, sem_getvalue.c, sem_destroy.c, sem_open.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! Semaphores: `val[0]` count (bit 31 is the waiter flag), `val[1]` waiters, `val[2]` private
//! flag. `sem_post` takes no lock and allocates nothing (async-signal-safe). `sem_open`,
//! `sem_close` and `sem_unlink` are `ENOSYS`: there is no shared object and no PG-4.

use core::sync::atomic::Ordering;

use crate::kernel::ThreadKernel;
use crate::limits::{
    EAGAIN, EINVAL, ENOSYS, ENOTSUP, EOVERFLOW, Errno, PTHREAD_PROCESS_PRIVATE,
    PTHREAD_PROCESS_SHARED, SEM_VALUE_MAX, Timespec,
};
use crate::raw;
use crate::sync::timedwait::timedwait_cp;

/// `sem_t` (32 bytes).
#[repr(C)]
pub struct Sem {
    /// The eight words of the C object.
    pub val: [core::sync::atomic::AtomicU32; 8],
}

const _: () = assert!(core::mem::size_of::<Sem>() == 32);
const _: () = assert!(core::mem::align_of::<Sem>() == 4);

impl Sem {
    /// A statically initialised semaphore with value 0.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            val: [const { core::sync::atomic::AtomicU32::new(0) }; 8],
        }
    }

    fn count_word(&self) -> &core::sync::atomic::AtomicI32 {
        raw::as_i32(&self.val[0])
    }

    fn waiters_word(&self) -> &core::sync::atomic::AtomicI32 {
        raw::as_i32(&self.val[1])
    }

    /// The current count (`val[0] & SEM_VALUE_MAX`).
    #[must_use]
    pub fn value(&self) -> i32 {
        self.count_word().load(Ordering::Acquire) & SEM_VALUE_MAX
    }
}

impl Default for Sem {
    fn default() -> Self {
        Self::new()
    }
}

/// `sem_init`.
///
/// # Errors
///
/// `EINVAL` above `SEM_VALUE_MAX` and `ENOTSUP` for a shared semaphore.
pub fn init<K: ThreadKernel>(
    kernel: &K,
    sem: &mut Sem,
    pshared: i32,
    value: u32,
) -> Result<(), Errno> {
    if value > SEM_VALUE_MAX as u32 {
        return Err(EINVAL);
    }
    if pshared != PTHREAD_PROCESS_PRIVATE && pshared != PTHREAD_PROCESS_SHARED {
        return Err(EINVAL);
    }
    if pshared == PTHREAD_PROCESS_SHARED && !kernel.caps().shared_futex {
        return Err(ENOTSUP);
    }
    *sem = Sem::new();
    sem.val[0].store(value, Ordering::Release);
    sem.val[1].store(0, Ordering::Release);
    sem.val[2].store(
        if pshared == PTHREAD_PROCESS_PRIVATE {
            128
        } else {
            0
        },
        Ordering::Release,
    );
    Ok(())
}

/// `sem_destroy`.
pub fn destroy(_sem: &Sem) -> Result<(), Errno> {
    Ok(())
}

/// `sem_post`. No lock, no allocation.
///
/// # Errors
///
/// `EOVERFLOW` at `SEM_VALUE_MAX`.
pub fn post<K: ThreadKernel>(kernel: &K, sem: &Sem) -> Result<(), Errno> {
    loop {
        let value = sem.count_word().load(Ordering::Acquire);
        let waiters = sem.waiters_word().load(Ordering::Acquire);
        if (value & SEM_VALUE_MAX) == SEM_VALUE_MAX {
            return Err(EOVERFLOW);
        }
        let mut new = value.wrapping_add(1);
        if waiters <= 1 {
            new &= !(i32::MIN);
        }
        if sem
            .count_word()
            .compare_exchange(value, new, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            if value < 0 || waiters != 0 {
                let count = if waiters > 1 { 1 } else { u32::MAX };
                kernel.futex_wake(raw::as_u32(sem.count_word()), count);
            }
            return Ok(());
        }
    }
}

/// `sem_trywait`.
///
/// # Errors
///
/// `EAGAIN` when the count is zero.
pub fn trywait(sem: &Sem) -> Result<(), Errno> {
    loop {
        let value = sem.count_word().load(Ordering::Acquire);
        if value & SEM_VALUE_MAX == 0 {
            return Err(EAGAIN);
        }
        if sem
            .count_word()
            .compare_exchange(
                value,
                value.wrapping_sub(1),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        {
            return Ok(());
        }
    }
}

/// `sem_wait` (a cancellation point).
///
/// # Errors
///
/// See [`timedwait`].
pub fn wait<K: ThreadKernel>(kernel: &K, sem: &Sem) -> Result<(), Errno> {
    timedwait(kernel, sem, None)
}

/// `sem_timedwait` (a cancellation point).
///
/// # Errors
///
/// `EAGAIN` is retried; `ETIMEDOUT` for a passed deadline; `EINTR` from a kick.
pub fn timedwait<K: ThreadKernel>(
    kernel: &K,
    sem: &Sem,
    at: Option<Timespec>,
) -> Result<(), Errno> {
    crate::thread::cancel::cancel_point_check(kernel);
    if trywait(sem).is_ok() {
        return Ok(());
    }
    let mut spins = 100u32;
    while spins > 0
        && (sem.count_word().load(Ordering::Acquire) & SEM_VALUE_MAX) == 0
        && sem.waiters_word().load(Ordering::Acquire) == 0
    {
        core::hint::spin_loop();
        spins -= 1;
    }
    while trywait(sem).is_err() {
        sem.waiters_word().fetch_add(1, Ordering::AcqRel);
        let _ = sem
            .count_word()
            .compare_exchange(0, i32::MIN, Ordering::AcqRel, Ordering::Acquire);
        let result = timedwait_cp(
            kernel,
            sem.count_word(),
            i32::MIN,
            crate::limits::CLOCK_REALTIME,
            at,
        );
        sem.waiters_word().fetch_sub(1, Ordering::AcqRel);
        result?;
    }
    Ok(())
}

/// `sem_clockwait`.
///
/// # Errors
///
/// See [`timedwait`].
pub fn clockwait<K: ThreadKernel>(
    kernel: &K,
    sem: &Sem,
    at: Option<Timespec>,
) -> Result<(), Errno> {
    timedwait(kernel, sem, at)
}

/// `sem_getvalue`.
#[must_use]
pub fn getvalue(sem: &Sem) -> i32 {
    sem.value()
}

/// `sem_open` returns `ENOSYS` (no shm filesystem, no PG-4).
///
/// # Errors
///
/// Always `ENOSYS`.
pub fn open(_name: &[u8], _flags: i32) -> Result<usize, Errno> {
    Err(ENOSYS)
}

/// `sem_close` returns `ENOSYS`.
///
/// # Errors
///
/// Always `ENOSYS`.
pub fn close(_sem: &Sem) -> Result<(), Errno> {
    Err(ENOSYS)
}

/// `sem_unlink` returns `ENOSYS`.
///
/// # Errors
///
/// Always `ENOSYS`.
pub fn unlink(_name: &[u8]) -> Result<(), Errno> {
    Err(ENOSYS)
}

#[cfg(test)]
#[path = "sem_ut.rs"]
mod tests;
