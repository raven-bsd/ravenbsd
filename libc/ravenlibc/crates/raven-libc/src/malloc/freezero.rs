// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `freezero(3)`.
use core::ffi::c_void;

/// Zero `size` bytes of the block and free it.
///
/// # Safety
/// `pointer` is NULL or a live block of at least `size` bytes from this allocator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freezero(pointer: *mut c_void, size: usize) {
    crate::malloc::heap::api().freezero(pointer as usize, size);
}
