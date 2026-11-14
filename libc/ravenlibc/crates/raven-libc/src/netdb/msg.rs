// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The pointer forms of C51's DNS name/record codec. Every helper validates the three pointer
//! relationships (`base <= cursor <= end`) before it forms a slice; a bad relation is `-1`
//! without touching `errno`.

use raven_libc_abi::errno::Errno;

/// The `-1` failure of the `res_*`/`ns_*` family.
fn failed() -> core::ffi::c_int {
    crate::ffi::errno::set(Errno(raven_libc_abi::errno::EPROTO as u16));
    -1
}

/// `dn_expand(msg, eom, comp_dn, exp_dn, length)`.
///
/// # Safety
/// The pointers name one buffer with `base <= {comp_dn, eom} <= end`.
pub(crate) unsafe fn dn_expand(
    msg: *const u8,
    eom: *const u8,
    comp_dn: *const u8,
    out: *mut core::ffi::c_char,
    length: core::ffi::c_int,
) -> core::ffi::c_int {
    if msg.is_null() || eom.is_null() || comp_dn.is_null() || out.is_null() || length <= 0 {
        return failed();
    }
    let total = (eom as usize).checked_sub(msg as usize);
    let position = (comp_dn as usize).checked_sub(msg as usize);
    let (Some(total), Some(position)) = (total, position) else {
        return failed();
    };
    if position > total {
        return failed();
    }
    // SAFETY: the caller grants one readable message of `total` bytes.
    let message = unsafe { core::slice::from_raw_parts(msg, total) };
    let mut name = [0u8; 256];
    let Some((consumed, text)) = raven_libc_resolv::dn_expand(message, position, &mut name) else {
        return failed();
    };
    let capacity = match usize::try_from(length) {
        Ok(value) => value,
        Err(_) => return failed(),
    };
    if text.saturating_add(1) > capacity {
        return failed();
    }
    // SAFETY: the caller's output buffer has `capacity` writable bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(name.as_ptr(), out.cast::<u8>(), text);
        out.cast::<u8>().add(text).write(0);
    }
    match core::ffi::c_int::try_from(consumed) {
        Ok(value) => value,
        Err(_) => failed(),
    }
}

/// `dn_skipname(comp_dn, eom)`; compression pointers are not followed.
///
/// # Safety
/// `comp_dn <= eom` and both point into one readable buffer.
pub(crate) unsafe fn dn_skipname(comp_dn: *const u8, eom: *const u8) -> core::ffi::c_int {
    if comp_dn.is_null() || eom.is_null() {
        return failed();
    }
    let Some(total) = (eom as usize).checked_sub(comp_dn as usize) else {
        return failed();
    };
    // SAFETY: the caller grants one readable name of `total` bytes.
    let message = unsafe { core::slice::from_raw_parts(comp_dn, total) };
    match raven_libc_resolv::dn_skipname(message, 0) {
        Some(value) => core::ffi::c_int::try_from(value).unwrap_or(-1),
        None => failed(),
    }
}

/// `ns_skiprr(msg, eom, section, count)`.
///
/// # Safety
/// `msg <= eom` and both point into one readable buffer.
pub(crate) unsafe fn ns_skiprr(
    msg: *const u8,
    eom: *const u8,
    section: core::ffi::c_int,
    count: core::ffi::c_int,
) -> core::ffi::c_int {
    if msg.is_null() || eom.is_null() {
        return failed();
    }
    let Some(total) = (eom as usize).checked_sub(msg as usize) else {
        return failed();
    };
    // SAFETY: the caller grants one readable message of `total` bytes.
    let message = unsafe { core::slice::from_raw_parts(msg, total) };
    match raven_libc_resolv::ns_skiprr(message, 0, total, section, count) {
        Some(value) => core::ffi::c_int::try_from(value).unwrap_or(-1),
        None => failed(),
    }
}
