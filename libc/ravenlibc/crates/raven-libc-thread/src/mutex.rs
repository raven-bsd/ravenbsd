// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_mutex_lock.c, pthread_mutex_trylock.c, pthread_mutex_timedlock.c, pthread_mutex_unlock.c, pthread_mutex_init.c, pthread_mutex_destroy.c, pthread_mutex_consistent.c, pthread_mutexattr_setprotocol.c, pthread_mutexattr_setpshared.c, pthread_mutexattr_setrobust.c, pthread_mutexattr_settype.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! Mutexes: normal, recursive, error-checking, robust and the PI-gated paths.
//!
//! The word layout is the donor's: word 0 is `m_type` (kind bits 0-1, robust bit 2, PI bit 3,
//! shared bit 7), word 1 is `m_lock` (30-bit owner, bit 31 waiters, bit 30 owner-died), word 2 is
//! `m_waiters`, word 5 is `m_count`, and the two pointer slots at bytes 24 and 32 are the robust
//! list node. A normal `lock` fast path is one CAS (A-thr-10). `pthread_mutex_lock` is not a
//! cancellation point (T-thr-13).

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use crate::kernel::ThreadKernel;
use crate::limits::{
    EAGAIN, EBUSY, EDEADLK, EINVAL, ENOTRECOVERABLE, ENOTSUP, EOWNERDEAD, EPERM, MUTEX_TYPE_MASK,
    MUTEX_TYPE_PI, MUTEX_TYPE_ROBUST, MUTEX_TYPE_SHARED, MUTEX_UNRECOVERABLE, OWNER_DIED,
    OWNER_MASK, PTHREAD_MUTEX_ERRORCHECK, PTHREAD_MUTEX_NORMAL, PTHREAD_MUTEX_RECURSIVE,
    PTHREAD_PRIO_INHERIT, PTHREAD_PRIO_NONE, PTHREAD_PRIO_PROTECT, PTHREAD_PROCESS_PRIVATE,
    PTHREAD_PROCESS_SHARED, Timespec, WAITERS_BIT,
};
use crate::raw;
use crate::sched_point;
use crate::sync::timedwait::timedwait;
use crate::thread::current_pthread;

/// `pthread_mutex_t` (40 bytes, 8-byte aligned).
#[repr(C, align(8))]
pub struct Mutex {
    /// The ten words of the C object, all zero for `PTHREAD_MUTEX_INITIALIZER`.
    pub words: [AtomicU32; 10],
}

const _: () = assert!(core::mem::size_of::<Mutex>() == 40);
const _: () = assert!(core::mem::align_of::<Mutex>() == 8);

impl Mutex {
    /// A statically initialised normal mutex.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            words: [const { AtomicU32::new(0) }; 10],
        }
    }

    /// `m_type`.
    #[must_use]
    pub fn type_word(&self) -> i32 {
        self.words[0].load(Ordering::Acquire) as i32
    }

    /// The kind bits (`m_type & 15`, which still carries robust and PI).
    #[must_use]
    pub fn kind(&self) -> i32 {
        self.type_word() & MUTEX_TYPE_MASK
    }

    /// Whether the mutex is robust.
    #[must_use]
    pub fn is_robust(&self) -> bool {
        self.type_word() & MUTEX_TYPE_ROBUST != 0
    }

    /// Whether the mutex is process-shared.
    #[must_use]
    pub fn is_shared(&self) -> bool {
        self.type_word() & MUTEX_TYPE_SHARED != 0
    }

    /// `m_lock` as an atomic word.
    #[must_use]
    pub fn lock_word(&self) -> &AtomicI32 {
        raw::as_i32(&self.words[1])
    }

    /// `m_waiters`.
    #[must_use]
    pub fn waiters_word(&self) -> &AtomicI32 {
        raw::as_i32(&self.words[2])
    }

    /// The current `m_lock` value.
    #[must_use]
    pub fn lock_value(&self) -> u32 {
        self.lock_word().load(Ordering::Acquire) as u32
    }

    /// The current `m_count`.
    #[must_use]
    pub fn count(&self) -> i32 {
        raw::as_i32(&self.words[5]).load(Ordering::Acquire)
    }

    fn set_count(&self, value: i32) {
        raw::as_i32(&self.words[5]).store(value, Ordering::Release);
    }

    fn next_addr(&self) -> usize {
        self as *const Self as usize + 32
    }

    fn prev_addr(&self) -> usize {
        self as *const Self as usize + 24
    }

    /// The `m_waiters` value.
    #[must_use]
    pub fn waiters(&self) -> i32 {
        self.waiters_word().load(Ordering::Acquire)
    }

    /// Set the type word (init and attribute tests).
    pub fn set_type(&mut self, value: i32) {
        self.words[0].store(value as u32, Ordering::Release);
    }
}

