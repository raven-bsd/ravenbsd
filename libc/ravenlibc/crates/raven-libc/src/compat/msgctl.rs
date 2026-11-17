// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `msgctl`: no SysV message queue service exists, so the answer is `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::sysv;

use super::fail;

/// `msgctl`.
///
/// # Safety
/// `buffer` is NULL or points to a caller-owned `struct msqid_ds`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgctl(_msqid: c_int, _cmd: c_int, _buffer: *mut c_void) -> c_int {
    fail(sysv::unsupported())
}
