// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host filesystem tests for the ustar archive model.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use tar::{Entry, encode_archive, parse_archive};

fn temp(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("raven-tar-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn a_real_tree_round_trips_through_the_archive() {
    let root = temp("tree");
    let source = root.join("src");
    let nested = source.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(source.join("a.txt"), b"alpha\n").unwrap();
    fs::write(nested.join("b.txt"), b"beta\n").unwrap();
    fs::set_permissions(source.join("a.txt"), fs::Permissions::from_mode(0o640)).unwrap();

    let entries = vec![
        Entry {
            name: "src/".into(),
            mode: 0o755,
            uid: 0,
            gid: 0,
            mtime: 0,
            kind: b'5',
            linkname: String::new(),
            data: Vec::new(),
        },
        Entry {
            name: "src/a.txt".into(),
            mode: fs::metadata(source.join("a.txt"))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            uid: 0,
            gid: 0,
            mtime: 0,
            kind: b'0',
            linkname: String::new(),
            data: fs::read(source.join("a.txt")).unwrap(),
        },
        Entry {
            name: "src/nested/".into(),
            mode: 0o755,
            uid: 0,
            gid: 0,
            mtime: 0,
            kind: b'5',
            linkname: String::new(),
            data: Vec::new(),
        },
        Entry {
            name: "src/nested/b.txt".into(),
            mode: 0o644,
            uid: 0,
            gid: 0,
            mtime: 0,
            kind: b'0',
            linkname: String::new(),
            data: fs::read(nested.join("b.txt")).unwrap(),
        },
    ];
    let archive = encode_archive(&entries).unwrap();
    let parsed = parse_archive(&archive).unwrap();
    assert_eq!(parsed, entries);
    assert_eq!(parsed[1].mode & 0o777, 0o640);
    let _ = fs::remove_dir_all(root);
}
