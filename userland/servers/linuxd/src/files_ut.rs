// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! File path and deferred operation tests.

use raven_abi::types::Handle;

use super::*;
use crate::table::{FileKind, LinuxProcess};

fn process() -> LinuxProcess {
    LinuxProcess::new(7, Handle(9), 0x4000, 0x5000, 0x8000, 0x9000)
}

#[test]
fn schemes_are_detected_before_the_first_slash() {
    assert!(has_scheme(b"file:/tmp/a"));
    assert!(has_scheme(b"ufs:/root/a"));
    assert!(!has_scheme(b"/tmp/file:a"));
    assert!(!has_scheme(b"file/tmp"));
    assert!(!has_scheme(b":empty"));
}

#[test]
fn synthetic_paths_are_the_proc_and_sys_trees_only() {
    assert!(is_synthetic(b"/proc"));
    assert!(is_synthetic(b"/proc/self/status"));
    assert!(is_synthetic(b"/sys/devices/system/cpu/online"));
    assert!(!is_synthetic(b"/tmp/b23"));
    assert!(!is_synthetic(b"/procfs"));
    assert!(!is_synthetic(b"file:/proc/self/status"));
}

#[test]
fn a_linux_path_becomes_a_file_scheme_path() {
    assert_eq!(scheme_path(b"/tmp/a"), b"file:/tmp/a");
    assert_eq!(scheme_path(b"file:/tmp/a"), b"file:/tmp/a");
    assert_eq!(scheme_path(b"ufs:/root/a"), b"ufs:/root/a");
}

#[test]
fn absolute_and_relative_paths_resolve_against_the_cwd() {
    let process = process();
    assert_eq!(
        resolve_path(&process, AT_FDCWD, b"/tmp/a").unwrap(),
        b"/tmp/a"
    );
    assert_eq!(
        resolve_path(&process, AT_FDCWD, b"tmp/a").unwrap(),
        b"/tmp/a"
    );
    assert_eq!(
        resolve_path(&process, AT_FDCWD, b"./tmp/../data").unwrap(),
        b"/data"
    );
    assert_eq!(
        resolve_path(&process, AT_FDCWD, b"file:/tmp/a").unwrap(),
        b"file:/tmp/a"
    );
    assert_eq!(
        resolve_path(&process, AT_FDCWD, b""),
        Err(raven_abi::errno::Errno::ENOENT)
    );
}

#[test]
fn a_directory_descriptor_is_the_relative_base() {
    let mut process = process();
    let fd = process.open_file(b"/tmp".to_vec(), FileKind::Directory(Vec::new()));
    assert_eq!(
        resolve_path(&process, fd, b"file.txt").unwrap(),
        b"/tmp/file.txt"
    );
    let file = process.open_file(b"/hello".to_vec(), FileKind::Regular(Vec::new()));
    assert_eq!(
        resolve_path(&process, file, b"child"),
        Err(raven_abi::errno::Errno::ENOTDIR)
    );
    assert_eq!(
        resolve_path(&process, 99, b"child"),
        Err(raven_abi::errno::Errno::EBADF)
    );
}

#[test]
fn reads_are_bounded_by_the_server_buffer() {
    assert_eq!(bounded_len(8), 8);
    assert_eq!(bounded_len(USER_BUFFER_MAX + 1), USER_BUFFER_MAX);
}
