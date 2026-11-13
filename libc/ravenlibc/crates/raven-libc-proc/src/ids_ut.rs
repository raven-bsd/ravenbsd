// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the credential builders.

use super::*;

#[test]
fn setuid_builder_has_kind_one() {
    let request = SetCred::setuid(1000);
    assert_eq!(request.kind, 1);
    assert_eq!(request.a, 1000);
    assert_eq!(request.b, -1);
    assert_eq!(request.c, -1);
}

#[test]
fn seteuid_builder_has_kind_two() {
    assert_eq!(SetCred::seteuid(7).kind, 2);
}

#[test]
fn setreuid_builder_keeps_minus_one() {
    let request = SetCred::setreuid(-1, 5);
    assert_eq!(request.kind, 3);
    assert_eq!((request.a, request.b), (-1, 5));
}

#[test]
fn setresuid_builder_has_three_values() {
    let request = SetCred::setresuid(1, -1, 3);
    assert_eq!(request.kind, 4);
    assert_eq!((request.a, request.b, request.c), (1, -1, 3));
}

#[test]
fn group_builders_have_the_right_kinds() {
    assert_eq!(SetCred::setgid(1).kind, 5);
    assert_eq!(SetCred::setegid(1).kind, 6);
    assert_eq!(SetCred::setregid(-1, 2).kind, 7);
    assert_eq!(SetCred::setresgid(1, 2, 3).kind, 8);
    assert_eq!(SetCred::setgroups(&[1, 2]).expect("groups").kind, 9);
}

#[test]
fn group_list_is_copied() {
    let request = SetCred::setgroups(&[3, 4, 5]).expect("groups");
    assert_eq!(request.groups, alloc::vec![3, 4, 5]);
}

#[test]
fn too_many_groups_is_invalid() {
    let groups = alloc::vec![0u32; NGROUPS_MAX + 1];
    assert_eq!(SetCred::setgroups(&groups), Err(EINVAL));
    assert_eq!(NGROUPS_MAX, crate::limits::NGROUPS_MAX, "limit re-export");
}

#[test]
fn validate_accepts_every_kind() {
    for kind in 1..=9u32 {
        let mut request = SetCred::setuid(0);
        request.kind = kind;
        assert_eq!(request.validate(), Ok(()));
    }
}

#[test]
fn validate_rejects_kind_zero_and_ten() {
    let mut request = SetCred::setuid(0);
    request.kind = 0;
    assert_eq!(request.validate(), Err(EINVAL));
    request.kind = 10;
    assert_eq!(request.validate(), Err(EINVAL));
}

#[test]
fn issetugid_is_false() {
    assert!(!issetugid());
}
