// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/sigsuspend.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigwait.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigwaitinfo.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigtimedwait.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! The wait family: `sigsuspend`, `sigwait`, `sigwaitinfo`, `sigtimedwait`.
//!
//! `sigsuspend` is atomic: the temporary mask and the pending check are inside one critical
//! section and the sleep is entered only after leaving it, with the sticky kick closing the
//! window (PG-8; the mlibc three-call form is forbidden, A-sol-21). `sigtimedwait` removes the
//! signal from the pending set without running its handler, fills `siginfo` and maps a timeout
//! to `EAGAIN`. Both are cancellation points through the `__testcancel` hook (A-thr-6).

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::signal;
use raven_libc_abi::structs::{SiginfoT, Timespec};

use crate::errors::{EAGAIN_ERR, EINTR_ERR, EINVAL_ERR, ETIMEDOUT_ERR};
use crate::frame::user_siginfo;
use crate::kernel::KICK_CHECK;
use crate::mask;
use crate::raw;
use crate::sigset::{APP_MASK, SigSet};
use crate::{DispTable, SigKernel};

/// Validate a timeout the way POSIX does (`tv_nsec` in range).
fn valid_timeout(timeout: &Timespec) -> bool {
    timeout.tv_sec >= 0 && (0..1_000_000_000).contains(&timeout.tv_nsec)
}

/// `sigsuspend`: install the temporary mask and wait atomically, then run the handler.
pub fn sigsuspend<K: SigKernel>(kernel: &K, table: &DispTable, set: &SigSet) -> Errno {
    raw::call_testcancel_hook();
    let tp = kernel.thread_pointer();
    if kernel.caps().authoritative {
        // The kernel `SIGWAIT` operation installs its temporary mask and checks the queue under
        // one lock, so a signal arriving at the mask change cannot be lost. Block everything
        // first, then wait for the complement of the requested mask; the signal SIGWAIT returns
        // is re-queued thread-directed while still blocked and re-exposed under the temporary
        // mask, so its handler runs before this call returns (FreeBSD `kern_sigsuspend`).
        let temporary = set.app_word() & APP_MASK;
        let old = match kernel.sigprocmask(
            raven_libc_abi::generated::signal::SIG_SETMASK,
            Some(APP_MASK),
        ) {
            Ok(old) => old,
            Err(error) => return error,
        };
        raw::mask_store(tp, APP_MASK);
        let unblockable = crate::critical::signal_bit(raven_libc_abi::generated::signal::SIGKILL)
            | crate::critical::signal_bit(raven_libc_abi::generated::signal::SIGSTOP);
        let wait_set = APP_MASK & !temporary & !unblockable;
        let result = match kernel.wait_signal(wait_set, 0, false) {
            Ok((sig, record)) => {
                let queued = kernel.thread_sigqueue(crate::kernel::HANDLE_NONE, sig, record[4]);
                let exposed = queued.and_then(|()| {
                    kernel.sigprocmask(
                        raven_libc_abi::generated::signal::SIG_SETMASK,
                        Some(temporary),
                    )
                });
                let restored =
                    kernel.sigprocmask(raven_libc_abi::generated::signal::SIG_SETMASK, Some(old));
                exposed.and(restored).map(|_| ())
            }
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            kernel
                .sigprocmask(raven_libc_abi::generated::signal::SIG_SETMASK, Some(old))
                .ok();
            raw::mask_store(tp, old);
            return error;
        }
        raw::mask_store(tp, old);
        return EINTR_ERR;
    }
    crate::critical::critical_enter(kernel);
    let old = raw::mask_swap(tp, set.app_word() & APP_MASK);
    let pending = raw::pending_load(tp);
    let ready = mask::deliverable(pending, raw::mask_load(tp)) != 0;
    // Leave the critical section before running the entry path: the handler must not run with
    // the critical counter held (A-sol-20), and a deferred replay from `critical_exit` is
    // handled by the same rule.
    crate::critical::critical_exit(kernel, table);
    if ready {
        crate::entry::run_direct(kernel, table, KICK_CHECK, None);
        raw::mask_store(tp, old);
        return EINTR_ERR;
    }
    match kernel.wait_for_kick(None) {
        Ok(()) => {
            crate::entry::run_direct(kernel, table, KICK_CHECK, None);
            raw::mask_store(tp, old);
            EINTR_ERR
        }
        Err(error) => {
            raw::mask_store(tp, old);
            error
        }
    }
}

