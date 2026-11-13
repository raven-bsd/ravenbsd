// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `malloc(3)`.
use core::ffi::c_void;

use raven_libc_sys::weak_hook;

/// The dummy: the same heap the strong member uses, so an uninstalled cell is still a real
/// allocator (the hook table's "bump allocator" row, implemented by C49's tiny backend).
fn dummy_malloc(size: usize) -> usize {
    crate::malloc::heap::api().malloc(size)
}

weak_hook! {
    /// The tiny-profile allocation hook.
    pub(crate) fn __rl_malloc_impl(size: usize) -> usize;
    dummy = crate::malloc::malloc_fn::dummy_malloc;
}

/// The allocator's diagnostic hook; the dummy is a no-op (`MALLOC_OPTIONS` off).
#[allow(dead_code)] // the macro's dummy is only called when the hook cell is used
fn dummy_malloc_debug() {}

weak_hook! {
    /// The `MALLOC_OPTIONS` diagnostics hook.
    pub(crate) fn __rl_malloc_debug() -> ();
    dummy = crate::malloc::malloc_fn::dummy_malloc_debug;
}

/// Allocate `size` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn malloc(size: usize) -> *mut c_void {
    __rl_malloc_impl(size) as *mut c_void
}
