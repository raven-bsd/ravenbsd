// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getopt(3)`.
use core::ffi::{c_char, c_int};

/// Parse the next option.
///
/// # Safety
/// The C `getopt` argument contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getopt(
    argc: c_int,
    argv: *mut *mut c_char,
    optstring: *const c_char,
) -> c_int {
    // SAFETY: the caller's argv and optstring contracts.
    unsafe { crate::stdlib::getopt_support::short(argv, argc, optstring) }
}
