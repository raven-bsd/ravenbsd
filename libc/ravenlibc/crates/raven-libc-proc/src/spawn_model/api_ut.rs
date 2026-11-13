// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the spawn attribute and file-action API.

use super::*;
use crate::consts::{
    POSIX_SPAWN_CLOEXEC_DEFAULT, POSIX_SPAWN_RESETIDS, POSIX_SPAWN_SETSID, POSIX_SPAWN_SETSIGMASK,
};

#[test]
fn a_new_attribute_is_zeroed() {
    let attr = SpawnAttr::new();
    assert_eq!(attr.flags, 0);
    assert_eq!(attr.pgroup, 0);
    assert_eq!(attr.sigdefault, [0; 128]);
    assert_eq!(attr.sigmask, [0; 128]);
    assert_eq!(attr.sched_policy, 0);
    assert_eq!(attr.sched_priority, 0);
}

#[test]
fn all_linux_flag_bits_are_accepted() {
    let mut attr = SpawnAttr::new();
    let flags = POSIX_SPAWN_RESETIDS
        | POSIX_SPAWN_SETSID
        | POSIX_SPAWN_SETSIGMASK
        | POSIX_SPAWN_CLOEXEC_DEFAULT;
    assert_eq!(attr.set_flags(flags), Ok(()));
    assert_eq!(attr.flags(), flags);
}

#[test]
fn unknown_flag_bits_are_rejected() {
    let mut attr = SpawnAttr::new();
    assert_eq!(attr.set_flags(0x4000), Err(EINVAL));
    assert_eq!(attr.set_flags(0x0100), Err(EINVAL));
}

#[test]
fn freebsd_overlapping_bits_are_linux_bits() {
    let mut attr = SpawnAttr::new();
    assert_eq!(attr.set_flags(4), Ok(()));
    assert_eq!(attr.set_flags(8), Ok(()));
    assert_eq!(attr.set_flags(0x10), Ok(()));
    assert_eq!(attr.set_flags(0x20), Ok(()));
}

#[test]
fn attribute_fields_round_trip() {
    let mut attr = SpawnAttr::new();
    attr.set_pgroup(7);
    attr.set_sigdefault([3; 128]);
    attr.set_sigmask([4; 128]);
    attr.set_sched_policy(1);
    attr.set_sched_priority(9);
    assert_eq!(attr.pgroup(), 7);
    assert_eq!(attr.sigdefault(), [3; 128]);
    assert_eq!(attr.sigmask(), [4; 128]);
    assert_eq!(attr.sched_policy(), 1);
    assert_eq!(attr.sched_priority(), 9);
}

#[test]
fn a_new_action_list_is_empty() {
    let actions = FileActions::new();
    assert!(actions.is_empty());
    assert_eq!(actions.len(), 0);
    assert!(actions.actions().is_empty());
}

#[test]
fn actions_keep_append_order() {
    let mut actions = FileActions::new();
    actions.add_open(3, b"/dev/null", 0, 0).expect("open");
    actions.add_dup2(3, 0).expect("dup2");
    actions.add_close(3).expect("close");
    assert_eq!(actions.len(), 3);
    assert!(matches!(actions.actions()[0], FileAction::Open { .. }));
    assert!(matches!(actions.actions()[1], FileAction::Dup2 { .. }));
    assert!(matches!(actions.actions()[2], FileAction::Close { .. }));
}

#[test]
fn negative_descriptors_are_ebadf_for_every_adder() {
    let mut actions = FileActions::new();
    assert_eq!(actions.add_open(-1, b"/x", 0, 0), Err(EBADF));
    assert_eq!(actions.add_dup2(-1, 0), Err(EBADF));
    assert_eq!(actions.add_dup2(0, -1), Err(EBADF));
    assert_eq!(actions.add_close(-1), Err(EBADF));
    assert_eq!(actions.add_fchdir(-1), Err(EBADF));
    assert_eq!(actions.add_closefrom(-1), Err(EBADF));
    assert_eq!(actions.add_tcsetpgrp(-1), Err(EBADF));
}

#[test]
fn a_too_long_path_is_rejected() {
    let mut actions = FileActions::new();
    let path = alloc::vec![b'x'; PATH_MAX + 1];
    assert_eq!(actions.add_open(3, &path, 0, 0), Err(ENAMETOOLONG));
    assert_eq!(actions.add_chdir(&path), Err(ENAMETOOLONG));
}

#[test]
fn the_action_list_is_bounded() {
    let mut actions = FileActions::new();
    for fd in 0..MAX_FILE_ACTIONS {
        actions.add_close(fd as i32).expect("close");
    }
    assert_eq!(actions.add_close(0), Err(E2BIG));
}

#[test]
fn an_open_action_keeps_its_arguments() {
    let mut actions = FileActions::new();
    actions.add_open(9, b"/tmp/x", 2, 0o600).expect("open");
    assert_eq!(
        actions.actions(),
        &[FileAction::Open {
            fd: 9,
            path: b"/tmp/x".to_vec(),
            oflag: 2,
            mode: 0o600,
        }]
    );
}
