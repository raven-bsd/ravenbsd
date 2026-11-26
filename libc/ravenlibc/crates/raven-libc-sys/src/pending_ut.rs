// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for [`crate::pending`].

#![allow(clippy::indexing_slicing)]

use super::*;

#[test]
fn every_pending_entry_returns_enosys() {
    assert_eq!(thread_set_tls(core::ptr::null_mut()), Err(Errno::ENOSYS));
    assert_eq!(thread_kick(Handle::SELF_PROC, 1), Err(Errno::ENOSYS));
    assert_eq!(poll_once(&AtomicU32::new(0), 0), Err(Errno::ENOSYS));
    assert_eq!(proc_fork(Handle::SELF_PROC, 0), Err(Errno::ENOSYS));
    assert_eq!(clock_set(0, Timespec::default()), Err(Errno::ENOSYS));
    assert_eq!(sysinfo(), Err(Errno::ENOSYS));
    assert_eq!(madvise(Handle::SELF_PROC, 0, 4096, 0), Err(Errno::ENOSYS));
}

#[test]
fn getrandom_fills_from_the_seeded_fake_pool() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    crate::fake::seed_entropy(&[0x11; 8]);
    let mut buffer = [0u8; 8];
    assert_eq!(getrandom(&mut buffer, 0), Ok(8));
    assert_eq!(buffer, [0x11; 8]);
}

#[test]
fn getrandom_without_a_seeded_pool_is_eagain() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    let mut buffer = [0u8; 4];
    assert_eq!(getrandom(&mut buffer, 0), Err(Errno::EAGAIN));
}

#[test]
fn getrandom_rejects_unknown_flags() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    crate::fake::seed_entropy(&[0x22; 8]);
    let mut buffer = [0u8; 4];
    assert_eq!(getrandom(&mut buffer, 4), Err(Errno::EINVAL));
    assert_eq!(getrandom(&mut buffer, 0x8000), Err(Errno::EINVAL));
}

#[test]
fn getrandom_zero_length_is_zero() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    assert_eq!(getrandom(&mut [], 0), Ok(0));
}

#[test]
fn getrandom_accepts_nonblock_and_random_flags() {
    let _guard = crate::fake::test_lock();
    crate::fake::reset();
    crate::fake::seed_entropy(&[0x33; 4]);
    let mut buffer = [0u8; 4];
    assert_eq!(getrandom(&mut buffer, 1 | 2), Ok(4));
    assert_eq!(buffer, [0x33; 4]);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn pending_flag_is_set() {
    assert!(PENDING);
}

#[test]
fn sysinfo_layout_is_stable() {
    assert_eq!(core::mem::size_of::<SysInfo>(), 8 + 8 + 8 + 8);
    assert_eq!(core::mem::align_of::<SysInfo>(), 8);
}

#[test]
fn pending_stubs_do_not_touch_errno() {
    let previous = crate::errno::errno();
    let _ = sysinfo();
    assert_eq!(crate::errno::errno(), previous);
}
