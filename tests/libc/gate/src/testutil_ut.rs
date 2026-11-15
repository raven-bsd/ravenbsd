// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Temporary-directory helper tests.

use super::*;

#[test]
fn writes_and_reads_a_file() {
    let temp = TempDir::new("write");
    temp.write("a/b.txt", "hello\n");
    assert_eq!(
        fs::read_to_string(temp.path().join("a/b.txt")).unwrap(),
        "hello\n"
    );
}

#[test]
fn mkdir_creates_a_directory() {
    let temp = TempDir::new("mkdir");
    temp.mkdir("docs/man3");
    assert!(temp.path().join("docs/man3").is_dir());
}

#[test]
fn paths_are_unique() {
    let first = TempDir::new("unique");
    let second = TempDir::new("unique");
    assert_ne!(first.path(), second.path());
}

#[test]
fn drop_removes_the_tree() {
    let path = {
        let temp = TempDir::new("drop");
        temp.write("x", "y");
        temp.path().to_path_buf()
    };
    assert!(!path.exists());
}
