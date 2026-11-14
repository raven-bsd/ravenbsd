// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The exported `optarg` data symbol.
use core::ffi::c_char;

/// The argument of the current option.
#[unsafe(no_mangle)]
pub static mut optarg: *mut c_char = core::ptr::null_mut();
