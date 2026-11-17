// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `shmget`: no SysV shared-memory service exists, so the answer is `ENOSYS`.

use core::ffi::c_int;

use raven_libc_abi::types::key_t;
use raven_libc_ipc::sysv;

use super::fail;

/// `shmget`.
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shmget(_key: key_t, _size: usize, _flags: c_int) -> c_int {
    fail(sysv::unsupported())
}
