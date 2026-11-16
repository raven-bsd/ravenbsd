// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the synthetic procfs/sysfs content.

use super::*;
use crate::table::{DirEntry, LinuxProcess, Region};
use raven_abi::types::Handle;

fn process() -> LinuxProcess {
    let mut process = LinuxProcess::new(
        1,
        1,
        Handle(1),
        Handle(2),
        0x40_5000,
        0x41_0000,
        0x50_0000,
        0x60_0000,
    );
    process.regions.push(Region {
        start: 0x40_0000,
        len: 0x2000,
        backing: Handle(2),
        backing_offset: 0,
    });
    process.set_identity(b"/hello", vec![b"/hello".to_vec(), b"arg".to_vec()]);
    process
}

fn file(process: &LinuxProcess, path: &str) -> Vec<u8> {
    match lookup(process, path.as_bytes()) {
        Ok(Resource::File(bytes)) => bytes,
        other => panic!("{path}: {other:?}"),
    }
}

fn entries(process: &LinuxProcess, path: &str) -> Vec<DirEntry> {
    match lookup(process, path.as_bytes()) {
        Ok(Resource::Directory(entries)) => entries,
        other => panic!("{path}: {other:?}"),
    }
}

#[test]
fn status_has_the_expected_keys_and_pid() {
    let text = String::from_utf8(file(&process(), "/proc/self/status")).unwrap();
    assert!(text.contains("Name:\thello\n"), "{text}");
    assert!(text.contains("Pid:\t1\n"), "{text}");
    assert!(text.contains("Uid:\t0\t0\t0\t0\n"), "{text}");
    assert!(text.contains("VmSize:\t"), "{text}");
}

#[test]
fn stat_has_52_fields_and_a_comm() {
    let text = String::from_utf8(file(&process(), "/proc/self/stat")).unwrap();
    let fields: Vec<&str> = text.split_whitespace().collect();
    assert_eq!(fields.len(), 52);
    assert_eq!(fields[0], "1");
    assert_eq!(fields[1], "(hello)");
    assert_eq!(fields[2], "R");
}

#[test]
fn maps_lists_each_region() {
    let text = String::from_utf8(file(&process(), "/proc/self/maps")).unwrap();
    assert!(text.starts_with("00400000-00402000 rw-p "), "{text}");
}

#[test]
fn cmdline_is_nul_separated() {
    let bytes = file(&process(), "/proc/self/cmdline");
    assert_eq!(bytes, b"/hello\0arg\0");
}

#[test]
fn cpuinfo_and_meminfo_have_their_first_lines() {
    let cpu = String::from_utf8(file(&process(), "/proc/cpuinfo")).unwrap();
    assert!(cpu.starts_with("processor\t: 0\n"));
    assert!(cpu.contains("model name\t: RavenBSD virtual CPU"));
    let mem = String::from_utf8(file(&process(), "/proc/meminfo")).unwrap();
    assert!(mem.starts_with("MemTotal:         262144 kB\n"));
}

#[test]
fn online_is_a_cpu_list() {
    assert_eq!(file(&process(), "/sys/devices/system/cpu/online"), b"0\n");
}

#[test]
fn exe_is_a_symlink_to_the_loaded_path() {
    assert_eq!(
        lookup(&process(), b"/proc/self/exe"),
        Ok(Resource::Symlink(b"/hello".to_vec()))
    );
}

#[test]
fn std_descriptors_appear_in_the_fd_directory_and_as_targets() {
    let entries = entries(&process(), "/proc/self/fd");
    let names: Vec<&[u8]> = entries.iter().map(|entry| entry.name.as_slice()).collect();
    for expected in [b"0".as_slice(), b"1", b"2"] {
        assert!(names.contains(&expected), "{names:?}");
    }
    assert_eq!(
        lookup(&process(), b"/proc/self/fd/0"),
        Ok(Resource::Symlink(b"/dev/null".to_vec()))
    );
    assert_eq!(
        lookup(&process(), b"/proc/self/fd/1"),
        Ok(Resource::Symlink(b"tty:console".to_vec()))
    );
}

#[test]
fn an_open_file_appears_with_its_fd_and_path() {
    let mut process = process();
    let fd = process.open_file(
        b"/proc/self/status".to_vec(),
        crate::table::FileKind::Regular(vec![1]),
    );
    assert_eq!(fd, 3);
    let entries = entries(&process, "/proc/self/fd");
    assert!(entries.iter().any(|entry| entry.name == b"3"));
    assert_eq!(
        lookup(&process, b"/proc/self/fd/3"),
        Ok(Resource::Symlink(b"/proc/self/status".to_vec()))
    );
}

#[test]
fn directory_paths_and_trailing_slashes_resolve() {
    assert!(matches!(
        lookup(&process(), b"/proc/"),
        Ok(Resource::Directory(_))
    ));
    assert!(matches!(
        lookup(&process(), b"/sys/devices/system/cpu/"),
        Ok(Resource::Directory(_))
    ));
    assert!(matches!(
        lookup(&process(), b"/sys"),
        Ok(Resource::Directory(_))
    ));
}

#[test]
fn unknown_and_relative_paths_are_errors() {
    assert_eq!(lookup(&process(), b"/proc/nope"), Err(Errno::ENOENT));
    assert_eq!(lookup(&process(), b"/proc/self/fd/x"), Err(Errno::ENOENT));
    assert_eq!(lookup(&process(), b"proc/self/status"), Err(Errno::EINVAL));
    assert_eq!(lookup(&process(), b"/proc/self/fd/999"), Err(Errno::ENOENT));
}
