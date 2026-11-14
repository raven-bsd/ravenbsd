// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getopt_long_only(3)`.
use core::ffi::{c_char, c_int};

use super::getopt_support::OptionC;

/// Parse the next long option even when it starts with one dash.
///
/// # Safety
/// The C `getopt_long_only` argument contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getopt_long_only(
    argc: c_int,
    argv: *mut *mut c_char,
    optstring: *const c_char,
    longopts: *const OptionC,
    longindex: *mut c_int,
) -> c_int {
    // SAFETY: the caller's arrays.
    unsafe { crate::stdlib::getopt_support::long(argv, argc, optstring, longopts, longindex, true) }
}
