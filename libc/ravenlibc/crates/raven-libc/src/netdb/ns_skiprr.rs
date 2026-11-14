// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ns_skiprr(3)`.
use core::ffi::c_int;

/// The bytes of `count` records of `section`.
///
/// # Safety
/// `msg <= eom` and both point into one readable buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_skiprr(
    msg: *const u8,
    eom: *const u8,
    section: c_int,
    count: c_int,
) -> c_int {
    // SAFETY: the caller's message contract.
    unsafe { crate::netdb::msg::ns_skiprr(msg, eom, section, count) }
}
