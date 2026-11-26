// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use raven_proto_fs::{
    DIRENT_HEADER_LEN, DT_DIR, DT_REG, O_APPEND, O_CREAT, O_EXCL, O_RDWR, O_TRUNC, S_IFDIR, S_IFREG,
};

#[test]
fn resolve_absolute_path() {
    assert_eq!(resolve_with_pwd("/tmp/a", "/home/me"), "/tmp/a");
    assert_eq!(resolve_with_pwd("/tmp/../a", "/home/me"), "/a");
}

#[test]
fn resolve_relative_path() {
    assert_eq!(resolve_with_pwd("a", "/tmp"), "/tmp/a");
    assert_eq!(resolve_with_pwd("a/../b", "/tmp"), "/tmp/b");
    assert_eq!(resolve_with_pwd("a", "/"), "/a");
}

#[test]
fn resolve_keeps_scheme_paths() {
    assert_eq!(resolve_with_pwd("file:/tmp", "/home"), "file:/tmp");
    assert_eq!(resolve_with_pwd("tty:console", "/home"), "tty:console");
    assert!(!has_scheme("/tmp/a:b"));
    assert!(has_scheme("echo:hello"));
}

#[test]
fn resolve_rejects_escape_fallback_keeps_input() {
    assert_eq!(resolve_with_pwd("../../x", "/tmp"), "/tmp/../../x");
}

#[test]
fn plain_path_is_qualified_with_the_file_scheme() {
    assert_eq!(scheme_path("/etc/passwd"), "file:/etc/passwd");
    assert_eq!(scheme_path("/"), "file:/");
}

#[test]
fn explicit_scheme_is_preserved() {
    assert_eq!(scheme_path("file:/etc/passwd"), "file:/etc/passwd");
    assert_eq!(scheme_path("ramfs:/etc"), "ramfs:/etc");
    assert_eq!(scheme_path("tty:console"), "tty:console");
}

#[test]
fn normalize_uses_the_k60_helper() {
    assert_eq!(normalize("/a/./b/../c").unwrap(), "/a/c");
    assert_eq!(normalize("relative"), Err(Errno::EINVAL));
}

#[test]
fn s11_flags_map_access_and_create() {
    assert_eq!(s11_flags(O_RDONLY).unwrap(), 1);
    assert_eq!(s11_flags(O_WRONLY).unwrap(), 2);
    assert_eq!(s11_flags(O_RDWR).unwrap(), 3);
    assert_eq!(s11_flags(O_RDWR | O_CREAT).unwrap(), 7);
    assert_eq!(
        s11_flags(O_RDWR | O_CREAT | O_EXCL | O_TRUNC | O_APPEND).unwrap(),
        0x3f
    );
    assert_eq!(s11_flags(3), Err(Errno::EINVAL));
    assert_eq!(s11_flags(O_RDONLY | 0o2000000), Err(Errno::EINVAL));
}

#[test]
fn decode_two_directory_entries() {
    let mut buffer = [0u8; 48];
    let first = ProtoDirEntry {
        header: raven_proto_fs::DirEntryHeader {
            ino: 1,
            kind: DT_REG,
            name_len: 1,
            pad: [0; 6],
        },
        name: b"a",
    };
    let second = ProtoDirEntry {
        header: raven_proto_fs::DirEntryHeader {
            ino: 2,
            kind: DT_DIR,
            name_len: 1,
            pad: [0; 6],
        },
        name: b"b",
    };
    let first_len = first.encode(&mut buffer).unwrap();
    second.encode(&mut buffer[first_len..]).unwrap();
    let entries = decode_dir_entries(&buffer).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "a");
    assert_eq!(entries[1].name, "b");
    assert_eq!(entries[1].kind, DT_DIR);
}

#[test]
fn decode_bad_directory_entry_is_eproto() {
    assert!(decode_dir_entries(&[0u8; 8]).is_err());
}

#[test]
fn dir_entry_kind_maps_from_mode() {
    assert_eq!(raven_proto_fs::dirent_kind(S_IFREG | 0o644), DT_REG);
    assert_eq!(raven_proto_fs::dirent_kind(S_IFDIR | 0o755), DT_DIR);
}

#[test]
fn header_size_matches_k60() {
    assert_eq!(DIRENT_HEADER_LEN, 16);
}
