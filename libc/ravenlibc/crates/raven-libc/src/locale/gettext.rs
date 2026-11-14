// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::c_char;

/// `gettext`.
///
/// # Safety
/// `msgid` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gettext(msgid: *const c_char) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::gettext_impl(msgid) }
}

/// `dgettext`.
///
/// # Safety
/// `domain` and `msgid` are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dgettext(domain: *const c_char, msgid: *const c_char) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::dgettext_impl(domain, msgid) }
}

/// `dcgettext`.
///
/// # Safety
/// `domain` and `msgid` are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dcgettext(
    domain: *const c_char,
    msgid: *const c_char,
    category: i32,
) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::dcgettext_impl(domain, msgid, category) }
}

/// `ngettext`.
///
/// # Safety
/// `msgid1` and `msgid2` are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ngettext(
    msgid1: *const c_char,
    msgid2: *const c_char,
    n: u64,
) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::ngettext_impl(msgid1, msgid2, n) }
}
