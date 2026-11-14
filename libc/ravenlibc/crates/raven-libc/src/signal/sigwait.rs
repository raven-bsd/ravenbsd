// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigwait(3)`.
use core::ffi::c_int;
use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::{SigSet, wait};
/// Wait for a signal in `set` and store its number.
///
/// # Safety
/// `set` is readable; `sig` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigwait(set: *const SigSet, sig: *mut c_int) -> c_int {
    if set.is_null() || sig.is_null() {
        return -1;
    }
    match wait::sigwait(&SysKernel, unsafe { &*set }) {
        Ok(value) => {
            unsafe { *sig = value };
            0
        }
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
