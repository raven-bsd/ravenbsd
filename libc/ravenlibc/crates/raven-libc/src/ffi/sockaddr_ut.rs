// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn address_types_match_the_c_layout() {
    assert_eq!(core::mem::size_of::<InAddr>(), 4);
    assert_eq!(core::mem::align_of::<InAddr>(), 4);
    assert_eq!(core::mem::size_of::<In6Addr>(), 16);
    assert_eq!(core::mem::align_of::<In6Addr>(), 4);
}

#[test]
fn family_reads_the_first_word() {
    let mut bytes = [0u8; 16];
    bytes[0..2].copy_from_slice(&(AF_INET as u16).to_ne_bytes());
    assert_eq!(family(&bytes), Some(AF_INET));
    assert_eq!(family(&bytes[..2]), None);
}

#[test]
fn view_rejects_null_with_a_length() {
    // SAFETY: the NULL plus non-zero length is the rejected condition.
    let error = unsafe { view(core::ptr::null(), 4) }.expect_err("must reject");
    assert_eq!(i32::from(error.0), raven_libc_abi::errno::EFAULT);
}

#[test]
fn length_is_capped_by_the_caller() {
    let mut out = 0u32;
    // SAFETY: one writable `socklen_t`.
    unsafe { write_len(&mut out, 128, 16) };
    assert_eq!(out, 16);
    // SAFETY: one writable `socklen_t`.
    unsafe { write_len(&mut out, 4, 16) };
    assert_eq!(out, 4);
}

const AF_INET: i32 = 2;
