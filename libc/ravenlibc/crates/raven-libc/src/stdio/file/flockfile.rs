// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `flockfile`, `funlockfile` and `ftrylockfile`.
//!
//! The RavenBSD stdio state serializes through the shell's single-threaded stream lock; the
//! explicit POSIX file lock calls are no-ops and `ftrylockfile` always succeeds.

use core::ffi::{c_int, c_void};

/// `flockfile`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flockfile(_stream: *mut c_void) {}

/// `funlockfile`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn funlockfile(_stream: *mut c_void) {}

/// `ftrylockfile`: the stream lock is always available.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftrylockfile(_stream: *mut c_void) -> c_int {
    0
}
