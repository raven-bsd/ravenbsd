// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `malloc_usable_size(3)`.
use core::ffi::c_void;

/// The usable size of a live allocation.
///
/// # Safety
/// `pointer` is NULL or a live block from this allocator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc_usable_size(pointer: *mut c_void) -> usize {
    crate::malloc::heap::api().malloc_usable_size(pointer as usize)
}
