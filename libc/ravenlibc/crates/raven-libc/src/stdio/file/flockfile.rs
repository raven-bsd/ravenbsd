// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `flockfile`, `funlockfile` and `ftrylockfile`.
//!
//! The real owner-tid recursive stream lock lives in `raven-libc-stdio`; these C entries delegate
//! to it so an explicit lock and the implicit per-operation guard see the same owner.

use core::ffi::{c_int, c_void};

/// `flockfile`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flockfile(stream: *mut c_void) {
    if let Some(file) = super::file(stream) {
        raven_libc_stdio::flockfile(file);
    }
}

/// `funlockfile`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn funlockfile(stream: *mut c_void) {
    if let Some(file) = super::file(stream) {
        raven_libc_stdio::funlockfile(file);
    }
}

/// `ftrylockfile`: non-blocking; 0 on success, -1 when another thread owns the stream.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftrylockfile(stream: *mut c_void) -> c_int {
    match super::file(stream) {
        Some(file) => raven_libc_stdio::ftrylockfile(file),
        None => 0,
    }
}
