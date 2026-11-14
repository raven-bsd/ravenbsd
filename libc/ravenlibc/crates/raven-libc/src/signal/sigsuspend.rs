// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigsuspend(3)`.
use super::DISP;
use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::{SigSet, wait};
/// Wait with a temporary mask.
///
/// # Safety
/// `set` points to a readable `sigset_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigsuspend(set: *const SigSet) -> core::ffi::c_int {
    if set.is_null() {
        return -1;
    }
    crate::ffi::errno::set(wait::sigsuspend(&SysKernel, &DISP, unsafe { &*set }));
    -1
}
