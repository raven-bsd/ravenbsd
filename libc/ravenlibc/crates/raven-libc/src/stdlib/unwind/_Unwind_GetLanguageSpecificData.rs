#![cfg(target_os = "none")]
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use core::ffi::c_void;

use super::{Context, get_language_specific_data_impl};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_GetLanguageSpecificData(context: *mut Context) -> *mut c_void {
    get_language_specific_data_impl(context)
}