impl Default for Mutex {
    fn default() -> Self {
        Self::new()
    }
}

/// `pthread_mutexattr_t` (4 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MutexAttr {
    /// The packed attribute word.
    pub attr: i32,
}

const _: () = assert!(core::mem::size_of::<MutexAttr>() == 4);

/// `pthread_mutexattr_init`.
pub fn attr_init() -> MutexAttr {
    MutexAttr { attr: 0 }
}

/// `pthread_mutexattr_destroy`.
pub fn attr_destroy(_attr: &MutexAttr) {}

/// `pthread_mutexattr_settype`.
///
/// # Errors
///
/// `EINVAL` outside normal, recursive and error-checking.
pub fn attr_settype(attr: &mut MutexAttr, kind: i32) -> Result<(), crate::limits::Errno> {
    if !(0..=2).contains(&kind) {
        return Err(EINVAL);
    }
    attr.attr = (attr.attr & !3) | kind;
    Ok(())
}

/// `pthread_mutexattr_gettype`.
#[must_use]
pub fn attr_gettype(attr: &MutexAttr) -> i32 {
    attr.attr & 3
}

/// `pthread_mutexattr_setpshared`.
///
/// # Errors
///
/// `EINVAL` outside private/shared and `ENOTSUP` for shared without PG-4.
pub fn attr_setpshared<K: ThreadKernel>(
    kernel: &K,
    attr: &mut MutexAttr,
    pshared: i32,
) -> Result<(), crate::limits::Errno> {
    if pshared != PTHREAD_PROCESS_PRIVATE && pshared != PTHREAD_PROCESS_SHARED {
        return Err(EINVAL);
    }
    if pshared == PTHREAD_PROCESS_SHARED && !kernel.caps().shared_futex {
        return Err(ENOTSUP);
    }
    attr.attr &= !MUTEX_TYPE_SHARED;
    if pshared == PTHREAD_PROCESS_SHARED {
        attr.attr |= MUTEX_TYPE_SHARED;
    }
    Ok(())
}

/// `pthread_mutexattr_getpshared`.
#[must_use]
pub fn attr_getpshared(attr: &MutexAttr) -> i32 {
    if attr.attr & MUTEX_TYPE_SHARED != 0 {
        PTHREAD_PROCESS_SHARED
    } else {
        PTHREAD_PROCESS_PRIVATE
    }
}

/// `pthread_mutexattr_setrobust`.
///
/// # Errors
///
/// `EINVAL` outside stalled/robust.
pub fn attr_setrobust(attr: &mut MutexAttr, robust: i32) -> Result<(), crate::limits::Errno> {
    if robust != 0 && robust != 1 {
        return Err(EINVAL);
    }
    attr.attr &= !MUTEX_TYPE_ROBUST;
    if robust == 1 {
        attr.attr |= MUTEX_TYPE_ROBUST;
    }
    Ok(())
}

/// `pthread_mutexattr_getrobust`.
#[must_use]
pub fn attr_getrobust(attr: &MutexAttr) -> i32 {
    i32::from(attr.attr & MUTEX_TYPE_ROBUST != 0)
}

/// `pthread_mutexattr_setprotocol`.
///
/// # Errors
///
/// `EINVAL` for an unknown protocol and `ENOTSUP` for inherit/protect without PG-6.
pub fn attr_setprotocol<K: ThreadKernel>(
    kernel: &K,
    attr: &mut MutexAttr,
    protocol: i32,
) -> Result<(), crate::limits::Errno> {
    match protocol {
        PTHREAD_PRIO_NONE => {
            attr.attr &= !MUTEX_TYPE_PI;
            Ok(())
        }
        PTHREAD_PRIO_INHERIT | PTHREAD_PRIO_PROTECT => {
            if kernel.caps().pi {
                attr.attr |= MUTEX_TYPE_PI;
                Ok(())
            } else {
                Err(ENOTSUP)
            }
        }
        _ => Err(EINVAL),
    }
}

