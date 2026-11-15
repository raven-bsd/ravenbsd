#![cfg(target_os = "none")]
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use core::ffi::c_void;

use super::find_enclosing_function_impl;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_FindEnclosingFunction(pc: *mut c_void) -> *mut c_void {
    find_enclosing_function_impl(pc)
}
