// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::c_int;

/// `getchar`.
#[unsafe(no_mangle)]
pub extern "C" fn getchar() -> c_int {
    super::getchar_impl()
}

/// `putchar`.
#[unsafe(no_mangle)]
pub extern "C" fn putchar(c: c_int) -> c_int {
    super::putchar_impl(c)
}
