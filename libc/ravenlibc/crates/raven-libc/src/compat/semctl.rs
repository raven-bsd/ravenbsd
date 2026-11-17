// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `semctl`: no SysV semaphore service exists, so the answer is `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::sysv;

use super::fail;

crate::raven_variadic!(
    prefix: "",
    export: "semctl",
    target: __raven_va_semctl,
    named: 3
);

/// The variadic target: `semctl(semid, semnum, cmd, arg)`.
///
/// # Safety
/// The trampoline built `list`; no fixed argument is a pointer owned by this function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_va_semctl(
    _semid: c_int,
    _semnum: c_int,
    _cmd: c_int,
    _list: *mut c_void,
) -> c_int {
    fail(sysv::unsupported())
}
