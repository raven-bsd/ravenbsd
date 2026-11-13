// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_cond_timedwait.c, pthread_cond_signal.c, pthread_cond_broadcast.c, pthread_cond_wait.c, pthread_cond_init.c, pthread_cond_destroy.c, pthread_condattr_setclock.c, pthread_condattr_setpshared.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! Condition variables: the musl waiter-node design (A-thr-4, T-thr-4).
//!
//! Each waiter puts a node on its own stack with a three-state field (`WAITING`, `SIGNALED`,
//! `LEAVING`) and a private barrier futex. `signal`/`broadcast` detach the list and mark nodes
//! `SIGNALED`; each woken waiter hands the barrier to the next one or requeues it to the mutex.
//! This is not a counter plus wake-all: the hand-over decides exactly which waiter proceeds and
//! keeps the mutex's waiter accounting right. `PTHREAD_CANCEL_MASKED` is set for the duration of
//! the wait and a signal that was consumed suppresses a concurrent cancellation.

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use crate::kernel::ThreadKernel;
use crate::limits::{
    EINVAL, ENOTSUP, EPERM, Errno, PTHREAD_CANCEL_DISABLE, PTHREAD_CANCEL_MASKED,
    PTHREAD_PROCESS_PRIVATE, PTHREAD_PROCESS_SHARED, Timespec, WAITERS_BIT,
};
use crate::mutex::{lock_word_of, waiters_word_of};
use crate::raw;
use crate::sched_point;
use crate::sync::timedwait::timedwait_cp;

/// Waiter is on the condition-variable list.
pub const WAITING: i32 = 0;
/// The signaller has chosen this waiter.
pub const SIGNALED: i32 = 1;
/// The waiter is removing itself after a timeout or cancellation.
pub const LEAVING: i32 = 2;

/// One waiter node, living on the waiting thread's stack.
#[repr(C)]
pub struct Waiter {
    /// Previous node in the list (towards the tail).
    pub prev: usize,
    /// Next node in the list (towards the head).
    pub next: usize,
    /// `WAITING`, `SIGNALED` or `LEAVING`.
    pub state: AtomicI32,
    /// The private barrier futex.
    pub barrier: AtomicI32,
    /// Address of a local reference counter while this waiter is in a detached list.
    pub notify: usize,
}

impl Waiter {
    /// A node that is not yet linked.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prev: 0,
            next: 0,
            state: AtomicI32::new(WAITING),
            barrier: AtomicI32::new(2),
            notify: 0,
        }
    }

    /// The address used in the condition-variable list.
    #[must_use]
    pub fn address(&self) -> usize {
        self as *const Self as usize
    }
}

impl Default for Waiter {
    fn default() -> Self {
        Self::new()
    }
}

/// `pthread_cond_t` (48 bytes).
#[repr(C, align(8))]
pub struct Cond {
    /// The twelve words of the C object.
    pub words: [AtomicU32; 12],
}

const _: () = assert!(core::mem::size_of::<Cond>() == 48);
const _: () = assert!(core::mem::align_of::<Cond>() == 8);

impl Cond {
    /// A statically initialised condition variable.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            words: [const { AtomicU32::new(0) }; 12],
        }
    }

    /// Whether the object was created process-shared.
    #[must_use]
    pub fn is_shared(&self) -> bool {
        self.words[0].load(Ordering::Acquire) != 0 || self.words[1].load(Ordering::Acquire) != 0
    }

    /// `c_clock`.
    #[must_use]
    pub fn clock(&self) -> u32 {
        self.words[4].load(Ordering::Acquire)
    }

    /// Number of waiters currently linked on the private list (tests and diagnostics).
    #[must_use]
    pub fn waiter_count(&self) -> usize {
        let mut node = self.head();
        let mut count = 0usize;
        while node != 0 && count < 4096 {
            count = count.wrapping_add(1);
            node = crate::raw::waiter(node).next;
        }
        count
    }

    fn lock_word(&self) -> &AtomicI32 {
        raw::as_i32(&self.words[8])
    }

    fn head(&self) -> usize {
        raw::load_usize_atomic(&self.words, 2)
    }

    fn set_head(&self, value: usize) {
        raw::store_usize_atomic(&self.words, 2, value);
    }

    fn tail(&self) -> usize {
        raw::load_usize_atomic(&self.words, 10)
    }

    fn set_tail(&self, value: usize) {
        raw::store_usize_atomic(&self.words, 10, value);
    }
}

