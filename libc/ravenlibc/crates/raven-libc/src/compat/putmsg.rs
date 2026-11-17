// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `putmsg` and `putpmsg`. The records are validated and the answer is `ENOSYS`; no message is
//! passed to a non-existent STREAMS stack.

use core::ffi::c_int;

use raven_libc_ipc::stropts;

use super::fail;
use super::stropts_state::{StrBuf, check_strbuf};

/// `putmsg`.
///
/// # Safety
/// `control`/`data` are null or readable `struct strbuf` records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putmsg(
    _fd: c_int,
    control: *const StrBuf,
    data: *const StrBuf,
    _flags: c_int,
) -> c_int {
    if !control.is_null() {
        // SAFETY: `control` is null or readable by the caller contract.
        if let Err(error) = unsafe { check_strbuf(control) } {
            return fail(error);
        }
    }
    if !data.is_null() {
        // SAFETY: `data` is null or readable by the caller contract.
        if let Err(error) = unsafe { check_strbuf(data) } {
            return fail(error);
        }
    }
    fail(stropts::unsupported())
}

/// `putpmsg`.
///
/// # Safety
/// `control`/`data` are null or readable `struct strbuf` records.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putpmsg(
    _fd: c_int,
    control: *const StrBuf,
    data: *const StrBuf,
    _band: c_int,
    _flags: c_int,
) -> c_int {
    if !control.is_null() {
        // SAFETY: `control` is null or readable by the caller contract.
        if let Err(error) = unsafe { check_strbuf(control) } {
            return fail(error);
        }
    }
    if !data.is_null() {
        // SAFETY: `data` is null or readable by the caller contract.
        if let Err(error) = unsafe { check_strbuf(data) } {
            return fail(error);
        }
    }
    fail(stropts::unsupported())
}
