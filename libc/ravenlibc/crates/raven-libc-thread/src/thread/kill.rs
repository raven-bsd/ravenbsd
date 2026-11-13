// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_kill.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `pthread_kill`: takes `killlock`, tests `tid`, and uses `kick` instead of `tkill`.
//!
//! Without PG-7 the call returns `ENOSYS` (never 0): a silently ignored signal would be worse
//! than an honest failure.

use core::sync::atomic::Ordering;

use crate::kernel::ThreadKernel;
use crate::limits::{EINVAL, ENOSYS, ESRCH, Errno};
use crate::raw;

/// The highest signal number accepted (Linux `_NSIG`).
pub const NSIG: i32 = 65;

/// `pthread_kill`.
///
/// # Errors
///
/// `EINVAL` for a signal number outside `0..NSIG`, `ESRCH` for a stale thread and `ENOSYS`
/// without PG-7.
pub fn kill<K: ThreadKernel>(kernel: &K, thread: usize, signal: i32) -> Result<(), Errno> {
    if thread == 0 {
        return Err(ESRCH);
    }
    let pthread = raw::tcb(thread);
    crate::sync::lock::lock(kernel, &pthread.killlock);
    let tid = pthread.tid.load(Ordering::Acquire);
    let result = if tid > 0 {
        if !kernel.caps().kick {
            Err(ENOSYS)
        } else if !(0..NSIG).contains(&signal) {
            Err(EINVAL)
        } else if signal == 0 {
            Ok(())
        } else {
            kernel.kick(pthread.handle, 1u64 << (signal as u32 - 1))
        }
    } else if !(0..NSIG).contains(&signal) {
        Err(EINVAL)
    } else {
        Err(ESRCH)
    };
    crate::sync::lock::unlock(kernel, &pthread.killlock);
    result
}

/// `pthread_sigqueue`-shaped helper: `ENOSYS` without PG-7.
///
/// # Errors
///
/// Always `ENOSYS`.
pub fn sigqueue(_thread: usize, _signal: i32, _value: usize) -> Result<(), Errno> {
    Err(ENOSYS)
}

#[cfg(test)]
#[path = "kill_ut.rs"]
mod tests;