impl Default for Cond {
    fn default() -> Self {
        Self::new()
    }
}

/// `pthread_condattr_t` (4 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CondAttr {
    /// Clock in bits 0-30, shared flag in bit 31.
    pub attr: u32,
}

/// `pthread_condattr_init`.
pub fn attr_init() -> CondAttr {
    CondAttr { attr: 0 }
}

/// `pthread_condattr_destroy`.
pub fn attr_destroy(_attr: &CondAttr) {}

/// `pthread_condattr_setclock`.
///
/// # Errors
///
/// `EINVAL` for a negative or CPU-time clock.
pub fn attr_setclock(attr: &mut CondAttr, clock: i32) -> Result<(), Errno> {
    if clock < 0 || clock == 2 || clock == 3 {
        return Err(EINVAL);
    }
    attr.attr &= 0x8000_0000;
    attr.attr |= clock as u32;
    Ok(())
}

/// `pthread_condattr_getclock`.
#[must_use]
pub fn attr_getclock(attr: &CondAttr) -> i32 {
    (attr.attr & 0x7fff_ffff) as i32
}

/// `pthread_condattr_setpshared`.
///
/// # Errors
///
/// `EINVAL` outside the two values and `ENOTSUP` for shared without PG-4.
pub fn attr_setpshared<K: ThreadKernel>(
    kernel: &K,
    attr: &mut CondAttr,
    pshared: i32,
) -> Result<(), Errno> {
    if pshared != PTHREAD_PROCESS_PRIVATE && pshared != PTHREAD_PROCESS_SHARED {
        return Err(EINVAL);
    }
    if pshared == PTHREAD_PROCESS_SHARED && !kernel.caps().shared_futex {
        return Err(ENOTSUP);
    }
    attr.attr &= 0x7fff_ffff;
    if pshared == PTHREAD_PROCESS_SHARED {
        attr.attr |= 0x8000_0000;
    }
    Ok(())
}

/// `pthread_condattr_getpshared`.
#[must_use]
pub fn attr_getpshared(attr: &CondAttr) -> i32 {
    ((attr.attr >> 31) & 1) as i32
}

/// `pthread_cond_init`.
pub fn init(cond: &mut Cond, attr: Option<&CondAttr>) {
    *cond = Cond::new();
    if let Some(attr) = attr {
        cond.words[4].store(attr.attr & 0x7fff_ffff, Ordering::Release);
        if attr.attr >> 31 != 0 {
            cond.words[0].store(1, Ordering::Release);
        }
    }
}

/// `pthread_cond_destroy`.
///
/// # Errors
///
/// Never; a private condition variable has no externally visible state at rest.
pub fn destroy(_cond: &Cond) -> Result<(), Errno> {
    Ok(())
}

/// The condition variable's private two-state lock (`self-synchronized-destruction-safe`).
fn cond_lock<K: ThreadKernel>(kernel: &K, lock: &AtomicI32) {
    if lock
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        return;
    }
    let _ = lock.compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire);
    loop {
        let _ = kernel.futex_wait(raw::as_u32(lock), 2, None);
        if lock
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return;
        }
    }
}

fn cond_unlock<K: ThreadKernel>(kernel: &K, lock: &AtomicI32) {
    if lock.swap(0, Ordering::AcqRel) == 2 {
        kernel.futex_wake(raw::as_u32(lock), 1);
    }
}

/// Unlock a waiter barrier; the next waiter may proceed.
fn barrier_unlock<K: ThreadKernel>(kernel: &K, barrier: &AtomicI32) {
    if barrier.swap(0, Ordering::AcqRel) == 2 {
        kernel.futex_wake(raw::as_u32(barrier), 1);
    }
}

fn barrier_lock<K: ThreadKernel>(kernel: &K, barrier: &AtomicI32) {
    if barrier
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        return;
    }
    let _ = barrier.compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire);
    loop {
        let _ = kernel.futex_wait(raw::as_u32(barrier), 2, None);
        if barrier
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return;
        }
    }
}

