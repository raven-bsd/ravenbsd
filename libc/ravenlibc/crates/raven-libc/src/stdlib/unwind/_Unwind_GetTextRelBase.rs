#![cfg(target_os = "none")]
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::{Context, get_text_rel_base_impl};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_GetTextRelBase(context: *mut Context) -> usize {
    get_text_rel_base_impl(context)
}
