// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::c_char;

/// `bind_textdomain_codeset`.
///
/// # Safety
/// `domain` and `codeset` are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bind_textdomain_codeset(
    domain: *const c_char,
    codeset: *const c_char,
) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::bind_textdomain_codeset_impl(domain, codeset) }
}
