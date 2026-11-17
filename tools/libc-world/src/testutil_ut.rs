// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn temp_dir_is_created_and_unique() {
    let first = TempDir::new("a");
    let second = TempDir::new("a");
    assert!(first.path().is_dir());
    assert!(second.path().is_dir());
    assert_ne!(first.path(), second.path());
}

#[test]
fn temp_dir_is_removed_on_drop() {
    let path = {
        let temp = TempDir::new("drop");
        let path = temp.path().to_path_buf();
        fs::create_dir_all(path.join("nested")).expect("nested");
        path
    };
    assert!(!path.exists());
}

#[test]
fn write_creates_parents() {
    let temp = TempDir::new("write");
    let path = temp.join("a/b/c.txt");
    write(&path, "hello\n");
    assert_eq!(fs::read_to_string(path).expect("read"), "hello\n");
}

#[test]
fn tool_available_finds_sh() {
    assert!(tool_available("sh"));
    assert!(!tool_available("definitely-not-a-tool-xyz"));
}

#[test]
fn require_passes_for_known_tools() {
    assert!(require(&["sh"]));
}

#[test]
fn has_line_matches_only_prefixes() {
    let text = "alpha\nbeta\n";
    assert!(has_line(text, "alpha"));
    assert!(!has_line(text, "alphax"));
    assert!(!has_line(text, "gamma"));
}

#[test]
fn make_tar_builds_a_readable_archive() {
    let temp = TempDir::new("tar");
    let tree = temp.join("pkg-1.0");
    fs::create_dir_all(&tree).expect("tree");
    write(&tree.join("a.txt"), "hi\n");
    let tarball = temp.join("pkg.tar.gz");
    if !make_tar(temp.path(), "pkg-1.0", &tarball) {
        return;
    }
    assert!(tarball.is_file());
    let listing = Command::new("tar")
        .arg("-tf")
        .arg(&tarball)
        .output()
        .expect("list");
    assert!(String::from_utf8_lossy(&listing.stdout).contains("pkg-1.0/a.txt"));
}
