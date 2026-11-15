#![cfg(target_os = "none")]
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use core::ffi::c_int;

use super::{Context, get_ip_info_impl};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_GetIPInfo(context: *mut Context, before: *mut c_int) -> usize {
    // SAFETY: the caller's writable `before` contract.
    unsafe { get_ip_info_impl(context, before) }
}
