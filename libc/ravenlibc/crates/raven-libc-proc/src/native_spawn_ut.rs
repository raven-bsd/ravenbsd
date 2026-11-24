// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the P9a native spawner.

use super::*;
use crate::consts::{KERNEL_WCONTINUED, P_ALL, P_PID, WCONTINUED, WNOHANG};
use crate::errors::{EBADF, ECHILD};

use core::cell::{Cell, RefCell};

struct FakeLoader {
    image: RefCell<Vec<u8>>,
    read_path: RefCell<String>,
    load_path: RefCell<String>,
    argv: RefCell<Vec<String>>,
    cwd: RefCell<Vec<u8>>,
    handle: u32,
    code: i32,
    close_calls: Cell<usize>,
    waited: Cell<bool>,
    last_options: Cell<u32>,
}

impl FakeLoader {
    fn new(handle: u32, code: i32) -> Self {
        Self {
            image: RefCell::new(vec![0x7f, b'E', b'L', b'F']),
            read_path: RefCell::new(String::new()),
            load_path: RefCell::new(String::new()),
            argv: RefCell::new(Vec::new()),
            cwd: RefCell::new(Vec::new()),
            handle,
            code,
            close_calls: Cell::new(0),
            waited: Cell::new(false),
            last_options: Cell::new(0),
        }
    }
}

impl Loader for FakeLoader {
    fn read_image(&self, path: &str) -> Result<Vec<u8>, Errno> {
        *self.read_path.borrow_mut() = path.into();
        Ok(self.image.borrow().clone())
    }

    fn load(
        &self,
        image: &[u8],
        path: &str,
        argv: &[&str],
        _env: &[&str],
        cwd: &[u8],
        _fdmap: &[(i32, u32)],
    ) -> Result<u32, Errno> {
        assert_eq!(image, self.image.borrow().as_slice());
        *self.load_path.borrow_mut() = path.into();
        *self.cwd.borrow_mut() = cwd.to_vec();
        *self.argv.borrow_mut() = argv.iter().map(|value| (*value).into()).collect();
        self.waited.set(false);
        Ok(self.handle)
    }

    fn wait_ex(&self, _idtype: u32, id: i32, options: u32) -> Result<(i32, i32), Errno> {
        self.last_options.set(options);
        if id != 42 && id != 0 {
            return Err(ECHILD);
        }
        if options & WNOHANG != 0 {
            return Ok((0, 0));
        }
        if self.waited.replace(true) {
            return Err(ECHILD);
        }
        Ok((42, self.code << 8))
    }

    fn close(&self, _handle: u32) -> Result<(), Errno> {
        self.close_calls.set(self.close_calls.get() + 1);
        Ok(())
    }
}

struct FakeInfo {
    pid: i32,
    fail: bool,
}

impl HandleInfo for FakeInfo {
    fn pid_for_handle(&self, _handle: u32) -> Result<i32, Errno> {
        if self.fail { Err(EBADF) } else { Ok(self.pid) }
    }
}

fn request(path: &[u8], cwd: &[u8], argv: &[&[u8]]) -> SpawnRequest {
    SpawnRequest {
        spawn_flags: 0,
        pgroup: 0,
        sigdefault: [0; 128],
        sigmask: [0; 128],
        ignored: [0; 128],
        sched_policy: 0,
        sched_prio: 0,
        umask: 0,
        tcpgrp_fd: -1,
        path: path.to_vec(),
        cwd: cwd.to_vec(),
        argv: argv.iter().map(|value| value.to_vec()).collect(),
        envp: Vec::new(),
        fdmap: Vec::new(),
    }
}

#[test]
fn spawn_resolves_a_relative_path_and_records_the_pid() {
    let loader = FakeLoader::new(7, 0);
    let spawner = NativeSpawner::new(
        loader,
        FakeInfo {
            pid: 42,
            fail: false,
        },
    );
    let pid = spawner
        .spawn(&request(
            b"ls",
            b"/bin",
            &[b"/bin/ls".as_slice(), b"-l".as_slice()],
        ))
        .expect("spawn");
    assert_eq!(pid, 42);
    assert_eq!(*spawner.loader.read_path.borrow(), "/bin/ls");
    assert_eq!(*spawner.loader.load_path.borrow(), "/bin/ls");
    assert_eq!(*spawner.loader.cwd.borrow(), b"/bin");
    assert_eq!(
        *spawner.loader.argv.borrow(),
        vec!["/bin/ls".to_string(), "-l".to_string()]
    );
}

#[test]
fn invalid_cwd_is_rejected_before_the_loader() {
    let spawner = NativeSpawner::new(
        FakeLoader::new(7, 0),
        FakeInfo {
            pid: 1,
            fail: false,
        },
    );
    assert_eq!(
        spawner.spawn(&request(b"ls", b"bin", &[])).unwrap_err(),
        EINVAL
    );
    assert_eq!(
        spawner.spawn(&request(b"ls", b"", &[])).unwrap_err(),
        EINVAL
    );
    let long = vec![b'/'; 4096];
    assert_eq!(
        spawner.spawn(&request(b"ls", &long, &[])).unwrap_err(),
        ENAMETOOLONG
    );
}

