// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigset(3)`.
use core::ffi::c_int;
use core::ptr;

use raven_libc_abi::generated::signal;
use raven_libc_abi::structs::{Sigaction, SigsetT};
use raven_libc_signal::SigSet;

/// `SIG_HOLD` (`(void (*)(int))2`), the SysV hold sentinel.
const SIG_HOLD: usize = 2;

/// Hold, install or unblock `sig` through the authoritative kernel action and mask.
#[unsafe(no_mangle)]
pub extern "C" fn sigset(sig: c_int, handler: usize) -> usize {
    let mut set = SigSet::empty();
    if set.add(sig).is_err() {
        return usize::MAX;
    }
    if handler == SIG_HOLD {
        let mut old = Sigaction::ZERO;
        // SAFETY: `old` is a live writable C action; no new action is supplied.
        if unsafe { super::sigaction::sigaction(sig, ptr::null(), &mut old) } != 0 {
            return usize::MAX;
        }
        let mut old_mask = SigSet::empty();
        // SAFETY: `set`/`old_mask` are live public sigset_t objects.
        if unsafe { super::sigprocmask::sigprocmask(signal::SIG_BLOCK, &set, &mut old_mask) } != 0 {
            return usize::MAX;
        }
        return if old_mask.is_member(sig).unwrap_or(false) {
            SIG_HOLD
        } else {
            old.sa_handler as usize
        };
    }
    let action = Sigaction {
        sa_handler: handler as *mut core::ffi::c_void,
        sa_mask: SigsetT::ZERO,
        sa_flags: 0,
        sa_restorer: ptr::null_mut(),
    };
    let mut old = Sigaction::ZERO;
    // SAFETY: `action`/`old` are live for the call.
    if unsafe { super::sigaction::sigaction(sig, &action, &mut old) } != 0 {
        return usize::MAX;
    }
    let mut old_mask = SigSet::empty();
    // SAFETY: `set`/`old_mask` are live public sigset_t objects.
    if unsafe { super::sigprocmask::sigprocmask(signal::SIG_UNBLOCK, &set, &mut old_mask) } != 0 {
        return usize::MAX;
    }
    if old_mask.is_member(sig).unwrap_or(false) {
        SIG_HOLD
    } else {
        old.sa_handler as usize
    }
}
