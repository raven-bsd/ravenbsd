// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Shared `struct strbuf` record and validation for the `<stropts.h>` exports.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::Errno;
use raven_libc_ipc::stropts;

/// The C `struct strbuf` from `stropts.h`.
#[repr(C)]
pub struct StrBuf {
    /// Buffer capacity.
    pub maxlen: c_int,
    /// Bytes used.
    pub len: c_int,
    /// Buffer address.
    pub buf: *mut c_char,
}

/// Validate a caller's `struct strbuf`.
///
/// # Safety
/// `strbuf` is null or points to a readable `struct strbuf`.
pub(crate) unsafe fn check_strbuf(strbuf: *const StrBuf) -> Result<(), Errno> {
    // SAFETY: the caller contract above guarantees `strbuf` is null or readable.
    let Some(value) = (unsafe { strbuf.as_ref() }) else {
        return Err(raven_libc_ipc::EFAULT);
    };
    stropts::validate_strbuf(value.maxlen, value.len, !value.buf.is_null())
}
