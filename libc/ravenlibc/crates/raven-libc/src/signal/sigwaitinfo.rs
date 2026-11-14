// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigwaitinfo(3)`.
use core::ffi::c_int;
use raven_libc_abi::structs::SiginfoT;
use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::{SigSet, wait};
/// Wait for a signal and store its record.
///
/// # Safety
/// `set` is readable; `info` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigwaitinfo(set: *const SigSet, info: *mut SiginfoT) -> c_int {
    if set.is_null() || info.is_null() {
        return -1;
    }
    match wait::sigwaitinfo(&SysKernel, unsafe { &*set }, unsafe { &mut *info }) {
        Ok(sig) => sig,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
