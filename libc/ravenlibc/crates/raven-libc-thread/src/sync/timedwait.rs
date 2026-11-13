// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/__timedwait.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `__timedwait_cp` / `__timedwait`.
//!
//! The time64 ladder of the donor is deleted. A deadline is converted against the caller's
//! clock; a deadline that has already passed returns `ETIMEDOUT` before any kernel call
//! (A-thr-5, T-thr-5). The kernel is never given `timeout_ns == 0` for a deadline: `None` means
//! forever, and [`relative_ns`] never returns zero. Only `EINTR`, `ETIMEDOUT` and `ECANCELED`
//! pass; any other futex error is a spurious wake and becomes success.

use core::sync::atomic::AtomicI32;

use crate::kernel::ThreadKernel;
use crate::limits::{
    ECANCELED, EINTR, EINVAL, ETIMEDOUT, Errno, Timespec, relative_ns, timespec_to_ns,
};

/// `__timedwait_cp`: the cancellation-point-aware core.
///
/// # Errors
///
/// `EINVAL` for a malformed timespec, `ETIMEDOUT` for a passed deadline, and the kernel's
/// `EINTR`/`ECANCELED`.
pub fn timedwait_cp<K: ThreadKernel>(
    kernel: &K,
    word: &AtomicI32,
    value: i32,
    clock: u32,
    at: Option<Timespec>,
) -> Result<(), Errno> {
    let relative = match at {
        Some(ts) => {
            let deadline = timespec_to_ns(ts).ok_or(EINVAL)?;
            let now = kernel.now_ns(clock);
            match relative_ns(now, deadline) {
                Some(ns) => Some(ns),
                None => return Err(ETIMEDOUT),
            }
        }
        None => None,
    };
    let result = kernel.futex_wait(crate::raw::as_u32(word), value as u32, relative);
    match result {
        Ok(()) => Ok(()),
        Err(error) if error == EINTR => {
            if kernel.caps().eintr {
                Err(EINTR)
            } else {
                // T-thr-6: without PG-8 a futex wait is not interruptible; treat the spurious
                // `EINTR` exactly as the donor's `__eintr_valid_flag == 0` path does.
                Ok(())
            }
        }
        Err(error) if error == ETIMEDOUT || error == ECANCELED => Err(error),
        Err(_) => Ok(()),
    }
}

/// `__timedwait`: `__timedwait_cp` with cancellation disabled across the call.
///
/// # Errors
///
/// See [`timedwait_cp`].
pub fn timedwait<K: ThreadKernel>(
    kernel: &K,
    word: &AtomicI32,
    value: i32,
    clock: u32,
    at: Option<Timespec>,
    _private: bool,
) -> Result<(), Errno> {
    let old = crate::thread::cancel::set_state_raw(kernel, crate::limits::PTHREAD_CANCEL_DISABLE);
    let result = timedwait_cp(kernel, word, value, clock, at);
    crate::thread::cancel::restore_state_raw(kernel, old);
    result
}

#[cfg(test)]
#[path = "timedwait_ut.rs"]
mod tests;
