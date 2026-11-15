#![cfg(target_os = "none")]
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::{Context, get_region_start_impl};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_GetRegionStart(context: *mut Context) -> usize {
    get_region_start_impl(context)
}
