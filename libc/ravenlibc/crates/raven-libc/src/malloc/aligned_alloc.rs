// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `aligned_alloc(3)`.
use core::ffi::c_void;

/// C11 aligned allocation: `size` is a multiple of `alignment`.
#[unsafe(no_mangle)]
pub extern "C" fn aligned_alloc(alignment: usize, size: usize) -> *mut c_void {
    crate::malloc::heap::api().aligned_alloc(alignment, size) as *mut c_void
}
