// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `signal(3)`.
use core::ffi::c_int;
use core::ptr;

use raven_libc_abi::structs::{Sigaction, SigsetT};
use raven_libc_signal::dispositions::SA_RESTART;

/// Install a handler with BSD reliable semantics.
///
/// FreeBSD's `signal` builds a `struct sigaction` with `SA_RESTART` unless `siginterrupt` cleared
/// it and calls `__libc_sigaction`; going through the same public path here means the kernel
/// disposition and libc's legacy getter mirror are the same action.
#[unsafe(no_mangle)]
pub extern "C" fn signal(sig: c_int, handler: usize) -> usize {
    let action = Sigaction {
        sa_handler: handler as *mut core::ffi::c_void,
        sa_mask: SigsetT::ZERO,
        sa_flags: SA_RESTART as i32,
        sa_restorer: ptr::null_mut(),
    };
    let mut old = Sigaction::ZERO;
    // SAFETY: both objects are live for the call; `sigaction` copies in/out and retains neither.
    if unsafe { super::sigaction::sigaction(sig, &action, &mut old) } != 0 {
        return usize::MAX;
    }
    old.sa_handler as usize
}
