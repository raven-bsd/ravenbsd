// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `open` trampoline target: read the optional `mode_t` from the `va_list`.
use core::ffi::{c_char, c_int};

use raven_libc_abi::fcntl::{O_CREAT, O_TMPFILE};

use crate::stdio::va::{List, reader};
use crate::va::VaReader;

/// `open(path, flags, ...)` with the mode read only when the flags carry one.
///
/// # Safety
/// The C59 trampoline built `list`; `path` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_va_open(
    path: *const c_char,
    flags: c_int,
    list: *mut List,
) -> c_int {
    if path.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the trampoline's list is valid for the call.
    let mut values = unsafe { reader(list) };
    let mode = if flags & (O_CREAT | O_TMPFILE) != 0 {
        values.next_u64() as u32
    } else {
        0
    };
    // SAFETY: the caller's path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    let result = crate::ffi::platform::global().open(path, flags as u32, mode);
    crate::ffi::ret::platform_int(result)
}
