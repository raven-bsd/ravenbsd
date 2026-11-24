// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for scheme-namespace prefix validation.

use super::{NAME_MAX, allow_prefix, valid_prefix};
use raven_abi::errno::Errno;
use raven_abi::types::Handle;

#[test]
fn accepts_the_kernel_scheme_alphabet() {
    for prefix in ["block", "disk", "ufs", "net-tcp", "svc", "log2", "file"] {
        assert!(valid_prefix(prefix), "{prefix}");
    }
    assert!(valid_prefix(&"a".repeat(NAME_MAX)));
}

#[test]
fn rejects_empty_oversized_and_foreign_prefixes() {
    assert!(!valid_prefix(""));
    assert!(!valid_prefix(&"a".repeat(NAME_MAX + 1)));
    assert!(!valid_prefix("Block"));
    assert!(!valid_prefix("block/0"));
    assert!(!valid_prefix("block 0"));
}

#[test]
fn invalid_prefixes_fail_before_the_syscall() {
    // The host has no Raven kernel; these calls must return from validation without executing a
    // syscall, which is what makes them safe to exercise here.
    assert_eq!(allow_prefix(Handle::SELF_PROC, ""), Err(Errno::EINVAL));
    assert_eq!(
        allow_prefix(Handle::SELF_PROC, "block/0"),
        Err(Errno::EINVAL)
    );
}
