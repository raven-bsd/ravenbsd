// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `__progname` data symbol.
use core::ffi::c_char;

/// The program basename used by error messages.
#[unsafe(no_mangle)]
pub static mut __progname: *const c_char = core::ptr::null();
