// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Registered-stub registry tests.

use raven_abi::errno::Errno;

use alloc::vec::Vec;

use crate::stubs::{STUBS, lookup, stub_errno};

#[test]
fn lookup_finds_a_known_stub() {
    let stub = lookup("pwrite").expect("pwrite is TBD");
    assert_eq!(stub.gap, "PG-22");
    assert_eq!(stub.error(), Errno::ENOSYS);
}

#[test]
fn lookup_returns_none_for_an_implemented_name() {
    assert!(lookup("read").is_none());
}

#[test]
fn stub_errno_matches_the_record() {
    assert_eq!(stub_errno("chown"), Some(Errno::ENOSYS));
    assert_eq!(stub_errno("getxattr"), Some(Errno::EOPNOTSUPP));
    assert_eq!(stub_errno("chroot"), Some(Errno::EPERM));
}

#[test]
fn every_stub_has_a_gap_and_a_nonzero_errno() {
    for stub in STUBS {
        assert!(!stub.name.is_empty());
        assert!(!stub.gap.is_empty(), "{} has no gap", stub.name);
        assert_ne!(stub.errno, 0, "{} returns success", stub.name);
    }
}

#[test]
fn stub_names_are_unique() {
    let mut names: Vec<&str> = STUBS.iter().map(|stub| stub.name).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len());
}

#[test]
fn no_stub_is_also_a_supported_primitive_name() {
    for forbidden in ["read", "write", "close", "open", "poll", "mount", "uname"] {
        assert!(lookup(forbidden).is_none(), "{forbidden}");
    }
}