/// `sigtimedwait`: consume a pending signal in `set` without calling its handler.
pub fn sigtimedwait<K: SigKernel>(
    kernel: &K,
    set: &SigSet,
    mut info: Option<&mut SiginfoT>,
    timeout: Option<Timespec>,
) -> Result<i32, Errno> {
    if let Some(value) = timeout.as_ref()
        && !valid_timeout(value)
    {
        return Err(EINVAL_ERR);
    }
    raw::call_testcancel_hook();
    let tp = kernel.thread_pointer();
    let wanted = set.app_word() & APP_MASK;
    let poll_only = timeout
        .as_ref()
        .is_some_and(|value| value.tv_sec == 0 && value.tv_nsec == 0);
    if kernel.caps().authoritative {
        // The kernel consumes the record under its temporary mask, so the pending bit and the
        // mask move together and libc's mirror is not consulted.
        let timeout_ns = timeout.as_ref().map_or(0, |value| {
            (value.tv_sec.max(0) as u64)
                .saturating_mul(1_000_000_000)
                .saturating_add(value.tv_nsec.max(0) as u64)
        });
        let (sig, record) = kernel.wait_signal(wanted, timeout_ns, poll_only)?;
        if let Some(out) = info {
            *out = crate::kernel::process_siginfo(&record).1;
        }
        return Ok(sig);
    }
    let deadline = timeout.as_ref().and_then(|value| {
        if poll_only {
            None
        } else {
            Some(
                kernel
                    .now_ns()
                    .saturating_add((value.tv_sec.max(0) as u64).saturating_mul(1_000_000_000))
                    .saturating_add(value.tv_nsec.max(0) as u64),
            )
        }
    });
    loop {
        let pending = raw::pending_load(tp) & wanted;
        if let Some(sig) = lowest(pending) {
            let bit = crate::critical::signal_bit(sig);
            raw::pending_and(tp, !bit);
            let mut record = raw::load_pending_info(tp);
            if record.si_signo != sig {
                record = user_siginfo(sig, signal::SI_USER);
            }
            if let Some(out) = info.as_deref_mut() {
                *out = record;
            }
            return Ok(sig);
        }
        if let Some((sig, record)) = kernel.take_process_pending(wanted) {
            if let Some(out) = info.as_deref_mut() {
                *out = record;
            }
            return Ok(sig);
        }
        if poll_only {
            return Err(EAGAIN_ERR);
        }
        match kernel.wait_for_kick(deadline) {
            Ok(()) => {}
            Err(error) if error == ETIMEDOUT_ERR => return Err(EAGAIN_ERR),
            Err(error) => return Err(error),
        }
    }
}

/// `sigwait`: number only.
pub fn sigwait<K: SigKernel>(kernel: &K, set: &SigSet) -> Result<i32, Errno> {
    sigtimedwait(kernel, set, None, None)
}

/// `sigwaitinfo`: number and record.
pub fn sigwaitinfo<K: SigKernel>(
    kernel: &K,
    set: &SigSet,
    info: &mut SiginfoT,
) -> Result<i32, Errno> {
    sigtimedwait(kernel, set, Some(info), None)
}

fn lowest(bits: u64) -> Option<i32> {
    if bits == 0 {
        None
    } else {
        Some((bits.trailing_zeros() as i32).wrapping_add(1))
    }
}

/// The documented `EINTR` result of a handler that ran during a wait.
#[must_use]
pub const fn interrupted() -> Errno {
    EINTR_ERR
}

#[cfg(test)]
#[path = "wait_ut.rs"]
mod wait_ut;
