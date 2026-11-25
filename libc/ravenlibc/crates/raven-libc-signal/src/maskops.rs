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
    if kernel.caps().authoritative {
        // The kernel is the authority for the live mask. The TCB word is only the mirror the
        // legacy direct entry path (host fake) reads; keep it in step with the kernel result.
        let requested = set.map(|request| request.app_word() & APP_MASK);
        let previous = match kernel.sigprocmask(how, requested) {
            Ok(previous) => previous,
            Err(error) => return number(error),
        };
        if let Some(out) = old
            && let Some(slot) = out.0.first_mut()
        {
            *slot = previous & APP_MASK;
        }
        let requested = requested.unwrap_or(0);
        let unblockable = crate::critical::signal_bit(signal::SIGKILL)
            | crate::critical::signal_bit(signal::SIGSTOP);
        let next = match how {
            signal::SIG_BLOCK => previous | requested,
            signal::SIG_UNBLOCK => previous & !requested,
            _ => requested,
        };
        raw::mask_store(tp, next & APP_MASK & !unblockable);
        return 0;
    }
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
    // An unblock must re-check the *process* queue too: a signal that arrived while blocked is
    // owned by the kernel, so thread-pending bits alone cannot prove there is nothing to run.
    // The fresh kick is cheap and the entry path treats an empty check as spurious.
    let unblocked = previous & !next & APP_MASK;
    if unblocked != 0 || mask::deliverable(raw::pending_load(tp), next) != 0 {
        kernel.kick(kernel.thread_handle(), KICK_CHECK).ok();
    }
    0
}

/// `sigpending`: the kernel's authoritative pending word for the calling thread.
pub fn sigpending<K: SigKernel>(kernel: &K, out: &mut SigSet) -> Result<(), Errno> {
    let pending = if kernel.caps().authoritative {
        kernel.pending()?
    } else {
        raw::pending_load(kernel.thread_pointer())
    };
    *out = mask::set_from_word(pending);
    Ok(())
}

#[cfg(test)]
#[path = "maskops_ut.rs"]
mod maskops_ut;
