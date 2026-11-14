// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getnetbyaddr`, `getnetbyname` and `getnetent`.

use core::ffi::{c_char, c_int};

use super::records::{self, Netent};

/// `getnetbyaddr(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn getnetbyaddr(net: u32, _kind: c_int) -> *mut Netent {
    records::net_by_addr(net)
        .map(|entry| records::fill_net(&entry))
        .unwrap_or(core::ptr::null_mut())
}

/// `getnetbyname(3)`.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getnetbyname(name: *const c_char) -> *mut Netent {
    // SAFETY: the caller's C string.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    records::net_by_name(name)
        .map(|entry| records::fill_net(&entry))
        .unwrap_or(core::ptr::null_mut())
}

/// `getnetent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn getnetent() -> *mut Netent {
    records::next_network()
        .map(|entry| records::fill_net(&entry))
        .unwrap_or(core::ptr::null_mut())
}
