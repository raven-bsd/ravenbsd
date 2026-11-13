// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the getusershell export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `getusershell` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::usershell_state::{SHELL_BUF, SHELLS, init};

/// `getusershell`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getusershell() -> *mut c_char {
    // SAFETY: the static is private; the C API is not re-entrant.
    let slot = unsafe { &raw mut SHELLS };
    if unsafe { (*slot).is_none() } {
        unsafe { init() };
    }
    let Some(shells) = (unsafe { (*slot).as_mut() }) else {
        return core::ptr::null_mut();
    };
    let Some(shell) = shells.next() else {
        return core::ptr::null_mut();
    };
    // SAFETY: SHELL_BUF is private and larger than any shell line.
    let buffer = unsafe { &mut *(&raw mut SHELL_BUF) };
    if shell.len().saturating_add(1) > buffer.len() {
        return core::ptr::null_mut();
    }
    buffer[..shell.len()].copy_from_slice(shell);
    buffer[shell.len()] = 0;
    buffer.as_mut_ptr().cast::<c_char>()
}
