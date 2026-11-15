#![cfg(target_os = "none")]
// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::{Context, set_ip_impl};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn _Unwind_SetIP(context: *mut Context, ip: usize) {
    set_ip_impl(context, ip);
}
