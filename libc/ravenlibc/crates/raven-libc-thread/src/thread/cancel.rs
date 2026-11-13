// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_cancel.c, pthread_setcancelstate.c, pthread_setcanceltype.c, pthread_testcancel.c, pthread_cleanup_push.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! Cancellation: one atomic word, one wrapper (`syscall_cp` in C54), one `__testcancel`.
//!
//! Word bits: `ENABLE` (bit 0), `ASYNC` (bit 1), `REQUESTED` (bit 2), `MASKED` (bit 3). A
//! cancellation point acts when `ENABLE && REQUESTED && !MASKED` with one load. Cleanup handlers
//! are an intrusive list on the waiting thread's stack; there is no unwinding and no `Drop`
//! value is live across a cancellation point (T-thr-11).

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::kernel::ThreadKernel;
use crate::limits::{
    CANCEL_ASYNC, CANCEL_ENABLE, CANCEL_MASKED, CANCEL_REQUESTED, EINVAL, ENOSYS, ESRCH, Errno,
    PTHREAD_CANCEL_ASYNCHRONOUS, PTHREAD_CANCEL_DEFERRED, PTHREAD_CANCEL_DISABLE,
    PTHREAD_CANCEL_ENABLE, PTHREAD_CANCEL_MASKED, PTHREAD_CANCELED,
};
use crate::raw;

/// One on-stack cleanup node.
#[repr(C)]
pub struct Cleanup {
    /// Address of the next node (0 at the end).
    pub next: usize,
    /// The function to call with `arg`.
    pub function: fn(usize),
    /// The function argument.
    pub arg: usize,
}

/// Signal bit used by `pthread_cancel` (`SIGCANCEL` = 33, zero-based bit 32).
pub const SIGCANCEL_BIT: u64 = 1 << 32;

/// The cancellation window (`__cp_begin`/`__cp_end`/`__cp_cancel`).
static WINDOW_BEGIN: AtomicUsize = AtomicUsize::new(0);
static WINDOW_END: AtomicUsize = AtomicUsize::new(0);
static WINDOW_CANCEL: AtomicUsize = AtomicUsize::new(0);

/// Install the cancellation window (C54 has no exported labels yet).
pub fn set_cancel_window(begin: usize, end: usize, cancel: usize) {
    WINDOW_BEGIN.store(begin, Ordering::Release);
    WINDOW_END.store(end, Ordering::Release);
    WINDOW_CANCEL.store(cancel, Ordering::Release);
}

/// The installed window.
#[must_use]
pub fn cancel_window() -> (usize, usize, usize) {
    (
        WINDOW_BEGIN.load(Ordering::Acquire),
        WINDOW_END.load(Ordering::Acquire),
        WINDOW_CANCEL.load(Ordering::Acquire),
    )
}

/// The result of a kick for the signal trampoline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KickAction {
    /// Nothing to do; the next cancellation point will check.
    Nothing,
    /// Asynchronous cancellation: exit now.
    CancelNow,
    /// Rewrite the interrupted PC to the cancellation stub.
    RewritePc(usize),
}

/// The signal-frame view C58 passes to [`on_kick`].
pub trait KickContext {
    /// The interrupted program counter.
    fn pc(&self) -> usize;
    /// Rewrite the interrupted program counter.
    fn set_pc(&mut self, pc: usize);
}

fn state_value(word: u32) -> i32 {
    if word & CANCEL_ENABLE == 0 {
        PTHREAD_CANCEL_DISABLE
    } else if word & CANCEL_MASKED != 0 {
        PTHREAD_CANCEL_MASKED
    } else {
        PTHREAD_CANCEL_ENABLE
    }
}

fn encode_state(value: i32, word: &mut u32) -> bool {
    match value {
        PTHREAD_CANCEL_ENABLE => {
            *word |= CANCEL_ENABLE;
            *word &= !CANCEL_MASKED;
            true
        }
        PTHREAD_CANCEL_DISABLE => {
            *word &= !CANCEL_ENABLE;
            true
        }
        PTHREAD_CANCEL_MASKED => {
            *word |= CANCEL_MASKED;
            true
        }
        _ => false,
    }
}

/// Change the cancel state word without validating; returns the old POSIX state.
pub fn set_state_raw<K: ThreadKernel>(kernel: &K, value: i32) -> i32 {
    let pthread = crate::thread::current_pthread(kernel);
    let mut word = pthread.cancel.load(Ordering::Acquire);
    let old = state_value(word);
    if encode_state(value, &mut word) {
        pthread.cancel.store(word, Ordering::Release);
    }
    old
}

/// Restore a word previously returned by [`set_state_raw`].
pub fn restore_state_raw<K: ThreadKernel>(kernel: &K, value: i32) {
    set_state_raw(kernel, value);
}

/// `pthread_setcancelstate`.
///
/// # Errors
///
/// `EINVAL` outside enable/disable/masked.
pub fn setcancelstate<K: ThreadKernel>(kernel: &K, new: i32) -> Result<i32, Errno> {
    if !matches!(
        new,
        PTHREAD_CANCEL_ENABLE | PTHREAD_CANCEL_DISABLE | PTHREAD_CANCEL_MASKED
    ) {
        return Err(EINVAL);
    }
    Ok(set_state_raw(kernel, new))
}

