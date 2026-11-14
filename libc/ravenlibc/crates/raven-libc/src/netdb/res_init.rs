// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `res_init(3)`.
use core::ffi::c_int;

/// C51's resolver has no global state: a successful no-op.
#[unsafe(no_mangle)]
pub extern "C" fn res_init() -> c_int {
    raven_libc_resolv::compat::res_init()
}
