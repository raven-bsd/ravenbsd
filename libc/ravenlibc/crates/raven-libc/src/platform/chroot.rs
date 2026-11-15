// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `chroot(2)`.
use core::ffi::{c_char, c_int};

/// Change the process root; no per-process root primitive exists yet (PG-33, class-A EPERM).
///
/// # Safety
/// `path` is not dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chroot(_path: *const c_char) -> c_int {
    crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::EPERM as u16,
    )))
}
