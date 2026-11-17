// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `shmctl`: no SysV shared-memory service exists, so the answer is `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::sysv;

use super::fail;

/// `shmctl`.
///
/// # Safety
/// `buffer` is NULL or points to a caller-owned `struct shmid_ds`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shmctl(_shmid: c_int, _cmd: c_int, _buffer: *mut c_void) -> c_int {
    fail(sysv::unsupported())
}
