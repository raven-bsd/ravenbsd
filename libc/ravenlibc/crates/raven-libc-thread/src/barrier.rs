// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_barrier_init.c, pthread_barrier_wait.c, pthread_barrier_destroy.c, pthread_barrierattr_setpshared.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! Barriers: a stack-resident instance, a count word and the `vmlock` calls of the donor.
//!
//! `PTHREAD_BARRIER_SERIAL_THREAD` (-1) is returned to exactly one waiter. The `vmlock` calls
//! keep the instance alive while a late waiter may still touch the barrier
//! (`pthread_barrier_wait.c:26`); process-shared barriers are `ENOTSUP`.

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use crate::kernel::ThreadKernel;
use crate::limits::{
    EINVAL, ENOTSUP, Errno, PTHREAD_BARRIER_SERIAL_THREAD, PTHREAD_PROCESS_PRIVATE,
    PTHREAD_PROCESS_SHARED,
};
use crate::raw;
use crate::sched_point;
use crate::sync::ptc::{vm_lock, vm_unlock};
use crate::sync::wait::wait;

/// The per-instance state owned by the first thread entering the barrier.
#[repr(C)]
pub struct Instance {
    /// Threads that have arrived for this round.
    pub count: AtomicI32,
    /// Set to 1 by the last arriver.
    pub last: AtomicI32,
    /// Waiters on `last`.
    pub waiters: AtomicI32,
    /// Number of threads that have left the instance.
    pub finished: AtomicI32,
}

impl Instance {
    /// A zeroed instance.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: AtomicI32::new(0),
            last: AtomicI32::new(0),
            waiters: AtomicI32::new(0),
            finished: AtomicI32::new(0),
        }
    }
}

impl Default for Instance {
    fn default() -> Self {
        Self::new()
    }
}

/// `pthread_barrier_t` (32 bytes).
#[repr(C, align(8))]
pub struct Barrier {
    /// The eight words of the C object.
    pub words: [AtomicU32; 8],
}

const _: () = assert!(core::mem::size_of::<Barrier>() == 32);
const _: () = assert!(core::mem::align_of::<Barrier>() == 8);

impl Barrier {
    /// A statically initialised barrier with limit 0 (a one-thread barrier).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            words: [const { AtomicU32::new(0) }; 8],
        }
    }

    fn lock_word(&self) -> &AtomicI32 {
        raw::as_i32(&self.words[0])
    }

    fn waiters_word(&self) -> &AtomicI32 {
        raw::as_i32(&self.words[1])
    }

    fn limit(&self) -> i32 {
        self.words[2].load(Ordering::Acquire) as i32
    }

    fn instance(&self) -> usize {
        raw::load_usize_atomic(&self.words, 6)
    }

    fn set_instance(&self, value: usize) {
        raw::store_usize_atomic(&self.words, 6, value);
    }
}

impl Default for Barrier {
    fn default() -> Self {
        Self::new()
    }
}

/// `pthread_barrierattr_t` (4 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BarrierAttr {
    /// The private/shared flag.
    pub attr: u32,
}

/// `pthread_barrierattr_init`.
pub fn attr_init() -> BarrierAttr {
    BarrierAttr { attr: 0 }
}

/// `pthread_barrierattr_destroy`.
pub fn attr_destroy(_attr: &BarrierAttr) {}

/// `pthread_barrierattr_setpshared`.
///
/// # Errors
///
/// `EINVAL` outside the two values and `ENOTSUP` for shared without PG-4.
pub fn attr_setpshared<K: ThreadKernel>(
    kernel: &K,
    attr: &mut BarrierAttr,
    pshared: i32,
) -> Result<(), Errno> {
    if pshared != PTHREAD_PROCESS_PRIVATE && pshared != PTHREAD_PROCESS_SHARED {
        return Err(EINVAL);
    }
    if pshared == PTHREAD_PROCESS_SHARED && !kernel.caps().shared_futex {
        return Err(ENOTSUP);
    }
    attr.attr = if pshared == PTHREAD_PROCESS_SHARED {
        i32::MIN as u32
    } else {
        0
    };
    Ok(())
}

/// `pthread_barrierattr_getpshared`.
#[must_use]
pub fn attr_getpshared(attr: &BarrierAttr) -> i32 {
    i32::from(attr.attr != 0)
}

/// `pthread_barrier_init`.
///
/// # Errors
///
/// `EINVAL` for a zero count.
pub fn init(barrier: &mut Barrier, attr: Option<&BarrierAttr>, count: u32) -> Result<(), Errno> {
    if count == 0 {
        return Err(EINVAL);
    }
    let limit = count.wrapping_sub(1) as i32;
    let shared = attr.map_or(0, |attr| attr.attr as i32);
    *barrier = Barrier::new();
    barrier.words[2].store((limit | shared) as u32, Ordering::Release);
    Ok(())
}