/// `a_store(l, 0)` followed by a hand-over to the next waiter or the mutex.
fn unlock_requeue<K: ThreadKernel>(
    kernel: &K,
    barrier: &AtomicI32,
    mutex_lock: &AtomicI32,
    wake_only: i32,
) {
    barrier.store(0, Ordering::Release);
    if wake_only != 0 {
        kernel.futex_wake(raw::as_u32(barrier), 1);
        return;
    }
    sched_point!(kernel, "cond_before_requeue");
    if kernel.caps().requeue
        && kernel
            .futex_requeue(raw::as_u32(barrier), 0, raw::as_u32(mutex_lock), 1)
            .is_ok()
    {
        sched_point!(kernel, "cond_after_requeue");
        return;
    }
    sched_point!(kernel, "cond_after_requeue");
    kernel.futex_wake(raw::as_u32(barrier), 1);
}

/// The donor's `__private_cond_signal`.
fn private_cond_signal<K: ThreadKernel>(kernel: &K, cond: &Cond, mut count: i32) {
    cond_lock(kernel, cond.lock_word());
    let mut node = cond.tail();
    let mut first = 0usize;
    let reference = AtomicI32::new(0);
    while count != 0 && node != 0 {
        let waiter = raw::waiter(node);
        if waiter
            .state
            .compare_exchange(WAITING, SIGNALED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            reference.fetch_add(1, Ordering::AcqRel);
            raw::waiter_mut(node).notify = reference.as_ptr() as usize;
        } else {
            count = count.wrapping_sub(1);
            if first == 0 {
                first = node;
            }
        }
        node = raw::waiter(node).prev;
    }
    if node != 0 {
        let next = raw::waiter(node).next;
        if next != 0 {
            raw::waiter_mut(next).prev = 0;
        }
        raw::waiter_mut(node).next = 0;
    } else {
        cond.set_head(0);
    }
    cond.set_tail(node);
    cond_unlock(kernel, cond.lock_word());
    sched_point!(kernel, "cond_list_detached");
    while reference.load(Ordering::Acquire) != 0 {
        let current = reference.load(Ordering::Acquire);
        crate::sync::wait::wait(kernel, &reference, None, current, true);
    }
    if first != 0 {
        let barrier = &raw::waiter(first).barrier;
        barrier_unlock(kernel, barrier);
        sched_point!(kernel, "cond_barrier_handover");
    }
}

/// `pthread_cond_signal`.
///
/// # Errors
///
/// `ENOTSUP` for a process-shared condition variable (PG-4 absent).
pub fn signal<K: ThreadKernel>(kernel: &K, cond: &Cond) -> Result<(), Errno> {
    if cond.is_shared() {
        return Err(ENOTSUP);
    }
    private_cond_signal(kernel, cond, 1);
    Ok(())
}

/// `pthread_cond_broadcast`.
///
/// # Errors
///
/// `ENOTSUP` for a process-shared condition variable.
pub fn broadcast<K: ThreadKernel>(kernel: &K, cond: &Cond) -> Result<(), Errno> {
    if cond.is_shared() {
        return Err(ENOTSUP);
    }
    private_cond_signal(kernel, cond, -1);
    Ok(())
}

/// `pthread_cond_wait`.
///
/// # Errors
///
/// See [`timedwait`].
pub fn wait<K: ThreadKernel>(
    kernel: &K,
    cond: &Cond,
    mutex: &crate::mutex::Mutex,
) -> Result<(), Errno> {
    timedwait(kernel, cond, mutex, cond.clock(), None)
}

