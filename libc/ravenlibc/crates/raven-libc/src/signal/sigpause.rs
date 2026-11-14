// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigpause(3)`.
use super::DISP;
use core::ffi::c_int;
use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::legacy;
/// Atomically unblock `sig` and wait.
#[unsafe(no_mangle)]
pub extern "C" fn sigpause(sig: c_int) -> c_int {
    crate::ffi::errno::set(legacy::sigpause(&SysKernel, &DISP, sig));
    -1
}