/// `pthread_barrier_destroy`.
pub fn destroy<K: ThreadKernel>(kernel: &K, barrier: &Barrier) -> Result<(), Errno> {
    if barrier.limit() < 0 {
        if barrier.lock_word().load(Ordering::Acquire) != 0 {
            barrier.lock_word().fetch_or(i32::MIN, Ordering::AcqRel);
            loop {
                let value = barrier.lock_word().load(Ordering::Acquire);
                if value & i32::MAX == 0 {
                    break;
                }
                wait(kernel, barrier.lock_word(), None, value, false);
            }
        }
        crate::sync::ptc::vm_wait(kernel);
    }
    Ok(())
}

fn recursive_unlock<K: ThreadKernel>(kernel: &K, barrier: &Barrier) {
    loop {
        let value = barrier.lock_word().load(Ordering::Acquire);
        let waiters = barrier.waiters_word().load(Ordering::Acquire);
        let new = if value == i32::MIN + 1 { 0 } else { value - 1 };
        if barrier
            .lock_word()
            .compare_exchange(value, new, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            if value == i32::MIN + 1 || (value == 1 && waiters != 0) {
                kernel.futex_wake(raw::as_u32(barrier.lock_word()), 1);
            }
            return;
        }
    }
}

/// `pthread_barrier_wait`.
///
/// # Errors
///
/// `ENOTSUP` for a process-shared barrier.
pub fn wait_barrier<K: ThreadKernel>(kernel: &K, barrier: &Barrier) -> Result<i32, Errno> {
    let limit = barrier.limit();
    if limit == 0 {
        return Ok(PTHREAD_BARRIER_SERIAL_THREAD);
    }
    if limit < 0 {
        return Err(ENOTSUP);
    }
    while barrier.lock_word().swap(1, Ordering::AcqRel) != 0 {
        wait(
            kernel,
            barrier.lock_word(),
            Some(barrier.waiters_word()),
            1,
            true,
        );
    }
    let instance_address = barrier.instance();
    if instance_address == 0 {
        let instance = Instance::new();
        let address = &instance as *const Instance as usize;
        barrier.set_instance(address);
        barrier.lock_word().store(0, Ordering::Release);
        if barrier.waiters_word().load(Ordering::Acquire) != 0 {
            kernel.futex_wake(raw::as_u32(barrier.lock_word()), 1);
        }
        let mut spins = 200u32;
        while spins > 0 && instance.finished.load(Ordering::Acquire) == 0 {
            core::hint::spin_loop();
            spins -= 1;
        }
        instance.finished.fetch_add(1, Ordering::AcqRel);
        while instance.finished.load(Ordering::Acquire) == 1 {
            let _ = kernel.futex_wait(raw::as_u32(&instance.finished), 1, None);
        }
        sched_point!(kernel, "barrier_handover");
        return Ok(PTHREAD_BARRIER_SERIAL_THREAD);
    }
    let instance = raw::barrier_instance(instance_address);
    let arrived = instance
        .count
        .fetch_add(1, Ordering::AcqRel)
        .wrapping_add(1);
    if arrived == limit {
        barrier.set_instance(0);
        barrier.lock_word().store(0, Ordering::Release);
        if barrier.waiters_word().load(Ordering::Acquire) != 0 {
            kernel.futex_wake(raw::as_u32(barrier.lock_word()), 1);
        }
        instance.last.store(1, Ordering::Release);
        if instance.waiters.load(Ordering::Acquire) != 0 {
            kernel.futex_wake(raw::as_u32(&instance.last), u32::MAX);
        }
    } else {
        barrier.lock_word().store(0, Ordering::Release);
        if barrier.waiters_word().load(Ordering::Acquire) != 0 {
            kernel.futex_wake(raw::as_u32(barrier.lock_word()), 1);
        }
        wait(kernel, &instance.last, Some(&instance.waiters), 0, true);
    }
    sched_point!(kernel, "barrier_handover");
    if instance.count.fetch_add(-1, Ordering::AcqRel) == 1
        && instance.finished.fetch_add(1, Ordering::AcqRel) != 0
    {
        kernel.futex_wake(raw::as_u32(&instance.finished), 1);
    }
    vm_lock();
    vm_unlock(kernel);
    recursive_unlock(kernel, barrier);
    Ok(0)
}

/// The current count of the barrier instance (tests).
#[must_use]
pub fn instance_count(barrier: &Barrier) -> i32 {
    let address = barrier.instance();
    if address == 0 {
        0
    } else {
        raw::barrier_instance(address).count.load(Ordering::Acquire)
    }
}

#[cfg(test)]
#[path = "barrier_ut.rs"]
mod tests;