/// `pthread_cond_timedwait`.
///
/// # Errors
///
/// `EINVAL` for a bad timespec, `EPERM` when the mutex is not owned by the caller, `ENOTSUP` for
/// a shared condition variable, `ETIMEDOUT` for a passed deadline.
pub fn timedwait<K: ThreadKernel>(
    kernel: &K,
    cond: &Cond,
    mutex: &crate::mutex::Mutex,
    clock: u32,
    at: Option<Timespec>,
) -> Result<(), Errno> {
    if let Some(ts) = at
        && (ts.tv_nsec < 0 || ts.tv_nsec >= 1_000_000_000)
    {
        return Err(EINVAL);
    }
    if mutex.kind() != 0
        && (mutex.lock_value() & 0x3fff_ffff) != crate::thread::current_tid(kernel) as u32
    {
        return Err(EPERM);
    }
    crate::thread::cancel::cancel_point_check(kernel);
    if cond.is_shared() {
        return Err(ENOTSUP);
    }
    let mut node = Waiter::new();
    let address = node.address();
    cond_lock(kernel, cond.lock_word());
    node.barrier.store(2, Ordering::Release);
    let previous_head = cond.head();
    node.next = previous_head;
    cond.set_head(address);
    let tail = cond.tail();
    if tail == 0 {
        cond.set_tail(address);
    } else {
        raw::waiter_mut(previous_head).prev = address;
    }
    cond_unlock(kernel, cond.lock_word());
    sched_point!(kernel, "cond_node_linked");
    let _ = crate::mutex::unlock(kernel, mutex);
    sched_point!(kernel, "cond_after_mutex_unlock");
    let old_cancel = crate::thread::cancel::set_state_raw(kernel, PTHREAD_CANCEL_MASKED);
    if old_cancel == PTHREAD_CANCEL_DISABLE {
        crate::thread::cancel::restore_state_raw(kernel, old_cancel);
    }
    let mut error;
    loop {
        sched_point!(kernel, "cond_before_wait");
        error = timedwait_cp(kernel, &node.barrier, 2, clock, at);
        let current = node.barrier.load(Ordering::Acquire);
        if current != 2 {
            break;
        }
        match error {
            Ok(()) => {}
            Err(value) if value == crate::limits::EINTR => {}
            _ => break,
        }
    }
    if error == Err(crate::limits::EINTR) {
        error = Ok(());
    }
    let old_state = node
        .state
        .compare_exchange(WAITING, LEAVING, Ordering::AcqRel, Ordering::Acquire)
        .unwrap_or_else(|value| value);
    let mut removed_self = false;
    if old_state == WAITING {
        sched_point!(kernel, "cond_leaving");
        cond_lock(kernel, cond.lock_word());
        let prev = node.prev;
        let next = node.next;
        if cond.head() == address {
            cond.set_head(next);
        } else if prev != 0 {
            raw::waiter_mut(prev).next = next;
        }
        if cond.tail() == address {
            cond.set_tail(prev);
        } else if next != 0 {
            raw::waiter_mut(next).prev = prev;
        }
        cond_unlock(kernel, cond.lock_word());
        let notify = node.notify;
        if notify != 0 {
            // The notify word lives in the signaller's local `AtomicI32`; decrement it and
            // wake the signaller when it reaches one.
            let word = raw::counter(notify);
            if word.fetch_sub(1, Ordering::AcqRel) == 1 {
                kernel.futex_wake(raw::as_u32(word), 1);
            }
        }
    } else {
        removed_self = true;
        barrier_lock(kernel, &node.barrier);
    }
    let relock = crate::mutex::lock(kernel, mutex);
    if let Err(value) = relock {
        error = Err(value);
    }
    if removed_self {
        if node.next == 0 && mutex.kind() & crate::limits::MUTEX_TYPE_PI == 0 {
            waiters_word_of(mutex).fetch_add(1, Ordering::AcqRel);
        }
        let prev = node.prev;
        if prev != 0 {
            let lock = lock_word_of(mutex);
            let value = lock.load(Ordering::Acquire);
            if value > 0 {
                let _ = lock.compare_exchange(
                    value,
                    value | (WAITERS_BIT as i32),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
            }
            let barrier = &raw::waiter(prev).barrier;
            unlock_requeue(
                kernel,
                barrier,
                lock_word_of(mutex),
                mutex.kind() & (crate::limits::MUTEX_TYPE_PI | crate::limits::MUTEX_TYPE_SHARED),
            );
        } else if mutex.kind() & crate::limits::MUTEX_TYPE_PI == 0 {
            waiters_word_of(mutex).fetch_sub(1, Ordering::AcqRel);
        }
    }
    if error == Err(crate::limits::ECANCELED) {
        error = Ok(());
    }
    crate::thread::cancel::restore_state_raw(kernel, old_cancel);
    if error == Err(crate::limits::ECANCELED) {
        crate::thread::cancel::cancel_point_check(kernel);
        crate::thread::cancel::set_state_raw(kernel, PTHREAD_CANCEL_DISABLE);
    }
    error
}

/// `pthread_cond_clockwait`: a thin function over the timed-wait core (CLEANROOM glue).
///
/// # Errors
///
/// See [`timedwait`].
pub fn clockwait<K: ThreadKernel>(
    kernel: &K,
    cond: &Cond,
    mutex: &crate::mutex::Mutex,
    clock: u32,
    at: Option<Timespec>,
) -> Result<(), Errno> {
    timedwait(kernel, cond, mutex, clock, at)
}

#[cfg(test)]
#[path = "cond_ut.rs"]
mod tests;
