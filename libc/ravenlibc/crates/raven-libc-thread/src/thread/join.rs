// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_join.c, pthread_detach.c, pthread_self.c, pthread_equal.c, pthread_getcpuclockid.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `pthread_join`, `pthread_detach`, `pthread_self`, `pthread_equal`.
//!
//! The joiner waits for `exit_word` set by the exiting thread, then for the kernel to confirm
//! that the thread has finished its last user instruction (`thread_is_dead`), and only then
//! unmaps the stack. Never on a user-space flag alone (A-thr-3).

use core::sync::atomic::Ordering;

use crate::kernel::ThreadKernel;
use crate::limits::{EBUSY, EINVAL, ENOTSUP, ESRCH, Errno, PTHREAD_CANCEL_DISABLE, Timespec};
use crate::raw;
use crate::sched_point;
use crate::thread::{list, stack};

fn stale<K: ThreadKernel>(kernel: &K, tcb: usize) -> bool {
    let pthread = raw::tcb(tcb);
    let tid = pthread.tid.load(Ordering::Acquire);
    if tid < 0 {
        return true;
    }
    if tid == 0 {
        let state = pthread.detach_state.load(Ordering::Acquire);
        return state != raven_libc_tls::tcb::DT_EXITING && state != raven_libc_tls::tcb::DT_EXITED;
    }
    let _ = kernel;
    false
}

/// `pthread_join`.
///
/// # Errors
///
/// `ESRCH` for a stale thread, `EINVAL` for a detached one, `ETIMEDOUT` from a timed variant.
pub fn join<K: ThreadKernel>(kernel: &K, thread: usize) -> Result<usize, Errno> {
    timedjoin(kernel, thread, None)
}

/// `pthread_timedjoin_np`.
///
/// # Errors
///
/// See [`join`].
pub fn timedjoin<K: ThreadKernel>(
    kernel: &K,
    thread: usize,
    at: Option<Timespec>,
) -> Result<usize, Errno> {
    if thread == 0 || stale(kernel, thread) {
        return Err(ESRCH);
    }
    if thread == crate::thread::current_pthread(kernel) as *const raven_libc_tls::Pthread as usize {
        return Err(crate::limits::EDEADLK);
    }
    crate::thread::cancel::cancel_point_check(kernel);
    let pthread = raw::tcb(thread);
    let old = crate::thread::cancel::set_state_raw(kernel, PTHREAD_CANCEL_DISABLE);
    let mut outcome = Ok(());
    loop {
        let state = pthread.detach_state.load(Ordering::Acquire);
        if state == raven_libc_tls::tcb::DT_DETACHED {
            outcome = Err(EINVAL);
            break;
        }
        if pthread.exit_word.load(Ordering::Acquire) == 1 {
            break;
        }
        if state == raven_libc_tls::tcb::DT_EXITED {
            break;
        }
        outcome = crate::sync::timedwait::timedwait_cp(
            kernel,
            &pthread.exit_word,
            0,
            crate::limits::CLOCK_REALTIME,
            at,
        );
        if let Err(error) = outcome
            && error != crate::limits::EINTR
        {
            break;
        }
    }
    crate::thread::cancel::restore_state_raw(kernel, old);
    outcome?;
    // The kernel must confirm death before the stack is unmapped.
    let mut spins = 0u32;
    while !kernel.thread_is_dead(thread) {
        kernel.yield_now();
        spins = spins.wrapping_add(1);
        if spins > 1_000_000 {
            return Err(EINVAL);
        }
    }
    sched_point!(kernel, "join_before_unmap");
    list::sync(kernel);
    stack::reap(kernel);
    let result = pthread.result;
    if pthread.map_base != 0 && pthread.map_size != 0 {
        let allocation = stack::StackAlloc {
            map_base: pthread.map_base,
            map_size: pthread.map_size,
            guard: pthread.guard_size,
            stack_limit: pthread.stack_base,
            stack_top: pthread.stack_base.wrapping_add(pthread.stack_size),
            tls_base: 0,
            tsd_base: 0,
            tls_size: 0,
            tsd_size: 0,
        };
        let _ = stack::release(kernel, allocation);
    }
    Ok(result)
}

/// `pthread_tryjoin_np`.
///
/// # Errors
///
/// `EBUSY` when the thread is still joinable and running.
pub fn tryjoin<K: ThreadKernel>(kernel: &K, thread: usize) -> Result<usize, Errno> {
    if thread == 0 || stale(kernel, thread) {
        return Err(ESRCH);
    }
    if raw::tcb(thread).detach_state.load(Ordering::Acquire) == raven_libc_tls::tcb::DT_JOINABLE {
        return Err(EBUSY);
    }
    join(kernel, thread)
}

/// `pthread_detach`.
///
/// # Errors
///
/// `EINVAL` when the thread is already detached or exiting, `ESRCH` for a stale handle.
pub fn detach<K: ThreadKernel>(kernel: &K, thread: usize) -> Result<(), Errno> {
    if thread == 0 || stale(kernel, thread) {
        return Err(ESRCH);
    }
    let pthread = raw::tcb(thread);
    match pthread.detach_state.compare_exchange(
        raven_libc_tls::tcb::DT_JOINABLE,
        raven_libc_tls::tcb::DT_DETACHED,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => Ok(()),
        Err(_) => Err(EINVAL),
    }
}

/// `pthread_self`.
#[must_use]
pub fn self_thread<K: ThreadKernel>(kernel: &K) -> usize {
    crate::thread::current_pthread(kernel) as *const raven_libc_tls::Pthread as usize
}

/// `pthread_equal`.
#[must_use]
pub fn equal(a: usize, b: usize) -> bool {
    a == b
}

/// `pthread_getcpuclockid`.
///
/// # Errors
///
/// Always `ENOTSUP` (PG-18).
pub fn getcpuclockid(_thread: usize) -> Result<u32, Errno> {
    Err(ENOTSUP)
}

#[cfg(test)]
#[path = "join_ut.rs"]
mod tests;
