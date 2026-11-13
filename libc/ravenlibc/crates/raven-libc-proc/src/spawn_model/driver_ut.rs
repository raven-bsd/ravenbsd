// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `posix_spawn` driver.

use super::*;
use crate::errors::{EACCES, ENOEXEC};
use crate::fake::FakeProcd;
use crate::spawn_model::api::{FileAction, FileActions, SpawnAttr};
use crate::wire::FdMapEntry;
use alloc::vec;
use alloc::vec::Vec;

struct DriverEnv {
    opens: Vec<u32>,
    closed: Vec<u32>,
}

impl DriverEnv {
    fn new(opens: Vec<u32>) -> Self {
        Self {
            opens,
            closed: Vec::new(),
        }
    }
}

impl SpawnEnv for DriverEnv {
    fn open(&mut self, _path: &[u8], _oflag: i32, _mode: u32) -> Result<u32, Errno> {
        if self.opens.is_empty() {
            return Err(ENOSYS);
        }
        Ok(self.opens.remove(0))
    }

    fn is_dir(&mut self, _handle: u32) -> Result<bool, Errno> {
        Ok(true)
    }

    fn close_temp(&mut self, handle: u32) {
        self.closed.push(handle);
    }

    fn fd_path(&mut self, _handle: u32) -> Option<Vec<u8>> {
        Some(b"/".to_vec())
    }
}

fn ctx<'a>(table: &'a [FdEntry], cwd: &'a [u8]) -> SpawnCtx<'a> {
    SpawnCtx {
        table,
        cwd,
        sigmask: [3; 128],
        ignored: [4; 128],
        umask: 0o022,
    }
}

fn spawn_request(procd: &FakeProcd) -> SpawnRequest {
    match procd.last_request() {
        Some(crate::wire::Request::Spawn(request)) => request,
        other => panic!("unexpected request {other:?}"),
    }
}

#[test]
fn success_returns_zero_and_the_child_pid() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &procd,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            None,
            None,
            &[b"true".to_vec()],
            &[]
        ),
        0
    );
    assert_eq!(
        spawn_model_pid(
            &procd,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            None,
            None,
            &[b"true".to_vec()],
            &[]
        ),
        Ok(101)
    );
}

#[test]
fn a_procd_error_is_returned_as_the_value() {
    let procd = FakeProcd::new();
    procd.fail_next(crate::wire::Op::Spawn, EACCES);
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &procd,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            None,
            None,
            &[b"true".to_vec()],
            &[]
        ),
        EACCES
    );
}

#[test]
fn the_request_carries_the_inherited_fdmap() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(Vec::new());
    let table = [FdEntry {
        fd: 1,
        handle: 7,
        cloexec: false,
    }];
    let context = ctx(&table, b"/tmp");
    posix_spawn_model(
        &procd,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        None,
        None,
        &[b"true".to_vec()],
        &[],
    );
    let request = spawn_request(&procd);
    assert_eq!(request.fdmap, vec![FdMapEntry { fd: 1, handle: 7 }]);
    assert_eq!(request.cwd, b"/tmp".to_vec());
    assert_eq!(request.umask, 0o022);
    assert_eq!(request.sigmask, [3; 128]);
    assert_eq!(request.ignored, [4; 128]);
}

#[test]
fn temporary_handles_are_closed_after_success() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(vec![9]);
    let mut actions = FileActions::new();
    actions.add_open(3, b"/dev/null", 0, 0).expect("open");
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &procd,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        Some(&actions),
        None,
        &[b"true".to_vec()],
        &[],
    );
    assert_eq!(env.closed, vec![9]);
    assert_eq!(
        spawn_request(&procd).fdmap,
        vec![FdMapEntry { fd: 3, handle: 9 }]
    );
}

#[test]
fn temporary_handles_are_closed_after_failure() {
    let procd = FakeProcd::new();
    procd.fail_next(crate::wire::Op::Spawn, EACCES);
    let mut env = DriverEnv::new(vec![9]);
    let mut actions = FileActions::new();
    actions.add_open(3, b"/dev/null", 0, 0).expect("open");
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &procd,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            Some(&actions),
            None,
            &[b"true".to_vec()],
            &[]
        ),
        EACCES
    );
    assert_eq!(env.closed, vec![9]);
}

#[test]
fn an_empty_path_is_enoent_without_a_request() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &procd,
            &mut env,
            &context,
            b"",
            false,
            None,
            None,
            None,
            &[],
            &[]
        ),
        ENOENT
    );
    assert!(procd.requests().is_empty());
}

#[test]
fn too_many_arguments_are_rejected_before_a_request() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    let argv = vec![b"x".to_vec(); MAX_ARGS + 1];
    assert_eq!(
        posix_spawn_model(
            &procd,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            None,
            None,
            &argv,
            &[]
        ),
        E2BIG
    );
    assert!(procd.requests().is_empty());
}

#[test]
fn too_many_argument_bytes_are_rejected_before_a_request() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    let argv = vec![vec![b'x'; ARG_MAX]];
    assert_eq!(
        posix_spawn_model(
            &procd,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            None,
            None,
            &argv,
            &[]
        ),
        E2BIG
    );
    assert!(procd.requests().is_empty());
}

