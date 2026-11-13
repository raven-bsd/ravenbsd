// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `popen` model.

use super::*;
use crate::spawn_model::api::FileAction;

#[test]
fn the_four_modes_parse() {
    let read = popen_mode(b"r").expect("r");
    assert!(read.read && !read.write && !read.cloexec);
    let write = popen_mode(b"w").expect("w");
    assert!(!write.read && write.write && !write.cloexec);
    let read_e = popen_mode(b"re").expect("re");
    assert!(read_e.read && read_e.cloexec);
    let write_e = popen_mode(b"we").expect("we");
    assert!(write_e.write && write_e.cloexec);
}

#[test]
fn other_modes_are_invalid() {
    assert_eq!(popen_mode(b"a"), Err(EINVAL));
    assert_eq!(popen_mode(b"rw"), Err(EINVAL));
    assert_eq!(popen_mode(b""), Err(EINVAL));
}

#[test]
fn the_pid_list_inserts_removes_and_lists() {
    let mut list = PidList::new();
    assert!(list.is_empty());
    list.insert(3, 100);
    list.insert(4, 101);
    assert_eq!(list.fds(), alloc::vec![3, 4]);
    assert_eq!(list.remove(3), Some(100));
    assert_eq!(list.remove(3), None);
    assert_eq!(list.len(), 1);
}

#[test]
fn a_read_plan_dups_to_stdout_and_closes_both_ends() {
    let (actions, argv) = popen_plan(
        popen_mode(b"r").expect("mode"),
        9,
        8,
        &PidList::new(),
        b"echo hi",
    );
    assert_eq!(
        argv,
        alloc::vec![b"sh".to_vec(), b"-c".to_vec(), b"echo hi".to_vec()]
    );
    assert_eq!(
        actions.actions(),
        &[
            FileAction::Dup2 { fd: 8, newfd: 1 },
            FileAction::Close { fd: 8 },
            FileAction::Close { fd: 9 }
        ]
    );
}

#[test]
fn a_write_plan_dups_to_stdin() {
    let (actions, _) = popen_plan(
        popen_mode(b"w").expect("mode"),
        9,
        8,
        &PidList::new(),
        b"cat",
    );
    assert!(matches!(
        actions.actions()[0],
        FileAction::Dup2 { fd: 8, newfd: 0 }
    ));
}

#[test]
fn earlier_popen_fds_are_closed_first() {
    let mut list = PidList::new();
    list.insert(3, 100);
    list.insert(4, 101);
    let (actions, _) = popen_plan(popen_mode(b"r").expect("mode"), 9, 8, &list, b"true");
    assert_eq!(
        actions.actions(),
        &[
            FileAction::Close { fd: 3 },
            FileAction::Close { fd: 4 },
            FileAction::Dup2 { fd: 8, newfd: 1 },
            FileAction::Close { fd: 8 },
            FileAction::Close { fd: 9 }
        ]
    );
}
