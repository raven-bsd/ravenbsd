// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dn_skipname(3)`.
use core::ffi::c_int;

/// The length of the name at `comp_dn`.
///
/// # Safety
/// `comp_dn <= eom` and both point into one readable buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dn_skipname(comp_dn: *const u8, eom: *const u8) -> c_int {
    // SAFETY: the caller's message contract.
    unsafe { crate::netdb::msg::dn_skipname(comp_dn, eom) }
}
