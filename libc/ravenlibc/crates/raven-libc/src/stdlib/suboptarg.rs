// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The exported `suboptarg` data symbol.
use core::ffi::c_char;

/// The token found by the last `getsubopt` call.
#[unsafe(no_mangle)]
pub static mut suboptarg: *mut c_char = core::ptr::null_mut();
