// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sighold(3)`.
use core::ffi::c_int;
use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::legacy;
/// sighold the signal.
#[unsafe(no_mangle)]
pub extern "C" fn sighold(sig: c_int) -> c_int {
    match legacy::sighold(&SysKernel, sig) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
