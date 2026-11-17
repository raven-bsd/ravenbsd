// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `shmat`: no SysV shared-memory service exists, so the answer is `(void *)-1` with `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::sysv;

use super::fail;

/// `shmat`.
///
/// # Safety
/// `address` is NULL or a caller-chosen address hint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shmat(
    _shmid: c_int,
    _address: *const c_void,
    _flags: c_int,
) -> *mut c_void {
    let _ = fail(sysv::unsupported());
    usize::MAX as *mut c_void
}
