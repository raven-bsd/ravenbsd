// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `mq_setattr`. A required null pointer is `EFAULT`; otherwise the descriptor is `EBADF`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::mqueue;

use super::fail;

/// `mq_setattr`.
///
/// # Safety
/// `attr` is NULL or readable; `old` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_setattr(_mqd: c_int, attr: *const c_void, _old: *mut c_void) -> c_int {
    if attr.is_null() {
        return fail(raven_libc_ipc::EFAULT);
    }
    fail(mqueue::queue_descriptor_error())
}
