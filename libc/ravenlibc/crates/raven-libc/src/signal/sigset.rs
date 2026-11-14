// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigset(3)`.
use super::DISP;
use core::ffi::c_int;
use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::legacy;
/// Hold, install or unblock `sig`.
#[unsafe(no_mangle)]
pub extern "C" fn sigset(sig: c_int, handler: usize) -> usize {
    match legacy::sigset(&SysKernel, &DISP, sig, handler) {
        Ok(old) => old,
        Err(error) => {
            crate::ffi::errno::set(error);
            usize::MAX
        }
    }
}
