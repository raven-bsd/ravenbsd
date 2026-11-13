// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `calloc(3)`.
use core::ffi::c_void;

/// Allocate zero-filled storage for `count` objects of `size` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn calloc(count: usize, size: usize) -> *mut c_void {
    crate::malloc::heap::api().calloc(count, size) as *mut c_void
}
