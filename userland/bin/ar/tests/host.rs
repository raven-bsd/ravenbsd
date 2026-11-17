// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host filesystem tests for the ar archive model.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use ar::{Member, parse_archive, write_archive};

fn temp(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("raven-ar-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn archives_real_files_and_preserves_modes() {
    let root = temp("files");
    let first = root.join("first.o");
    let second = root.join("second.o");
    fs::write(&first, b"first bytes").unwrap();
    fs::write(&second, b"second bytes").unwrap();
    fs::set_permissions(&first, fs::Permissions::from_mode(0o640)).unwrap();
    let mode = fs::metadata(&first).unwrap().permissions().mode() & 0o7777;
    let members = vec![
        Member {
            name: "first.o".into(),
            mtime: 0,
            uid: 0,
            gid: 0,
            mode,
            data: fs::read(&first).unwrap(),
        },
        Member {
            name: "second.o".into(),
            mtime: 0,
            uid: 0,
            gid: 0,
            mode: 0o644,
            data: fs::read(&second).unwrap(),
        },
    ];
    let archive = write_archive(&members, false);
    let parsed = parse_archive(&archive).unwrap();
    assert_eq!(parsed, members);
    assert_eq!(parsed[0].mode & 0o777, 0o640);
    let _ = fs::remove_dir_all(root);
}
