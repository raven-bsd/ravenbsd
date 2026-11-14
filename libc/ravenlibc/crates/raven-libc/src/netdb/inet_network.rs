// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inet_network(3)`.
use core::ffi::c_char;

/// The host-order network number, or `INADDR_NONE`.
///
/// # Safety
/// `text` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inet_network(text: *const c_char) -> u32 {
    if text.is_null() {
        return raven_libc_abi::netinet::INADDR_NONE;
    }
    // SAFETY: the caller's NUL-terminated string.
    raven_libc_resolv::inet_network(unsafe { crate::ffi::cstr::slice(text) })
}
