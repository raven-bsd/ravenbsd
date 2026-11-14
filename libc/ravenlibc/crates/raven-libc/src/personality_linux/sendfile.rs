// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sendfile(2)`.
use core::ffi::c_int;

use crate::personality_common::{RealFiles, ret_ssize};

/// Copy from `in_fd` to `out_fd`.
///
/// # Safety
/// `offset` is NULL or points to a readable/writable `off_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sendfile(
    out_fd: c_int,
    in_fd: c_int,
    offset: *mut i64,
    count: usize,
) -> isize {
    let mut value = if offset.is_null() {
        None
    } else {
        // SAFETY: the caller's offset contract.
        Some(unsafe { core::ptr::read_unaligned(offset) })
    };
    let result = raven_libc_personality_linux::sendfile::sendfile(
        &mut RealFiles,
        out_fd,
        in_fd,
        value.as_mut(),
        count,
    );
    if let (Some(position), false) = (value, offset.is_null()) {
        // SAFETY: the caller granted one writable off_t.
        unsafe { core::ptr::write_unaligned(offset, position) };
    }
    ret_ssize(result)
}
