// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `msgget`: no SysV message queue service exists, so the answer is `ENOSYS`.

use core::ffi::c_int;

use raven_libc_abi::types::key_t;
use raven_libc_ipc::sysv;

use super::fail;

/// `msgget`.
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgget(_key: key_t, _flags: c_int) -> c_int {
    fail(sysv::unsupported())
}
