// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `copy_file_range(2)`.
use core::ffi::c_int;

use crate::personality_common::{RealFiles, ret_ssize};

/// Copy a range between two descriptors.
///
/// # Safety
/// The two offset pointers are NULL or point to readable/writable `off_t` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn copy_file_range(
    fd_in: c_int,
    off_in: *mut i64,
    fd_out: c_int,
    off_out: *mut i64,
    len: usize,
    flags: u32,
) -> isize {
    let mut input = if off_in.is_null() {
        None
    } else {
        // SAFETY: the caller's offset contract.
        Some(unsafe { core::ptr::read_unaligned(off_in) })
    };
    let mut output = if off_out.is_null() {
        None
    } else {
        // SAFETY: the caller's offset contract.
        Some(unsafe { core::ptr::read_unaligned(off_out) })
    };
    let result = raven_libc_personality_linux::sendfile::copy_file_range(
        &mut RealFiles,
        fd_in,
        input.as_mut(),
        fd_out,
        output.as_mut(),
        len,
        flags,
    );
    if let (Some(position), false) = (input, off_in.is_null()) {
        // SAFETY: the caller granted one writable off_t.
        unsafe { core::ptr::write_unaligned(off_in, position) };
    }
    if let (Some(position), false) = (output, off_out.is_null()) {
        // SAFETY: the caller granted one writable off_t.
        unsafe { core::ptr::write_unaligned(off_out, position) };
    }
    ret_ssize(result)
}
