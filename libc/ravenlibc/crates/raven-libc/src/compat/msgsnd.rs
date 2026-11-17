// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `msgsnd`: a null message is `EFAULT`; otherwise no queue service is `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::sysv;

use super::fail;

/// `msgsnd`.
///
/// # Safety
/// `message` points to at least `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgsnd(
    _msqid: c_int,
    message: *const c_void,
    _length: usize,
    _flags: c_int,
) -> c_int {
    if message.is_null() {
        return fail(raven_libc_ipc::EFAULT);
    }
    fail(sysv::unsupported())
}
