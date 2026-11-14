// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/sigprocmask.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigpending.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `sigprocmask`, `pthread_sigmask` and `sigpending`.
//!
//! musl's `sigprocmask` is a thin wrapper over the thread-aware `pthread_sigmask` and the
//! `rt_sigprocmask` syscall. In ravenlibc the mask is a TCB word (A-sig-1), the internal bits are
//! never set, and after an unblock that makes a signal deliverable the call self-kicks with
//! `CHECK` (PG-7). `sigpending` reports the thread-directed bits; process-directed bits live in
//! procd's shared page (PG-44) and are only consumable through `SigKernel::take_process_pending`.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::signal;

use crate::SigKernel;
use crate::errors::{EINVAL_ERR, number};
use crate::kernel::KICK_CHECK;
use crate::mask;
use crate::raw;
use crate::sigset::{APP_MASK, SigSet};

/// `sigprocmask`.
pub fn sigprocmask<K: SigKernel>(
    kernel: &K,
    how: i32,
    set: Option<&SigSet>,
    old: Option<&mut SigSet>,
) -> Result<(), Errno> {
    let result = pthread_sigmask(kernel, how, set, old);
    if result == 0 {
        Ok(())
    } else {
        Err(crate::errors::errno(result))
    }
}

/// `pthread_sigmask`; returns 0 or a positive error number.
pub fn pthread_sigmask<K: SigKernel>(
    kernel: &K,
    how: i32,
    set: Option<&SigSet>,
    old: Option<&mut SigSet>,
) -> i32 {
    if !matches!(
        how,
        signal::SIG_BLOCK | signal::SIG_UNBLOCK | signal::SIG_SETMASK
    ) {
        return number(EINVAL_ERR);
    }
    let tp = kernel.thread_pointer();
    let previous = raw::mask_load(tp);
    if let Some(out) = old
        && let Some(slot) = out.0.first_mut()
    {
        *slot = previous & APP_MASK;
    }
    let Some(request) = set else {
        return 0;
    };
    let requested = request.app_word() & APP_MASK;
    let mut next = match how {
        signal::SIG_BLOCK => previous | requested,
        signal::SIG_UNBLOCK => previous & !requested,
        _ => requested,
    };
    next &= APP_MASK;
    raw::mask_store(tp, next);
    if mask::deliverable(raw::pending_load(tp), next) != 0 {
        kernel.kick(kernel.thread_handle(), KICK_CHECK).ok();
    }
    0
}

/// `sigpending`: the thread-directed pending bits (process-directed bits are procd's).
pub fn sigpending<K: SigKernel>(kernel: &K, out: &mut SigSet) -> Result<(), Errno> {
    let pending = raw::pending_load(kernel.thread_pointer());
    *out = mask::set_from_word(pending);
    Ok(())
}

#[cfg(test)]
#[path = "maskops_ut.rs"]
mod maskops_ut;
