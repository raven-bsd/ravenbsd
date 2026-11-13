// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for [`crate::pending`].

#![allow(clippy::indexing_slicing)]

use super::*;

#[test]
fn every_pending_entry_returns_enosys() {
    assert_eq!(thread_set_tls(core::ptr::null_mut()), Err(Errno::ENOSYS));
    assert_eq!(thread_kick(Handle::SELF_PROC, 1), Err(Errno::ENOSYS));
    assert_eq!(getrandom(&mut [], 0), Err(Errno::ENOSYS));
    assert_eq!(poll_once(&AtomicU32::new(0), 0), Err(Errno::ENOSYS));
    assert_eq!(proc_fork(Handle::SELF_PROC, 0), Err(Errno::ENOSYS));
    assert_eq!(clock_set(0, Timespec::default()), Err(Errno::ENOSYS));
    assert_eq!(sysinfo(), Err(Errno::ENOSYS));
    assert_eq!(madvise(Handle::SELF_PROC, 0, 4096, 0), Err(Errno::ENOSYS));
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
