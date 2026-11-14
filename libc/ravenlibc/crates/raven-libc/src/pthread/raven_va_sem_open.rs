// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sem_open(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_thread::sem;

use crate::stdio::va::List;

crate::raven_variadic!(
    prefix: "",
    export: "sem_open",
    target: crate::pthread::sem_open::__raven_va_sem_open,
    named: 2
);

/// The variadic target: `sem_open(name, flags, mode, value)`.
///
/// # Safety
/// `name` is a NUL-terminated string; the trampoline built `list`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_va_sem_open(
    name: *const c_char,
    flags: c_int,
    _list: *mut List,
) -> *mut sem::Sem {
    if name.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller supplies a NUL-terminated name.
    let bytes = unsafe { crate::ffi::cstr::slice(name) };
    match sem::open(bytes, flags) {
        Ok(_) => core::ptr::null_mut(),
        Err(error) => {
            crate::ffi::errno::set(error);
            core::ptr::null_mut()
        }
    }
}
