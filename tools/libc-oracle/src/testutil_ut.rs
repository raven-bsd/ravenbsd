// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn tempdir_writes_nested_files_and_removes_them() {
    let root = TempDir::new("self");
    let path = root.write("a/b/c.txt", "hello");
    assert_eq!(fs::read_to_string(&path).unwrap(), "hello");
    let path = root.path().to_path_buf();
    drop(root);
    assert!(!path.exists());
}

#[test]
fn tempdirs_are_unique() {
    let first = TempDir::new("unique");
    let second = TempDir::new("unique");
    assert_ne!(first.path(), second.path());
}
