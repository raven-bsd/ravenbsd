// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dn_expand(3)`.
use core::ffi::{c_char, c_int};

/// Expand one compressed name into the caller's buffer.
///
/// # Safety
/// The pointers name one readable message and one writable output buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dn_expand(
    msg: *const u8,
    eom: *const u8,
    comp_dn: *const u8,
    exp_dn: *mut c_char,
    length: c_int,
) -> c_int {
    // SAFETY: the caller's message and output contracts.
    unsafe { crate::netdb::msg::dn_expand(msg, eom, comp_dn, exp_dn, length) }
}
