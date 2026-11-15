#![cfg(target_os = "none")]
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use core::ffi::c_int;

use super::{Context, set_gr_impl};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_SetGR(context: *mut Context, index: c_int, value: usize) {
    set_gr_impl(context, index, value);
}