#[test]
fn a_path_search_tries_each_element() {
    let procd = FakeProcd::new();
    procd.push_spawn_result(Err(ENOENT));
    procd.push_spawn_result(Ok(100));
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &procd,
            &mut env,
            &context,
            b"tool",
            true,
            Some(b"/a:/b"),
            None,
            None,
            &[b"tool".to_vec()],
            &[]
        ),
        0
    );
    let requests = procd.requests();
    let paths: Vec<Vec<u8>> = requests
        .iter()
        .map(|request| match request {
            crate::wire::Request::Spawn(spawn) => spawn.path.clone(),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(paths, vec![b"/a/tool".to_vec(), b"/b/tool".to_vec()]);
}

#[test]
fn enoexec_retries_with_the_shell() {
    let procd = FakeProcd::new();
    procd.push_spawn_result(Err(ENOEXEC));
    procd.push_spawn_result(Ok(100));
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &procd,
            &mut env,
            &context,
            b"script",
            true,
            Some(b"/a"),
            None,
            None,
            &[b"script".to_vec(), b"one".to_vec()],
            &[]
        ),
        0
    );
    let requests = procd.requests();
    assert_eq!(requests.len(), 2);
    match &requests[1] {
        crate::wire::Request::Spawn(spawn) => {
            assert_eq!(spawn.path, b"/bin/sh".to_vec());
            assert_eq!(
                spawn.argv,
                vec![b"sh".to_vec(), b"/a/script".to_vec(), b"one".to_vec()]
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_slash_name_is_a_direct_spawn() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &procd,
        &mut env,
        &context,
        b"/bin/tool",
        true,
        Some(b"/a"),
        None,
        None,
        &[b"tool".to_vec()],
        &[],
    );
    assert_eq!(procd.requests().len(), 1);
    assert_eq!(spawn_request(&procd).path, b"/bin/tool".to_vec());
}

#[test]
fn attributes_are_forwarded() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(Vec::new());
    let mut attr = SpawnAttr::new();
    attr.set_flags(
        crate::consts::POSIX_SPAWN_SETPGROUP
            | crate::consts::POSIX_SPAWN_SETSIGMASK
            | crate::consts::POSIX_SPAWN_SETSIGDEF
            | crate::consts::POSIX_SPAWN_SETSCHEDULER,
    )
    .expect("flags");
    attr.set_pgroup(7);
    attr.set_sigmask([1; 128]);
    attr.set_sigdefault([2; 128]);
    attr.set_sched_policy(1);
    attr.set_sched_priority(9);
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &procd,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        None,
        Some(&attr),
        &[b"true".to_vec()],
        &[],
    );
    let request = spawn_request(&procd);
    assert_eq!(request.sigmask, [1; 128]);
    assert_eq!(request.sigdefault, [2; 128]);
    assert_eq!(request.pgroup, 7);
    assert_eq!(request.sched_policy, 1);
    assert_eq!(request.sched_prio, 9);
}

#[test]
fn cloexec_default_sets_the_hint_and_hides_inherited_fds() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(Vec::new());
    let mut attr = SpawnAttr::new();
    attr.set_flags(POSIX_SPAWN_CLOEXEC_DEFAULT).expect("flags");
    let table = [FdEntry {
        fd: 1,
        handle: 7,
        cloexec: false,
    }];
    let context = ctx(&table, b"/");
    posix_spawn_model(
        &procd,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        None,
        Some(&attr),
        &[b"true".to_vec()],
        &[],
    );
    let request = spawn_request(&procd);
    assert_eq!(request.spawn_flags & S_CLOEXEC_DEFAULT, S_CLOEXEC_DEFAULT);
    assert!(request.fdmap.is_empty());
}

#[test]
fn a_path_search_sets_the_path_hint() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &procd,
        &mut env,
        &context,
        b"true",
        true,
        Some(b"/bin"),
        None,
        None,
        &[b"true".to_vec()],
        &[],
    );
    assert_eq!(
        spawn_request(&procd).spawn_flags & S_USE_PATH_HINT,
        S_USE_PATH_HINT
    );
}

#[test]
fn an_open_action_is_in_the_request_and_closed() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(vec![9]);
    let mut actions = FileActions::new();
    actions.add_open(3, b"/dev/null", 0, 0).expect("open");
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &procd,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        Some(&actions),
        None,
        &[b"true".to_vec()],
        &[],
    );
    assert!(matches!(actions.actions()[0], FileAction::Open { .. }));
    assert_eq!(env.closed, vec![9]);
    assert_eq!(
        spawn_request(&procd).fdmap,
        vec![FdMapEntry { fd: 3, handle: 9 }]
    );
}

#[test]
fn spawn_with_mask_sets_both_signal_flags() {
    let procd = FakeProcd::new();
    let context = ctx(&[], b"/");
    assert_eq!(
        spawn_with_mask(
            &procd,
            &context,
            b"/bin/sh",
            &[b"sh".to_vec()],
            &[],
            [5; 128],
            [6; 128]
        ),
        Ok(100)
    );
    let request = spawn_request(&procd);
    assert_eq!(
        request.spawn_flags & 0x000c,
        (POSIX_SPAWN_SETSIGMASK | POSIX_SPAWN_SETSIGDEF) as u32
    );
    assert_eq!(request.sigmask, [5; 128]);
    assert_eq!(request.sigdefault, [6; 128]);
}

#[test]
fn tcsetpgrp_is_forwarded() {
    let procd = FakeProcd::new();
    let mut env = DriverEnv::new(Vec::new());
    let mut actions = FileActions::new();
    actions.add_tcsetpgrp(2).expect("tcsetpgrp");
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &procd,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        Some(&actions),
        None,
        &[b"true".to_vec()],
        &[],
    );
    assert_eq!(spawn_request(&procd).tcpgrp_fd, 2);
}
