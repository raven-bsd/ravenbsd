// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/raise.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/kill.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/killpg.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigqueue.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `raise`, `kill`, `killpg` and `sigqueue`.
//!
//! `raise` is thread-directed and never goes through procd (A-sig-4): it sets the calling
//! thread's pending bit and, when the signal is unmasked, runs the entry logic directly, so the
//! handler has run before `raise` returns. `kill`/`killpg`/`sigqueue` are procd RPCs (PG-44);
//! without `caps.proc_kill` they are `ENOSYS`, while `raise` still works. `si_pid`/`si_uid` of a
//! `sigqueue` record are stamped by procd (client identity, PG-27), so this crate leaves them 0.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::signal;
use raven_libc_abi::structs::Sigval;

use crate::errors::{EINVAL_ERR, ENOSYS_ERR};
use crate::frame::{queue_siginfo, user_siginfo};
use crate::kernel::KICK_CHECK;
use crate::mask;
use crate::raw;
use crate::sigset::APP_MASK;
use crate::{DispTable, SigKernel};

/// Reject signals an application may not name (0 is handled by the caller).
fn validate_signal(sig: i32) -> Result<(), Errno> {
    if !(1..=64).contains(&sig) || (32..=34).contains(&sig) {
        return Err(EINVAL_ERR);
    }
    Ok(())
}

/// `raise`: deliver to the calling thread, directly when it is unmasked.
pub fn raise<K: SigKernel>(kernel: &K, table: &DispTable, sig: i32) -> Result<(), Errno> {
    validate_signal(sig)?;
    let tp = kernel.thread_pointer();
    let bit = crate::critical::signal_bit(sig);
    raw::pending_or(tp, bit);
    let info = user_siginfo(sig, signal::SI_TKILL);
    raw::store_pending_info(tp, &info);
    let mask = raw::mask_load(tp);
    if mask & bit == 0 {
        crate::entry::run_direct(kernel, table, KICK_CHECK, None);
    }
    Ok(())
}

/// `kill`: procd forwards to `pid`; `sig == 0` is an existence/permission check.
pub fn kill<K: SigKernel>(kernel: &K, pid: i32, sig: i32) -> Result<(), Errno> {
    if sig != 0 {
        validate_signal(sig)?;
    }
    if !kernel.caps().proc_kill {
        return Err(ENOSYS_ERR);
    }
    kernel.proc_kill(pid, sig, None)
}

/// `killpg`: the donor's negative-group check, then `kill(-pgrp, sig)`.
pub fn killpg<K: SigKernel>(kernel: &K, pgrp: i32, sig: i32) -> Result<(), Errno> {
    if pgrp <= 0 {
        return Err(EINVAL_ERR);
    }
    if sig != 0 {
        validate_signal(sig)?;
    }
    if !kernel.caps().proc_kill {
        return Err(ENOSYS_ERR);
    }
    kernel.proc_kill(pgrp.wrapping_neg(), sig, None)
}

/// `sigqueue`: a `SI_QUEUE` record for procd's queue.
pub fn sigqueue<K: SigKernel>(kernel: &K, pid: i32, sig: i32, value: Sigval) -> Result<(), Errno> {
    validate_signal(sig)?;
    if !kernel.caps().proc_kill {
        return Err(ENOSYS_ERR);
    }
    let info = queue_siginfo(sig, value);
    kernel.proc_kill(pid, sig, Some(&info))
}

/// The mask word an application may use (helper for the shell).
#[must_use]
pub const fn application_mask(word: u64) -> u64 {
    word & APP_MASK
}

/// Whether `mask` blocks `sig` (helper reused by tests).
#[must_use]
pub fn blocked(mask: u64, sig: i32) -> bool {
    mask::is_blocked(mask, sig)
}

#[cfg(test)]
#[path = "send_ut.rs"]
mod send_ut;
