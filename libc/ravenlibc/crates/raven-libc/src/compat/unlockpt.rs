// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `unlockpt`: a pair created by `posix_openpt` is immediately openable by the same process.
use core::ffi::c_int;

/// `unlockpt`.
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unlockpt(_fd: c_int) -> c_int {
    0
}
