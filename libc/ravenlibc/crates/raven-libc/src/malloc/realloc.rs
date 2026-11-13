// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `realloc(3)`.
use core::ffi::c_void;

/// Resize a block.
///
/// # Safety
/// `pointer` is NULL or a live block from this allocator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(pointer: *mut c_void, size: usize) -> *mut c_void {
    crate::malloc::heap::api().realloc(pointer as usize, size) as *mut c_void
}
