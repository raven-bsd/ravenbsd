// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inet_addr(3)`.
use core::ffi::c_char;

/// The network-order address, or `INADDR_NONE`.
///
/// # Safety
/// `text` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inet_addr(text: *const c_char) -> u32 {
    if text.is_null() {
        return raven_libc_abi::netinet::INADDR_NONE;
    }
    // SAFETY: the caller's NUL-terminated string.
    raven_libc_resolv::inet_addr(unsafe { crate::ffi::cstr::slice(text) })
}
