// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::c_char;

/// `dngettext`.
///
/// # Safety
/// `domain`, `msgid1` and `msgid2` are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dngettext(
    domain: *const c_char,
    msgid1: *const c_char,
    msgid2: *const c_char,
    n: u64,
) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::dngettext_impl(domain, msgid1, msgid2, n) }
}

/// `dcngettext`.
///
/// # Safety
/// `domain`, `msgid1` and `msgid2` are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dcngettext(
    domain: *const c_char,
    msgid1: *const c_char,
    msgid2: *const c_char,
    n: u64,
    category: i32,
) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::dcngettext_impl(domain, msgid1, msgid2, n, category) }
}

/// `textdomain`.
///
/// # Safety
/// `domain` is NULL or NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn textdomain(domain: *const c_char) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::textdomain_impl(domain) }
}

/// `bindtextdomain`.
///
/// # Safety
/// `domain` is NUL terminated and `dir` is NULL or NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bindtextdomain(domain: *const c_char, dir: *const c_char) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::bindtextdomain_impl(domain, dir) }
}
