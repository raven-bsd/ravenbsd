// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `mq_open`. No message-queue server exists; a valid POSIX name still reports `ENOSYS`.

use core::ffi::{c_char, c_int, c_void};

use raven_libc_ipc::mqueue;

use super::fail;

crate::raven_variadic!(
    prefix: "",
    export: "mq_open",
    target: __raven_va_mq_open,
    named: 2
);

/// The variadic target: `mq_open(name, flags, mode, attr)`.
///
/// # Safety
/// `name` is a NUL-terminated string; the trampoline built `list`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_va_mq_open(
    name: *const c_char,
    _flags: c_int,
    _list: *mut c_void,
) -> c_int {
    if name.is_null() {
        return fail(raven_libc_ipc::EINVAL);
    }
    // SAFETY: the caller supplies a NUL-terminated name.
    let bytes = unsafe { crate::ffi::cstr::slice(name) };
    if let Err(error) = mqueue::validate_name(bytes) {
        return fail(error);
    }
    fail(mqueue::unsupported())
}
