// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `kill(3)`.
use core::ffi::c_int;

use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::send;

/// Send `sig` to `pid`.
#[unsafe(no_mangle)]
pub extern "C" fn kill(pid: c_int, sig: c_int) -> c_int {
    match send::kill(&SysKernel, pid, sig) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
