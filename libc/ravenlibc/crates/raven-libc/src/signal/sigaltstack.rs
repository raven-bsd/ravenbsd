// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigaltstack(3)`.
use core::ffi::c_int;

use raven_libc_abi::structs::StackT;
use raven_libc_signal::altstack;
use raven_libc_signal::kernel::SysKernel;

/// Replace or query the alternate signal stack.
///
/// # Safety
/// `new`/`old` are NULL or point to `stack_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigaltstack(new: *const StackT, old: *mut StackT) -> c_int {
    let request = if new.is_null() {
        None
    } else {
        Some(unsafe { &*new })
    };
    let previous = if old.is_null() {
        None
    } else {
        Some(unsafe { &mut *old })
    };
    match altstack::sigaltstack(&SysKernel, request, previous) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
