// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `siginterrupt(3)`.
use core::ffi::c_int;
use core::ptr;

use raven_libc_abi::structs::Sigaction;
use raven_libc_signal::dispositions::SA_RESTART;

/// Set the `SA_RESTART` policy for `sig` on the kernel's action, preserving handler and mask.
#[unsafe(no_mangle)]
pub extern "C" fn siginterrupt(sig: c_int, flag: c_int) -> c_int {
    let mut action = Sigaction::ZERO;
    // SAFETY: `action` is a live writable C action and no new action is supplied.
    if unsafe { super::sigaction::sigaction(sig, ptr::null(), &mut action) } != 0 {
        return -1;
    }
    if flag != 0 {
        action.sa_flags &= !(SA_RESTART as i32);
    } else {
        action.sa_flags |= SA_RESTART as i32;
    }
    // SAFETY: the queried action is live and no old action is requested.
    unsafe { super::sigaction::sigaction(sig, &action, ptr::null_mut()) }
}
