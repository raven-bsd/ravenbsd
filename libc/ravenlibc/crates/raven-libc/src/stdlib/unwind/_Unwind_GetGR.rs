#![cfg(target_os = "none")]
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use core::ffi::c_int;

use super::{Context, get_gr_impl};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_GetGR(context: *mut Context, index: c_int) -> usize {
    get_gr_impl(context, index)
}
