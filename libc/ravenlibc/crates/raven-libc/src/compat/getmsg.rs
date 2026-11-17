// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `getmsg` and `getpmsg`. Required output buffers are checked and the answer is `ENOSYS`; no
//! message is invented.

use core::ffi::c_int;

use raven_libc_ipc::stropts;

use super::fail;
use super::stropts_state::StrBuf;

/// `getmsg`.
///
/// # Safety
/// `control`/`data` are writable `struct strbuf` records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getmsg(
    _fd: c_int,
    control: *mut StrBuf,
    data: *mut StrBuf,
    _flags: *mut c_int,
) -> c_int {
    if control.is_null() || data.is_null() {
        return fail(raven_libc_ipc::EFAULT);
    }
    fail(stropts::unsupported())
}

/// `getpmsg`.
///
/// # Safety
/// `control`/`data` are writable `struct strbuf` records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpmsg(
    _fd: c_int,
    control: *mut StrBuf,
    data: *mut StrBuf,
    _band: *mut c_int,
    _flags: *mut c_int,
) -> c_int {
    if control.is_null() || data.is_null() {
        return fail(raven_libc_ipc::EFAULT);
    }
    fail(stropts::unsupported())
}
