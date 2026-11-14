// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_char, c_void};

use raven_libc_abi::structs::Lconv;

/// `freelocale`.
///
/// # Safety
/// `locale` is a locale object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freelocale(locale: *mut c_void) {
    // SAFETY: forwarded C contract.
    unsafe { super::freelocale_impl(locale) }
}

/// `localeconv`.
#[unsafe(no_mangle)]
pub extern "C" fn localeconv() -> *mut Lconv {
    super::localeconv_impl()
}

/// `nl_langinfo`.
#[unsafe(no_mangle)]
pub extern "C" fn nl_langinfo(item: u32) -> *mut c_char {
    super::nl_langinfo_impl(item)
}

/// `nl_langinfo_l`.
///
/// # Safety
/// `locale` is a locale object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nl_langinfo_l(item: u32, locale: *mut c_void) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::nl_langinfo_l_impl(item, locale) }
}