/// `pthread_mutexattr_getprotocol`.
#[must_use]
pub fn attr_getprotocol(attr: &MutexAttr) -> i32 {
    if attr.attr & MUTEX_TYPE_PI != 0 {
        PTHREAD_PRIO_INHERIT
    } else {
        PTHREAD_PRIO_NONE
    }
}

/// `pthread_mutex_init`.
pub fn init(mutex: &mut Mutex, attr: Option<&MutexAttr>) {
    *mutex = Mutex::new();
    if let Some(attr) = attr {
        mutex.set_type(attr.attr);
    }
}

/// `pthread_mutex_destroy`.
///
/// # Errors
///
/// Never in this implementation; the shared-without-PG-4 object is `ENOTSUP` at init time.
pub fn destroy<K: ThreadKernel>(kernel: &K, mutex: &Mutex) -> Result<(), crate::limits::Errno> {
    if mutex.type_word() > 128 {
        crate::sync::ptc::vm_wait(kernel);
    }
    Ok(())
}

/// `pthread_mutex_lock` (not a cancellation point).
///
/// # Errors
///
/// The donor's errors: `EDEADLK`, `EOWNERDEAD`, `ENOTRECOVERABLE`, `ENOTSUP` for PI.
pub fn lock<K: ThreadKernel>(kernel: &K, mutex: &Mutex) -> Result<(), crate::limits::Errno> {
    if mutex.type_word() & MUTEX_TYPE_PI != 0 {
        return Err(ENOTSUP);
    }
    if mutex.kind() == PTHREAD_MUTEX_NORMAL
        && mutex
            .lock_word()
            .compare_exchange(0, EBUSY.0 as i32, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    {
        return Ok(());
    }
    timedlock_inner(kernel, mutex, None)
}

/// `pthread_mutex_trylock`.
///
/// # Errors
///
/// The donor's `EBUSY`, `EAGAIN`, `EDEADLK`, `EOWNERDEAD`, `ENOTRECOVERABLE`.
pub fn trylock<K: ThreadKernel>(kernel: &K, mutex: &Mutex) -> Result<(), crate::limits::Errno> {
    if mutex.type_word() & MUTEX_TYPE_PI != 0 {
        return Err(ENOTSUP);
    }
    if mutex.kind() == PTHREAD_MUTEX_NORMAL {
        return match mutex.lock_word().compare_exchange(
            0,
            EBUSY.0 as i32,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(_) => Err(EBUSY),
        };
    }
    trylock_owner(kernel, mutex)
}

fn trylock_owner<K: ThreadKernel>(kernel: &K, mutex: &Mutex) -> Result<(), crate::limits::Errno> {
    let type_word = mutex.type_word();
    let self_pthread = current_pthread(kernel);
    let tid = self_pthread.tid.load(Ordering::Acquire);
    let old = mutex.lock_value();
    let own = old & OWNER_MASK;
    let mut owner = tid as u32;
    if own == owner {
        if type_word & MUTEX_TYPE_PI != 0 && mutex.count() < 0 {
            mutex.set_count(0);
            return success_after_try(kernel, mutex, old & OWNER_DIED);
        }
        if type_word & 3 == PTHREAD_MUTEX_RECURSIVE {
            if mutex.count() == i32::MAX {
                return Err(EAGAIN);
            }
            mutex.set_count(mutex.count().wrapping_add(1));
            return Ok(());
        }
    }
    if own == MUTEX_UNRECOVERABLE & OWNER_MASK {
        return Err(ENOTRECOVERABLE);
    }
    if own != 0 || (old != 0 && type_word & MUTEX_TYPE_ROBUST == 0) {
        return Err(EBUSY);
    }
    if mutex.is_robust() {
        if self_pthread.robust_off == 0 {
            raw::set_robust_off(self_pthread, 4i32.wrapping_sub(32));
            let _ = kernel.set_robust_list(raw::robust_head_addr(self_pthread));
        }
        if mutex.waiters() != 0 {
            owner |= WAITERS_BIT;
        }
        raw::set_robust_pending(self_pthread, mutex.next_addr());
    }
    owner |= old & OWNER_DIED;
    if mutex
        .lock_word()
        .compare_exchange(
            old as i32,
            owner as i32,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        raw::set_robust_pending(self_pthread, 0);
        if type_word & (MUTEX_TYPE_PI | MUTEX_TYPE_ROBUST) == (MUTEX_TYPE_PI | MUTEX_TYPE_ROBUST)
            && mutex.waiters() != 0
        {
            return Err(ENOTRECOVERABLE);
        }
        return Err(EBUSY);
    }
    if type_word & MUTEX_TYPE_PI != 0 && mutex.waiters() != 0 {
        raw::set_robust_pending(self_pthread, 0);
        return Err(if type_word & MUTEX_TYPE_ROBUST != 0 {
            ENOTRECOVERABLE
        } else {
            EBUSY
        });
    }
    robust_push(kernel, mutex);
    if old != 0 {
        mutex.set_count(0);
        return Err(EOWNERDEAD);
    }
    Ok(())
}

fn success_after_try<K: ThreadKernel>(
    kernel: &K,
    mutex: &Mutex,
    old: u32,
) -> Result<(), crate::limits::Errno> {
    robust_push(kernel, mutex);
    if old != 0 {
        mutex.set_count(0);
        return Err(EOWNERDEAD);
    }
    Ok(())
}

/// Push the mutex node on the calling thread's user-space robust list.
pub(crate) fn robust_push<K: ThreadKernel>(kernel: &K, mutex: &Mutex) {
    if !mutex.is_robust() {
        return;
    }
    let self_pthread = current_pthread(kernel);
    let _ = self_pthread;
    let head = raw::robust_head(self_pthread);
    let self_head = raw::robust_head_addr(self_pthread);
    let next_addr = mutex.next_addr();
    raw::write_usize(next_addr, head);
    raw::write_usize(mutex.prev_addr(), self_head);
    if head != self_head {
        raw::write_usize(head.wrapping_sub(8), next_addr);
    }
    raw::set_robust_head(self_pthread, next_addr);
    raw::set_robust_pending(self_pthread, 0);
}

/// Unlink the mutex node from the robust list.
pub(crate) fn robust_unlink<K: ThreadKernel>(kernel: &K, mutex: &Mutex) {
    if !mutex.is_robust() {
        return;
    }
    let self_pthread = current_pthread(kernel);
    let self_head = raw::robust_head_addr(self_pthread);
    let prev = raw::read_usize(mutex.prev_addr());
    let next = raw::read_usize(mutex.next_addr());
    if prev != 0 {
        raw::write_usize(prev, next);
    }
    if next != 0 && next != self_head {
        raw::write_usize(next.wrapping_sub(8), prev);
    }
    raw::set_robust_pending(self_pthread, 0);
}

/// `pthread_mutex_timedlock`.
///
/// # Errors
///
/// A past deadline is `ETIMEDOUT` without a kernel call (A-thr-5).
pub fn timedlock<K: ThreadKernel>(
    kernel: &K,
    mutex: &Mutex,
    at: Option<Timespec>,
) -> Result<(), crate::limits::Errno> {
    timedlock_inner(kernel, mutex, at)
}

fn timedlock_inner<K: ThreadKernel>(
    kernel: &K,
    mutex: &Mutex,
    at: Option<Timespec>,
) -> Result<(), crate::limits::Errno> {
    if mutex.kind() == PTHREAD_MUTEX_NORMAL
        && mutex
            .lock_word()
            .compare_exchange(0, EBUSY.0 as i32, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    {
        return Ok(());
    }
    let type_word = mutex.type_word();
    let result = trylock_owner(kernel, mutex);
    if result != Err(EBUSY) {
        return result;
    }
    if type_word & MUTEX_TYPE_PI != 0 {
        // PG-6 is absent; the attribute setter never sets the bit, but a hand-built object could.
        return Err(ENOTSUP);
    }
    let mut spins = 100u32;
    while spins > 0 && mutex.lock_value() != 0 && mutex.waiters() == 0 {
        core::hint::spin_loop();
        spins -= 1;
    }
    let mut outcome: Result<(), crate::limits::Errno>;
    loop {
        outcome = trylock_owner(kernel, mutex);
        if outcome != Err(EBUSY) {
            break;
        }
        let current = mutex.lock_value();
        let own = current & OWNER_MASK;
        if own == 0 && (current == 0 || type_word & MUTEX_TYPE_ROBUST != 0) {
            continue;
        }
        if type_word & 3 == PTHREAD_MUTEX_ERRORCHECK
            && own == current_pthread(kernel).tid.load(Ordering::Acquire) as u32
        {
            return Err(EDEADLK);
        }
        sched_point!(kernel, "mutex_waiters_set");
        mutex.waiters_word().fetch_add(1, Ordering::AcqRel);
        let waited = current | WAITERS_BIT;
        let _ = mutex.lock_word().compare_exchange(
            current as i32,
            waited as i32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        sched_point!(kernel, "mutex_before_wait");
        outcome = timedwait(
            kernel,
            mutex.lock_word(),
            waited as i32,
            crate::limits::CLOCK_REALTIME,
            at,
            true,
        );
        mutex.waiters_word().fetch_sub(1, Ordering::AcqRel);
        if let Err(error) = outcome
            && error != crate::limits::EINTR
        {
            break;
        }
    }
    outcome
}

/// `pthread_mutex_unlock`.
///
/// # Errors
///
/// `EPERM` for a non-owner unlock of a checked or recursive mutex.
pub fn unlock<K: ThreadKernel>(kernel: &K, mutex: &Mutex) -> Result<(), crate::limits::Errno> {
    let waiters = mutex.waiters();
    let kind = mutex.kind();
    let mut new = 0i32;
    if kind != PTHREAD_MUTEX_NORMAL {
        let self_pthread = current_pthread(kernel);
        let old = mutex.lock_value();
        let own = old & OWNER_MASK;
        if own != self_pthread.tid.load(Ordering::Acquire) as u32 {
            return Err(EPERM);
        }
        if kind & 3 == PTHREAD_MUTEX_RECURSIVE && mutex.count() != 0 {
            mutex.set_count(mutex.count().wrapping_sub(1));
            return Ok(());
        }
        if kind & MUTEX_TYPE_ROBUST != 0 && old & OWNER_DIED != 0 {
            new = MUTEX_UNRECOVERABLE as i32;
        }
        robust_unlink(kernel, mutex);
    }
    if kind & MUTEX_TYPE_PI != 0 {
        return Err(ENOTSUP);
    }
    let previous = mutex.lock_word().swap(new, Ordering::AcqRel);
    if waiters != 0 || previous < 0 {
        kernel.futex_wake(raw::as_u32(mutex.lock_word()), 1);
    }
    Ok(())
}

/// `pthread_mutex_consistent`.
///
/// # Errors
///
/// `EINVAL` for a non-robust or not-owner-died mutex, `EPERM` for another thread's mutex.
pub fn consistent<K: ThreadKernel>(kernel: &K, mutex: &Mutex) -> Result<(), crate::limits::Errno> {
    let old = mutex.lock_value();
    let own = old & OWNER_MASK;
    if !mutex.is_robust() || own == 0 || old & OWNER_DIED == 0 {
        return Err(EINVAL);
    }
    if own != current_pthread(kernel).tid.load(Ordering::Acquire) as u32 {
        return Err(EPERM);
    }
    mutex
        .lock_word()
        .fetch_and(!(OWNER_DIED as i32), Ordering::AcqRel);
    Ok(())
}

/// `pthread_mutex_getprioceiling`.
///
/// # Errors
///
/// Always `ENOTSUP` (PG-6 absent).
pub fn getprioceiling(_mutex: &Mutex) -> Result<i32, crate::limits::Errno> {
    Err(ENOTSUP)
}

/// `pthread_mutex_setprioceiling`.
///
/// # Errors
///
/// Always `ENOTSUP` (PG-6 absent).
pub fn setprioceiling(_mutex: &Mutex, _ceiling: i32) -> Result<i32, crate::limits::Errno> {
    Err(ENOTSUP)
}

/// The raw `m_waiters` word of a mutex, for the condition variable's hand-over.
#[must_use]
pub fn waiters_word_of(mutex: &Mutex) -> &AtomicI32 {
    mutex.waiters_word()
}

/// The raw `m_lock` word of a mutex, for the condition variable's hand-over.
#[must_use]
pub fn lock_word_of(mutex: &Mutex) -> &AtomicI32 {
    mutex.lock_word()
}

/// The `m_type` word of a mutex.
#[must_use]
pub fn type_of(mutex: &Mutex) -> i32 {
    mutex.type_word()
}

#[cfg(test)]
#[path = "mutex_ut.rs"]
mod tests;
