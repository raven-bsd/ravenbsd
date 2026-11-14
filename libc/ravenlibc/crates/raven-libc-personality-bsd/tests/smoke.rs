// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Integration smoke test of the public BSD personality surface.

use raven_libc_personality_bsd::kqueue::{self, Kevent, Kqueue};
use raven_libc_personality_bsd::sysctl::{self, TABLE};

#[test]
fn sysctl_table_is_the_twelve_documented_rows() {
    assert_eq!(TABLE.len(), 12);
    assert!(sysctl::by_name(b"kern.ostype").is_some());
    assert!(sysctl::by_name(b"hw.pagesize").is_some());
    assert!(sysctl::by_name(b"kern.proc.pid.1").is_none());
}

#[test]
fn kevent_layout_is_64_bytes() {
    assert_eq!(core::mem::size_of::<Kevent>(), 64);
    assert_eq!(Kevent::default().filter, 0);
    assert_eq!(kqueue::EVFILT_READ, -1);
}

#[test]
fn kqueue_state_starts_empty() {
    let state = Kqueue::new();
    assert!(state.is_empty(3));
}
