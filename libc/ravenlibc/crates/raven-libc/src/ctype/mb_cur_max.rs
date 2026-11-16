// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `__ctype_get_mb_cur_max(3)`, the function behind the `MB_CUR_MAX` macro.

/// `__ctype_get_mb_cur_max`: the active locale's maximum multibyte length.
#[unsafe(no_mangle)]
pub extern "C" fn __ctype_get_mb_cur_max() -> usize {
    crate::locale::mb_cur_max()
}
