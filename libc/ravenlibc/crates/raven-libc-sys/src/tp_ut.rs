// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for [`crate::tp`].

#![allow(clippy::indexing_slicing)]

use super::*;

#[cfg(all(target_arch = "x86_64", not(feature = "host-fake")))]
#[test]
fn x86_64_setter_is_pending_pg1() {
    // SAFETY: the call is rejected before touching any register.
    assert_eq!(
        unsafe { set_thread_pointer(0x1000 as *mut u8) },
        Err(Errno::ENOSYS)
    );
}

#[cfg(feature = "host-fake")]
#[test]
fn host_fake_roundtrips_the_thread_pointer() {
    // SAFETY: the fake setter only stores the value in a static cell.
    unsafe { set_thread_pointer(0x1234_5000 as *mut u8) }.unwrap();
    assert_eq!(thread_pointer(), 0x1234_5000 as *mut u8);
}

#[cfg(feature = "host-fake")]
#[test]
fn host_fake_accepts_null() {
    // SAFETY: the fake setter only stores the value in a static cell.
    unsafe { set_thread_pointer(core::ptr::null_mut()) }.unwrap();
    assert!(thread_pointer().is_null());
}

#[test]
fn reader_is_stable_without_a_setter() {
    let first = thread_pointer();
    let second = thread_pointer();
    assert_eq!(first, second);
}
