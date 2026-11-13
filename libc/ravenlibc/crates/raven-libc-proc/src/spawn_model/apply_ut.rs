// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the scratch fd-table applier.

use super::*;
use crate::errors::EIO;
use alloc::vec;
use alloc::vec::Vec;

struct FakeEnv {
    opens: Vec<Result<u32, Errno>>,
    dirs: Vec<(u32, bool)>,
    paths: Vec<(u32, Vec<u8>)>,
    closed: Vec<u32>,
}

impl FakeEnv {
    fn new(opens: Vec<Result<u32, Errno>>) -> Self {
        Self {
            opens,
            dirs: Vec::new(),
            paths: Vec::new(),
            closed: Vec::new(),
        }
    }

    fn with_dir(mut self, handle: u32, is_dir: bool) -> Self {
        self.dirs.push((handle, is_dir));
        self
    }

    fn with_path(mut self, handle: u32, path: &[u8]) -> Self {
        self.paths.push((handle, path.to_vec()));
        self
    }
}

impl SpawnEnv for FakeEnv {
    fn open(&mut self, _path: &[u8], _oflag: i32, _mode: u32) -> Result<u32, Errno> {
        if self.opens.is_empty() {
            return Err(EIO);
        }
        self.opens.remove(0)
    }

    fn is_dir(&mut self, handle: u32) -> Result<bool, Errno> {
        Ok(self
            .dirs
            .iter()
            .find(|(candidate, _)| *candidate == handle)
            .is_some_and(|(_, is_dir)| *is_dir))
    }

    fn close_temp(&mut self, handle: u32) {
        self.closed.push(handle);
    }

    fn fd_path(&mut self, handle: u32) -> Option<Vec<u8>> {
        self.paths
            .iter()
            .find(|(candidate, _)| *candidate == handle)
            .map(|(_, path)| path.clone())
    }
}

fn entry(fd: i32, handle: u32, cloexec: bool) -> FdEntry {
    FdEntry {
        fd,
        handle,
        cloexec,
    }
}

#[test]
fn open_records_a_temp_and_replaces_the_fd() {
    let mut env = FakeEnv::new(vec![Ok(9)]);
    let mut actions = FileActions::new();
    actions.add_open(3, b"/dev/null", 0, 0).expect("open");
    let table = vec![entry(3, 1, true)];
    let scratch = apply_actions(&table, b"/", false, &actions, &mut env).expect("apply");
    assert_eq!(scratch.temp_handles, vec![9]);
    assert_eq!(scratch.fdmap, vec![FdMapEntry { fd: 3, handle: 9 }]);
    assert!(env.closed.is_empty());
}

#[test]
fn a_failed_open_closes_the_temporaries_opened_so_far() {
    let mut env = FakeEnv::new(vec![Ok(9), Err(EIO)]);
    let mut actions = FileActions::new();
    actions.add_open(3, b"/a", 0, 0).expect("open");
    actions.add_open(4, b"/b", 0, 0).expect("open");
    let result = apply_actions(&[], b"/", false, &actions, &mut env);
    assert_eq!(result, Err(EIO));
    assert_eq!(env.closed, vec![9]);
}

#[test]
fn dup2_copies_and_clears_cloexec() {
    let mut env = FakeEnv::new(vec![]);
    let mut actions = FileActions::new();
    actions.add_dup2(1, 5).expect("dup2");
    let table = vec![entry(1, 7, false)];
    let scratch = apply_actions(&table, b"/", false, &actions, &mut env).expect("apply");
    assert_eq!(
        scratch.fdmap,
        vec![
            FdMapEntry { fd: 1, handle: 7 },
            FdMapEntry { fd: 5, handle: 7 }
        ]
    );
}

#[test]
fn dup2_to_the_same_fd_still_clears_cloexec() {
    let mut env = FakeEnv::new(vec![]);
    let mut actions = FileActions::new();
    actions.add_dup2(1, 1).expect("dup2");
    let table = vec![entry(1, 7, true)];
    let scratch = apply_actions(&table, b"/", false, &actions, &mut env).expect("apply");
    assert_eq!(scratch.fdmap, vec![FdMapEntry { fd: 1, handle: 7 }]);
}

#[test]
fn dup2_from_a_missing_fd_is_ebadf() {
    let mut env = FakeEnv::new(vec![]);
    let mut actions = FileActions::new();
    actions.add_dup2(9, 5).expect("dup2");
    assert_eq!(
        apply_actions(&[], b"/", false, &actions, &mut env),
        Err(EBADF)
    );
}

#[test]
fn close_of_a_free_fd_is_a_no_op() {
    let mut env = FakeEnv::new(vec![]);
    let mut actions = FileActions::new();
    actions.add_close(99).expect("close");
    let scratch =
        apply_actions(&[entry(1, 7, false)], b"/", false, &actions, &mut env).expect("apply");
    assert_eq!(scratch.fdmap, vec![FdMapEntry { fd: 1, handle: 7 }]);
}

#[test]
fn close_removes_the_fd() {
    let mut env = FakeEnv::new(vec![]);
    let mut actions = FileActions::new();
    actions.add_close(1).expect("close");
    let scratch =
        apply_actions(&[entry(1, 7, false)], b"/", false, &actions, &mut env).expect("apply");
    assert!(scratch.fdmap.is_empty());
}

