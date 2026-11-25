// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigignore(3)`.
use core::ffi::c_int;
use core::ptr;

use raven_libc_abi::structs::{Sigaction, SigsetT};

/// Ignore `sig` through the authoritative `sigaction` path.
#[unsafe(no_mangle)]
pub extern "C" fn sigignore(sig: c_int) -> c_int {
    let action = Sigaction {
        sa_handler: 1usize as *mut core::ffi::c_void,
        sa_mask: SigsetT::ZERO,
        sa_flags: 0,
        sa_restorer: ptr::null_mut(),
    };
    // SAFETY: `action` is live for the call and no old action is requested.
    unsafe { super::sigaction::sigaction(sig, &action, ptr::null_mut()) }
}
