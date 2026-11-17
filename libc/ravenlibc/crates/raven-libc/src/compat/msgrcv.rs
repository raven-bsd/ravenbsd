// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `msgrcv`: a null receive buffer is `EFAULT`; otherwise no queue service is `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::sysv;

use super::fail;

/// `msgrcv`.
///
/// # Safety
/// `message` points to at least `length` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgrcv(
    _msqid: c_int,
    message: *mut c_void,
    _length: usize,
    _type: i64,
    _flags: c_int,
) -> isize {
    if message.is_null() {
        let _ = fail(raven_libc_ipc::EFAULT);
        return -1;
    }
    let _ = fail(sysv::unsupported());
    -1
}
