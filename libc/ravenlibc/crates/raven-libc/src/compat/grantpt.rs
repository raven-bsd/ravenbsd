// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `grantpt`: the pty server owns the pair, so there is no slave device node to chown.
use core::ffi::c_int;

/// `grantpt`.
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grantpt(_fd: c_int) -> c_int {
    0
}
