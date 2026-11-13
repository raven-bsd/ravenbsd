// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `memalign(3)`.
use core::ffi::c_void;

/// Allocate `size` bytes at a `alignment` boundary.
#[unsafe(no_mangle)]
pub extern "C" fn memalign(alignment: usize, size: usize) -> *mut c_void {
    crate::malloc::heap::api().memalign(alignment, size) as *mut c_void
}
