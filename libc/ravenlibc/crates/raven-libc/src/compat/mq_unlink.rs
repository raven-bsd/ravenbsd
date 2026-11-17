// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `mq_unlink`. A valid POSIX name still reports `ENOSYS`.

use core::ffi::c_char;

use raven_libc_ipc::mqueue;

use super::fail;

/// `mq_unlink`.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_unlink(name: *const c_char) -> core::ffi::c_int {
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