#[test]
fn spawn_keeps_an_absolute_path_and_a_scheme_path() {
    let loader = FakeLoader::new(7, 0);
    let spawner = NativeSpawner::new(
        loader,
        FakeInfo {
            pid: 1,
            fail: false,
        },
    );
    spawner
        .spawn(&request(b"/bin/ls", b"/tmp", &[]))
        .expect("absolute spawn");
    assert_eq!(*spawner.loader.read_path.borrow(), "/bin/ls");
    spawner
        .spawn(&request(b"file:/root/bin/ls", b"/tmp", &[]))
        .expect("scheme spawn");
    assert_eq!(*spawner.loader.read_path.borrow(), "file:/root/bin/ls");
}

#[test]
fn spawn_closes_the_handle_when_pid_conversion_fails() {
    let loader = FakeLoader::new(7, 0);
    let spawner = NativeSpawner::new(loader, FakeInfo { pid: 0, fail: true });
    assert_eq!(spawner.spawn(&request(b"/bin/ls", b"/", &[])), Err(EBADF));
    assert_eq!(spawner.loader.close_calls.get(), 1);
}

#[test]
fn non_utf8_arguments_are_rejected_before_the_image_is_read() {
    let loader = FakeLoader::new(7, 0);
    let spawner = NativeSpawner::new(
        loader,
        FakeInfo {
            pid: 1,
            fail: false,
        },
    );
    assert_eq!(
        spawner.spawn(&request(b"/bin/ls", b"/", &[b"\xff".as_slice()])),
        Err(EINVAL)
    );
    assert_eq!(*spawner.loader.read_path.borrow(), "");
}

#[test]
fn wait_encodes_the_exit_status_and_closes_the_handle() {
    let loader = FakeLoader::new(7, 42);
    let spawner = NativeSpawner::new(
        loader,
        FakeInfo {
            pid: 42,
            fail: false,
        },
    );
    spawner
        .spawn(&request(b"/bin/ls", b"/", &[]))
        .expect("spawn");
    let result = spawner.wait(P_PID, 42, 0).expect("wait");
    assert_eq!(result.pid, 42);
    assert_eq!(result.status, 42 << 8);
    assert_eq!(spawner.loader.close_calls.get(), 1);
    assert_eq!(spawner.wait(P_PID, 42, 0), Err(ECHILD));
}

#[test]
fn wait_releases_the_table_slot() {
    let loader = FakeLoader::new(7, 0);
    let spawner = NativeSpawner::new(
        loader,
        FakeInfo {
            pid: 42,
            fail: false,
        },
    );
    for _ in 0..MAX_CHILDREN * 2 {
        spawner
            .spawn(&request(b"/bin/ls", b"/", &[]))
            .expect("spawn");
        spawner.wait(P_PID, 42, 0).expect("wait");
    }
}

#[test]
fn wait_for_an_unknown_child_is_echild() {
    let loader = FakeLoader::new(7, 0);
    let spawner = NativeSpawner::new(
        loader,
        FakeInfo {
            pid: 1,
            fail: false,
        },
    );
    assert_eq!(spawner.wait(P_PID, 99, 0), Err(ECHILD));
}

#[test]
fn wnohang_does_not_reap_and_wait_any_uses_the_kernel_seam() {
    let loader = FakeLoader::new(7, 0);
    let spawner = NativeSpawner::new(
        loader,
        FakeInfo {
            pid: 42,
            fail: false,
        },
    );
    spawner
        .spawn(&request(b"/bin/ls", b"/", &[]))
        .expect("spawn");
    let miss = spawner.wait(P_PID, 42, WNOHANG).expect("wnohang");
    assert_eq!(miss.pid, 0);
    assert_eq!(spawner.loader.close_calls.get(), 0);
    let ready = spawner.wait(P_ALL, 0, 0).expect("wait any");
    assert_eq!(ready.pid, 42);
    assert_eq!(spawner.loader.close_calls.get(), 1);
}

#[test]
fn wait_translates_public_continued_to_the_kernel_word() {
    let loader = FakeLoader::new(7, 0);
    let spawner = NativeSpawner::new(
        loader,
        FakeInfo {
            pid: 42,
            fail: false,
        },
    );
    spawner
        .spawn(&request(b"/bin/ls", b"/", &[]))
        .expect("spawn");
    spawner.wait(P_PID, 42, WCONTINUED).expect("wait");
    assert_eq!(spawner.loader.last_options.get(), KERNEL_WCONTINUED);
}

#[test]
fn command_name_uses_argv_zero() {
    assert_eq!(command_name(&["ls", "-l"], "/bin/ls"), "ls");
}

#[test]
fn command_name_falls_back_to_the_image_path() {
    assert_eq!(command_name(&[], "/bin/ls"), "/bin/ls");
    assert_eq!(command_name(&[""], "/bin/ls"), "/bin/ls");
}

#[test]
fn command_name_is_bounded_at_a_utf8_boundary() {
    let long = "é".repeat(MAX_COMMAND);
    let name = command_name(&[long.as_str()], "/bin/ls");
    assert!(name.len() <= MAX_COMMAND);
    assert!(name.is_char_boundary(name.len()));
}
