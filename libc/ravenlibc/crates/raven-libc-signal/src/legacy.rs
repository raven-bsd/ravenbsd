// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/signal.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigpause.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sighold.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigrelse.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigignore.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigset.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/siginterrupt.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigrtmin.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigrtmax.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! The legacy BSD/SysV calls.
//!
//! `signal()` is BSD reliable semantics and never one-shot (A-sig-3): the donor installs
//! `{handler, SA_RESTART}` and does not reset; `bsd_signal` and `sysv_signal` are the same body.
//! `sigrtmin` returns 35 and `sigrtmax` 64 (A-sig-2). `sigset`/`sighold`/`sigrelse`/`sigignore`/
//! `sigpause`/`siginterrupt` are the musl bodies over the mask word and `sigsuspend`.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::signal;

use crate::SigKernel;
use crate::action::sigaction;
use crate::dispositions::{DispTable, Disposition, SA_RESTART};
use crate::errors::EINVAL_ERR;
use crate::maskops::{pthread_sigmask, sigprocmask};
use crate::sigset::{SIGRTMAX, SIGRTMIN, SigSet};
use crate::wait::sigsuspend;

/// `SIG_HOLD` (`(void (*)(int))2`).
pub const SIG_HOLD: usize = 2;

/// `signal`: install `handler` with `SA_RESTART`, never reset it.
pub fn signal(table: &DispTable, sig: i32, handler: usize) -> Result<usize, Errno> {
    let action = Disposition {
        handler,
        mask: 0,
        flags: SA_RESTART,
    };
    let mut old = Disposition::DEFAULT;
    sigaction(table, sig, Some(&action), Some(&mut old))?;
    Ok(old.handler)
}

/// `sighold`: add `sig` to the mask.
pub fn sighold<K: SigKernel>(kernel: &K, sig: i32) -> Result<(), Errno> {
    let mut set = SigSet::empty();
    set.add(sig)?;
    sigprocmask(kernel, signal::SIG_BLOCK, Some(&set), None)
}

/// `sigrelse`: remove `sig` from the mask.
pub fn sigrelse<K: SigKernel>(kernel: &K, sig: i32) -> Result<(), Errno> {
    let mut set = SigSet::empty();
    set.add(sig)?;
    sigprocmask(kernel, signal::SIG_UNBLOCK, Some(&set), None)
}

/// `sigignore`: install `SIG_IGN` with no flags.
pub fn sigignore(table: &DispTable, sig: i32) -> Result<(), Errno> {
    sigaction(table, sig, Some(&Disposition::IGNORE), None)
}

/// `sigset`: `SIG_HOLD` blocks the signal; anything else installs and unblocks it.
pub fn sigset<K: SigKernel>(
    kernel: &K,
    table: &DispTable,
    sig: i32,
    handler: usize,
) -> Result<usize, Errno> {
    let mut set = SigSet::empty();
    set.add(sig)?;
    if handler == SIG_HOLD {
        let mut old = Disposition::DEFAULT;
        sigaction(table, sig, None, Some(&mut old))?;
        let mut old_mask = SigSet::empty();
        sigprocmask(kernel, signal::SIG_BLOCK, Some(&set), Some(&mut old_mask))?;
        return Ok(if old_mask.is_member(sig)? {
            SIG_HOLD
        } else {
            old.handler
        });
    }
    let action = Disposition {
        handler,
        mask: 0,
        flags: 0,
    };
    let mut old = Disposition::DEFAULT;
    sigaction(table, sig, Some(&action), Some(&mut old))?;
    let mut old_mask = SigSet::empty();
    sigprocmask(kernel, signal::SIG_UNBLOCK, Some(&set), Some(&mut old_mask))?;
    Ok(if old_mask.is_member(sig)? {
        SIG_HOLD
    } else {
        old.handler
    })
}

/// `sigpause`: remove `sig` from the caller's mask and suspend.
pub fn sigpause<K: SigKernel>(kernel: &K, table: &DispTable, sig: i32) -> Errno {
    let mut mask = SigSet::empty();
    if let Some(slot) = mask.0.first_mut() {
        *slot = crate::mask::current_mask(kernel);
    }
    if mask.del(sig).is_err() {
        return EINVAL_ERR;
    }
    sigsuspend(kernel, table, &mask)
}

/// `siginterrupt`: toggle `SA_RESTART`.
pub fn siginterrupt(table: &DispTable, sig: i32, flag: i32) -> Result<(), Errno> {
    let mut action = Disposition::DEFAULT;
    sigaction(table, sig, None, Some(&mut action))?;
    if flag != 0 {
        action.flags &= !SA_RESTART;
    } else {
        action.flags |= SA_RESTART;
    }
    sigaction(table, sig, Some(&action), None)
}

/// `__libc_current_sigrtmin` is 35 in ravenlibc.
#[must_use]
pub const fn sigrtmin() -> i32 {
    SIGRTMIN
}

/// `__libc_current_sigrtmax` is 64 in ravenlibc.
#[must_use]
pub const fn sigrtmax() -> i32 {
    SIGRTMAX
}

/// `pthread_sigmask` as a legacy error-number call (the shell's export shape).
#[must_use]
pub fn pthread_sigmask_number<K: SigKernel>(
    kernel: &K,
    how: i32,
    set: Option<&SigSet>,
    old: Option<&mut SigSet>,
) -> i32 {
    pthread_sigmask(kernel, how, set, old)
}

#[cfg(test)]
#[path = "legacy_ut.rs"]
mod legacy_ut;
