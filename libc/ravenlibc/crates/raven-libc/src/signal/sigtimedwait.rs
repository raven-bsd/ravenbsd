// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigtimedwait(3)`.
use core::ffi::c_int;
use raven_libc_abi::structs::{SiginfoT, Timespec};
use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::{SigSet, wait};
/// Wait for a pending signal with an optional timeout.
///
/// # Safety
/// `set` is readable; `info`/`timeout` are NULL or writable/readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigtimedwait(
    set: *const SigSet,
    info: *mut SiginfoT,
    timeout: *const Timespec,
) -> c_int {
    if set.is_null() {
        return -1;
    }
    let info = if info.is_null() {
        None
    } else {
        Some(unsafe { &mut *info })
    };
    let timeout = if timeout.is_null() {
        None
    } else {
        Some(unsafe { *timeout })
    };
    match wait::sigtimedwait(&SysKernel, unsafe { &*set }, info, timeout) {
        Ok(sig) => sig,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
