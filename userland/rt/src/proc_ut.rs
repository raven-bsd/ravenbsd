// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the credential-word decoding.

use super::*;

#[test]
fn credential_words_decode_in_kernel_order() {
    let credentials = decode_credentials([1000, 0, 0, 1001, 0, 0]);
    assert_eq!(credentials.uid, 1000);
    assert_eq!(credentials.euid, 0);
    assert_eq!(credentials.gid, 1001);
    assert_eq!(credentials.egid, 0);
    assert_eq!(credentials.euid(), 0);
    assert_eq!(credentials.egid(), 0);
}

#[test]
fn a_word_larger_than_u32_falls_back_to_zero() {
    let credentials = decode_credentials([u64::MAX, 0, 0, 0, 0, 0]);
    assert_eq!(credentials.uid, 0);
}

#[test]
fn the_group_limit_matches_the_kernel() {
    assert_eq!(MAX_GROUPS, 16);
}

#[test]
fn set_command_is_enosys_on_the_host() {
    assert_eq!(set_command(Handle::SELF_PROC, "worker"), Err(Errno::ENOSYS));
}
