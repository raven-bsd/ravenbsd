// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `__progname_full` data symbol.
use core::ffi::c_char;

/// The full program name (C61 start writes it).
#[unsafe(no_mangle)]
pub static mut __progname_full: *const c_char = core::ptr::null();
