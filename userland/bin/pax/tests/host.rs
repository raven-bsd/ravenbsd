// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host filesystem tests for the pax model.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use pax::{Entry, encode_archive, parse_archive};

#[test]
fn a_tree_with_a_long_path_round_trips() {
    let root = std::env::temp_dir().join(format!("raven-pax-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let long = format!("{}/payload.txt", "nested".repeat(40));
    let data = b"pax payload\n";
    let path = root.join(&long);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, data).unwrap();
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
    let entries = vec![Entry {
        name: long,
        mode,
        uid: 0,
        gid: 0,
        mtime: 0,
        kind: b'0',
        linkname: String::new(),
        data: data.to_vec(),
    }];
    let archive = encode_archive(&entries).unwrap();
    assert_eq!(parse_archive(&archive).unwrap(), entries);
    let _ = fs::remove_dir_all(root);
}
