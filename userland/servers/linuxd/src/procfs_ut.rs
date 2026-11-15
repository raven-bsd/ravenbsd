// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the synthetic procfs/sysfs content.

use super::*;
use crate::table::{DirEntry, LinuxProcess, Region};
use raven_abi::types::Handle;

fn process() -> LinuxProcess {
    let mut process = LinuxProcess::new(1, Handle(1), 0x40_5000, 0x41_0000, 0x50_0000, 0x60_0000);
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

#[test]
fn dot_segments_are_normalized() {
    assert!(matches!(
        lookup(&process(), b"/proc/./self/../self/status"),
        Ok(Resource::File(_))
    ));
    assert!(matches!(
        lookup(&process(), b"/proc//self//status"),
        Ok(Resource::File(_))
    ));
    assert!(matches!(
        lookup(&process(), b"/../sys/."),
        Ok(Resource::Directory(_))
    ));
}

#[test]
fn file_metadata_has_the_synthetic_shape() {
    let process = process();
    let meta = metadata(&process, b"/proc/self/status", false).expect("stat");
    assert_eq!(meta.mode, 0o100444);
    assert_eq!(meta.nlink, 1);
    assert_eq!(meta.size, file(&process, "/proc/self/status").len() as u64);
    assert_eq!(meta.blocks, meta.size.div_ceil(512));
    assert!(meta.inode != 0);
}

#[test]
fn directory_metadata_has_two_links_and_a_block() {
    let meta = metadata(&process(), b"/proc/self", false).expect("stat");
    assert!(meta.is_dir());
    assert_eq!(meta.mode, 0o040555);
    assert_eq!(meta.nlink, 2);
    assert_eq!(meta.size, BLKSIZE);
    assert_eq!(meta.blocks, 8);
}

#[test]
fn lstat_reports_the_link_but_stat_follows_it() {
    let process = process();
    let link = metadata(&process, b"/proc/self/exe", false).expect("lstat");
    assert_eq!(link.mode, 0o120777);
    assert_eq!(link.size, 6);
    // The synthetic tree does not serve the target (`/hello`), so following it is ENOENT.
    assert_eq!(
        metadata(&process, b"/proc/self/exe", true),
        Err(Errno::ENOENT)
    );
}

#[test]
fn standard_and_open_descriptors_have_metadata() {
    let mut process = process();
    let stdin = metadata_fd(&process, 0).expect("fstat 0");
    assert_eq!(stdin.mode, 0o020666);
    assert_eq!(stdin.nlink, 1);
    assert_eq!(stdin.size, 0);
    let stdout = metadata_fd(&process, 1).expect("fstat 1");
    assert_eq!(stdout.mode, 0o020666);
    assert_eq!(metadata_fd(&process, 2).expect("fstat 2").mode, 0o020666);
    assert_ne!(stdin.inode, stdout.inode);

    let fd = process.open_file(
        b"/proc/self/status".to_vec(),
        crate::table::FileKind::Regular(vec![1, 2, 3]),
    );
    let opened = metadata_fd(&process, fd).expect("fstat open");
    assert_eq!(opened.mode, 0o100444);
    assert_eq!(opened.size, 3);
    assert_eq!(opened.blocks, 1);
    assert_eq!(metadata_fd(&process, 99), Err(Errno::EBADF));
}

#[test]
fn access_uses_the_synthetic_mode_bits() {
    let process = process();
    let file = metadata(&process, b"/proc/self/status", false).expect("stat file");
    assert!(file.allows(0));
    assert!(file.allows(0o4));
    assert!(!file.allows(0o2));
    assert!(!file.allows(0o1));
    assert!(!file.allows(0o10));

    let dir = metadata(&process, b"/proc/self", false).expect("stat dir");
    assert!(dir.allows(0o4));
    assert!(dir.allows(0o1));
    assert!(!dir.allows(0o2));

    let link = metadata(&process, b"/proc/self/exe", false).expect("stat link");
    assert!(link.allows(0o2));
}
