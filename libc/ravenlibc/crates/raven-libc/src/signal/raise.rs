// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `raise(3)`.
use core::ffi::c_int;

use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::send;

use super::DISP;

/// Send `sig` to the calling thread.
#[unsafe(no_mangle)]
pub extern "C" fn raise(sig: c_int) -> c_int {
    // A masked raise defers to a later unblock, which kicks the thread; the thread needs its
    // PG-7 registration (and context area) before that kick can mean anything.
    if let Err(error) = super::ensure_entry() {
        crate::ffi::errno::set(error);
        return -1;
    }
    match send::raise(&SysKernel, &DISP, sig) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
