// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigqueue(3)`.
use core::ffi::c_int;
use raven_libc_abi::structs::Sigval;
use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::send;
/// Queue a signal with a value.
#[unsafe(no_mangle)]
pub extern "C" fn sigqueue(pid: c_int, sig: c_int, value: Sigval) -> c_int {
    match send::sigqueue(&SysKernel, pid, sig, value) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
