// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `free(3)`.
use core::ffi::c_void;

/// Release a block; NULL is a no-op.
///
/// # Safety
/// `pointer` is NULL or came from this allocator and is not freed twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(pointer: *mut c_void) {
    crate::malloc::heap::api().free(pointer as usize);
}
