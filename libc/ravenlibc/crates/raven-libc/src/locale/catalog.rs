// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Shared return buffer for the XSI catalog exports.

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ffi::c_char;

/// `(nl_catd)-1`, the failure sentinel of `catopen`.
pub(crate) const CATD_FAILED: usize = usize::MAX;

struct ReturnBuffer(UnsafeCell<Vec<u8>>);

// SAFETY: the shell is single-threaded until C57; the buffer is written for one call at a time.
unsafe impl Sync for ReturnBuffer {}

static RETURN: ReturnBuffer = ReturnBuffer(UnsafeCell::new(Vec::new()));

/// Copy a catalog value into the one C return buffer and terminate it.
pub(crate) fn c_string(bytes: &[u8]) -> *mut c_char {
    // SAFETY: single-threaded access; the buffer is only written here.
    let buffer = unsafe { &mut *RETURN.0.get() };
    buffer.clear();
    buffer.extend_from_slice(bytes);
    buffer.push(0);
    buffer.as_mut_ptr().cast::<c_char>()
}
