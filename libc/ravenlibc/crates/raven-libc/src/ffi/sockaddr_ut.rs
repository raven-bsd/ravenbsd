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
fn family_reads_the_byte_after_sa_len() {
    // FreeBSD `struct sockaddr`: `sa_len` at 0, `sa_family` at 1.
    let mut bytes = [0u8; 16];
    bytes[0] = 16;
    bytes[1] = AF_INET as u8;
    assert_eq!(family(&bytes), Some(AF_INET));
    assert_eq!(family(&bytes[..1]), None);
}

#[test]
fn a_linux_family_word_is_not_read_at_offset_zero() {
    let mut bytes = [0u8; 16];
    bytes[0..2].copy_from_slice(&(AF_INET as u16).to_ne_bytes());
    assert_eq!(family(&bytes), Some(0));
}

#[test]
fn view_rejects_null_with_a_length() {
    // SAFETY: the NULL plus non-zero length is the rejected condition.
    let error = unsafe { view(core::ptr::null(), 4) }.expect_err("must reject");
    assert_eq!(i32::from(error.0), raven_libc_abi::errno::EFAULT);
}

#[test]
fn control_rejects_null_with_a_length() {
    let error = unsafe { control(core::ptr::null(), 1) }.expect_err("must reject");
    assert_eq!(i32::from(error.0), raven_libc_abi::errno::EFAULT);
    // SAFETY: NULL with a zero length is the empty view.
    assert_eq!(
        unsafe { control(core::ptr::null(), 0) }.expect("empty"),
        &[]
    );
}

#[test]
fn control_views_the_caller_buffer() {
    let bytes = [1u8, 2, 3];
    // SAFETY: three readable bytes at the base.
    let view = unsafe { control(bytes.as_ptr().cast(), 3) }.expect("view");
    assert_eq!(view, &[1, 2, 3]);
    let mut out = [0u8; 3];
    // SAFETY: three writable bytes at the base.
    let writable = unsafe { control_mut(out.as_mut_ptr().cast(), 3) }.expect("view");
    writable.copy_from_slice(&bytes);
    assert_eq!(out, bytes);
}

#[test]
fn length_reports_the_required_size() {
    // FreeBSD `user_getsockname`: only `min(ss_len, user_len)` bytes are copied out, but
    // `*alen` is the full `ss_len` the caller needs.
    let mut out = 0u32;
    // SAFETY: one writable `socklen_t`.
    unsafe { write_len(&mut out, 128, 128) };
    assert_eq!(out, 128);
    // SAFETY: one writable `socklen_t`.
    unsafe { write_len(&mut out, 128, 16) };
    assert_eq!(out, 128);
}

const AF_INET: i32 = 2;