#[test]
fn chdir_resolves_relative_paths() {
    let mut env = FakeEnv::new(vec![]);
    let mut actions = FileActions::new();
    actions.add_chdir(b"a/b").expect("chdir");
    actions.add_chdir(b"../c").expect("chdir");
    let scratch = apply_actions(&[], b"/one/two", false, &actions, &mut env).expect("apply");
    assert_eq!(scratch.cwd, b"/one/two/a/c".to_vec());
}

#[test]
fn chdir_starts_from_an_absolute_path() {
    let mut env = FakeEnv::new(vec![]);
    let mut actions = FileActions::new();
    actions.add_chdir(b"/x/../y").expect("chdir");
    let scratch = apply_actions(&[], b"/one/two", false, &actions, &mut env).expect("apply");
    assert_eq!(scratch.cwd, b"/y".to_vec());
}

#[test]
fn dotdot_at_the_root_stays_at_the_root() {
    assert_eq!(resolve(b"/", b".."), b"/");
    assert_eq!(resolve(b"/a", b"../../.."), b"/");
}

#[test]
fn fchdir_needs_a_directory() {
    let mut env = FakeEnv::new(vec![]).with_dir(5, false);
    let mut actions = FileActions::new();
    actions.add_fchdir(1).expect("fchdir");
    let table = vec![entry(1, 5, false)];
    assert_eq!(
        apply_actions(&table, b"/", false, &actions, &mut env),
        Err(ENOTDIR)
    );
}

#[test]
fn fchdir_without_a_path_is_enosys() {
    let mut env = FakeEnv::new(vec![]).with_dir(5, true);
    let mut actions = FileActions::new();
    actions.add_fchdir(1).expect("fchdir");
    let table = vec![entry(1, 5, false)];
    assert_eq!(
        apply_actions(&table, b"/", false, &actions, &mut env),
        Err(ENOSYS)
    );
}

#[test]
fn fchdir_with_a_path_updates_the_cwd() {
    let mut env = FakeEnv::new(vec![])
        .with_dir(5, true)
        .with_path(5, b"/tmp/dir");
    let mut actions = FileActions::new();
    actions.add_fchdir(1).expect("fchdir");
    let table = vec![entry(1, 5, false)];
    let scratch = apply_actions(&table, b"/", false, &actions, &mut env).expect("apply");
    assert_eq!(scratch.cwd, b"/tmp/dir".to_vec());
}

#[test]
fn closefrom_drops_high_fds() {
    let mut env = FakeEnv::new(vec![]);
    let mut actions = FileActions::new();
    actions.add_closefrom(10).expect("closefrom");
    let table = vec![entry(9, 1, false), entry(10, 2, false), entry(11, 3, false)];
    let scratch = apply_actions(&table, b"/", false, &actions, &mut env).expect("apply");
    assert_eq!(scratch.fdmap, vec![FdMapEntry { fd: 9, handle: 1 }]);
}

#[test]
fn cloexec_default_hides_every_inherited_fd() {
    let mut env = FakeEnv::new(vec![]);
    let actions = FileActions::new();
    let table = vec![entry(1, 7, false)];
    let scratch = apply_actions(&table, b"/", true, &actions, &mut env).expect("apply");
    assert!(scratch.fdmap.is_empty());
}

#[test]
fn an_open_after_cloexec_default_survives() {
    let mut env = FakeEnv::new(vec![Ok(9)]);
    let mut actions = FileActions::new();
    actions.add_open(3, b"/dev/null", 0, 0).expect("open");
    let table = vec![entry(1, 7, false)];
    let scratch = apply_actions(&table, b"/", true, &actions, &mut env).expect("apply");
    assert_eq!(scratch.fdmap, vec![FdMapEntry { fd: 3, handle: 9 }]);
}

#[test]
fn tcsetpgrp_is_remembered() {
    let mut env = FakeEnv::new(vec![]);
    let mut actions = FileActions::new();
    actions.add_tcsetpgrp(2).expect("tcsetpgrp");
    let scratch = apply_actions(&[], b"/", false, &actions, &mut env).expect("apply");
    assert_eq!(scratch.tcsetpgrp_fd, 2);
}

#[test]
fn the_fdmap_is_sorted_by_fd() {
    let mut env = FakeEnv::new(vec![]);
    let actions = FileActions::new();
    let table = vec![entry(7, 1, false), entry(2, 2, false), entry(5, 3, false)];
    let scratch = apply_actions(&table, b"/", false, &actions, &mut env).expect("apply");
    assert_eq!(
        scratch.fdmap,
        vec![
            FdMapEntry { fd: 2, handle: 2 },
            FdMapEntry { fd: 5, handle: 3 },
            FdMapEntry { fd: 7, handle: 1 }
        ]
    );
}

#[test]
fn an_empty_cwd_becomes_root() {
    let mut env = FakeEnv::new(vec![]);
    let scratch = apply_actions(&[], b"", false, &FileActions::new(), &mut env).expect("apply");
    assert_eq!(scratch.cwd, b"/".to_vec());
}