/// `pthread_setcanceltype`.
///
/// # Errors
///
/// `EINVAL` outside deferred/asynchronous.
pub fn setcanceltype<K: ThreadKernel>(kernel: &K, new: i32) -> Result<i32, Errno> {
    if new != PTHREAD_CANCEL_DEFERRED && new != PTHREAD_CANCEL_ASYNCHRONOUS {
        return Err(EINVAL);
    }
    let pthread = crate::thread::current_pthread(kernel);
    let word = pthread.cancel.load(Ordering::Acquire);
    let old = i32::from(word & CANCEL_ASYNC != 0);
    if new == PTHREAD_CANCEL_ASYNCHRONOUS {
        pthread.cancel.fetch_or(CANCEL_ASYNC, Ordering::AcqRel);
    } else {
        pthread.cancel.fetch_and(!CANCEL_ASYNC, Ordering::AcqRel);
    }
    if new == PTHREAD_CANCEL_ASYNCHRONOUS {
        testcancel(kernel);
    }
    Ok(old)
}

/// Whether cancellation is pending and deliverable now.
#[must_use]
pub fn cancellation_pending<K: ThreadKernel>(kernel: &K) -> bool {
    let word = crate::thread::current_pthread(kernel)
        .cancel
        .load(Ordering::Acquire);
    word & CANCEL_ENABLE != 0 && word & CANCEL_REQUESTED != 0 && word & CANCEL_MASKED == 0
}

/// `pthread_testcancel`.
pub fn testcancel<K: ThreadKernel>(kernel: &K) {
    if cancellation_pending(kernel) {
        let pthread = crate::thread::current_pthread(kernel);
        if pthread.cancel.load(Ordering::Acquire) & CANCEL_ASYNC != 0
            || pthread.cancel.load(Ordering::Acquire) & CANCEL_ENABLE != 0
        {
            crate::thread::exit(kernel, PTHREAD_CANCELED);
        }
    }
}

/// A cancellation point check (the same predicate as `pthread_testcancel`).
pub fn cancel_point_check<K: ThreadKernel>(kernel: &K) {
    testcancel(kernel);
}

/// `pthread_cancel`.
///
/// # Errors
///
/// `ESRCH` for a stale thread; `ENOSYS` from the kick when PG-7 is absent (the request is still
/// recorded and acts at the next cancellation point).
pub fn cancel<K: ThreadKernel>(kernel: &K, thread: usize) -> Result<(), Errno> {
    if thread == 0 {
        return Err(ESRCH);
    }
    let pthread = raw::tcb(thread);
    if pthread.tid.load(Ordering::Acquire) < 0 {
        return Err(ESRCH);
    }
    pthread.cancel.fetch_or(CANCEL_REQUESTED, Ordering::AcqRel);
    if thread == crate::thread::join::self_thread(kernel) {
        // Asynchronous cancellation acts now; deferred cancellation waits for the next point.
        if pthread.cancel.load(Ordering::Acquire) & CANCEL_ASYNC != 0 {
            testcancel(kernel);
        }
        return Ok(());
    }
    let handle = pthread.handle;
    if handle == 0 {
        return Ok(());
    }
    match kernel.kick(handle, SIGCANCEL_BIT) {
        Ok(()) => Ok(()),
        Err(error) if error == ENOSYS => Ok(()),
        Err(error) => Err(error),
    }
}

/// The C58 signal trampoline's kick hook.
pub fn on_kick<K: ThreadKernel, C: KickContext>(kernel: &K, context: &mut C) -> KickAction {
    let pthread = crate::thread::current_pthread(kernel);
    let word = pthread.cancel.load(Ordering::Acquire);
    if word & CANCEL_REQUESTED == 0 || word & CANCEL_ENABLE == 0 {
        return KickAction::Nothing;
    }
    if word & CANCEL_ASYNC != 0 {
        return KickAction::CancelNow;
    }
    let (begin, end, cancel) = cancel_window();
    if begin != 0 && context.pc() >= begin && context.pc() < end {
        context.set_pc(cancel);
        return KickAction::RewritePc(cancel);
    }
    KickAction::Nothing
}

/// Push a cleanup node on the calling thread's list.
pub fn push_cleanup<K: ThreadKernel>(kernel: &K, node: &mut Cleanup) {
    let pthread = crate::thread::current_pthread(kernel);
    node.next = pthread.cancelbuf;
    raw::set_cancelbuf(pthread, node as *mut Cleanup as usize);
}

/// Pop the innermost cleanup node; run it when `run` is true.
pub fn pop_cleanup<K: ThreadKernel>(kernel: &K, node: &mut Cleanup, run: bool) {
    let pthread = crate::thread::current_pthread(kernel);
    raw::set_cancelbuf(pthread, node.next);
    if run {
        (node.function)(node.arg);
    }
}

/// Run every cleanup handler of an exiting thread, LIFO.
pub fn run_cleanup<K: ThreadKernel>(kernel: &K) {
    loop {
        let pthread = crate::thread::current_pthread(kernel);
        let node = pthread.cancelbuf;
        if node == 0 {
            return;
        }
        let cleanup = raw::cleanup_node(node);
        raw::set_cancelbuf(pthread, cleanup.next);
        (cleanup.function)(cleanup.arg);
    }
}

#[cfg(test)]
#[path = "cancel_ut.rs"]
mod tests;
