// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `program_invocation_name` data symbol.
use core::ffi::c_char;

/// The GNU full program name (C61 start writes it).
#[unsafe(no_mangle)]
pub static mut program_invocation_name: *const c_char = core::ptr::null();
