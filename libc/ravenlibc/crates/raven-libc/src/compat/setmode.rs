// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the setmode export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `setmode` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

/// `setmode`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setmode(spec: *const c_char) -> *mut core::ffi::c_void {
    if spec.is_null() {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return core::ptr::null_mut();
    }
    // SAFETY: `spec` is NUL-terminated.
    let spec = unsafe { super::c_bytes(spec, super::MAX_CSTR) };
    // The donor reads the umask at parse time (umask(mask = umask(0)); mask = ~mask).
    // SAFETY: `umask` is the libc accessor; restoring it immediately.
    let mask = unsafe { super::umask(0) };
    // SAFETY: restore the caller's mask.
    unsafe { super::umask(mask) };
    let Some(parsed) = raven_libc_compat::setmode::setmode(spec) else {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return core::ptr::null_mut();
    };
    #[repr(C)]
    struct ShellModeSet {
        parsed: raven_libc_compat::setmode::ModeSet,
        umask: u32,
    }
    // SAFETY: `malloc` returns a block of the requested size or null.
    let pointer =
        unsafe { super::malloc(core::mem::size_of::<ShellModeSet>()) }.cast::<ShellModeSet>();
    if pointer.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the allocation is one ShellModeSet.
    unsafe {
        core::ptr::write(
            pointer,
            ShellModeSet {
                parsed,
                umask: mask,
            },
        )
    };
    pointer.cast()
}
