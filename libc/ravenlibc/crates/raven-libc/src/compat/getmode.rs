// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the getmode export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `getmode` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

/// `getmode`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getmode(bbox: *const core::ffi::c_void, omode: u32) -> u32 {
    if bbox.is_null() {
        return omode;
    }
    #[repr(C)]
    struct ShellModeSet {
        parsed: raven_libc_compat::setmode::ModeSet,
        umask: u32,
    }
    // SAFETY: `bbox` is the object from `setmode`, which is exactly ShellModeSet.
    let set = unsafe { &*bbox.cast::<ShellModeSet>() };
    set.parsed.getmode(omode, set.umask)
}
