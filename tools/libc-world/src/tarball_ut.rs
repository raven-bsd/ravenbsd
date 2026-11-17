// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::path::Path;
use std::process::Command;

use super::*;
use crate::testutil::{TempDir, make_tar, require, write};

fn make_tree(temp: &TempDir) -> std::path::PathBuf {
    let root = temp.join("build");
    let tree = root.join("pkg-1.0");
    fs::create_dir_all(tree.join("sub")).expect("tree");
    write(&tree.join("a.txt"), "hello\n");
    write(&tree.join("sub/b.txt"), "world\n");
    root
}

fn tar_command(args: &[&str], cwd: &Path) -> bool {
    if !require(&["tar"]) {
        return false;
    }
    let status = Command::new("tar")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("tar");
    status.success()
}

#[test]
fn a_good_archive_lists_and_passes() {
    let temp = TempDir::new("tar-good");
    let root = make_tree(&temp);
    let tarball = temp.join("good.tar.gz");
    if !make_tar(&root, "pkg-1.0", &tarball) {
        return;
    }
    let entries = list("tar", &tarball).expect("list");
    assert!(entries.iter().any(|entry| entry.name == "pkg-1.0/a.txt"));
    check(&entries).expect("safe");
}

#[test]
fn an_internal_symlink_is_safe() {
    let temp = TempDir::new("tar-link");
    let root = make_tree(&temp);
    std::os::unix::fs::symlink("a.txt", root.join("pkg-1.0/sub/link")).expect("symlink");
    let tarball = temp.join("link.tar.gz");
    if !make_tar(&root, "pkg-1.0", &tarball) {
        return;
    }
    let entries = list("tar", &tarball).expect("list");
    check(&entries).expect("safe");
}

#[test]
fn a_safe_hardlink_is_accepted() {
    let temp = TempDir::new("tar-hard");
    let root = make_tree(&temp);
    fs::hard_link(root.join("pkg-1.0/a.txt"), root.join("pkg-1.0/hard.txt")).expect("hard");
    let tarball = temp.join("hard.tar.gz");
    if !make_tar(&root, "pkg-1.0", &tarball) {
        return;
    }
    let entries = list("tar", &tarball).expect("list");
    check(&entries).expect("safe");
}

#[test]
fn an_absolute_entry_is_refused() {
    let temp = TempDir::new("tar-abs");
    let tarball = temp.join("abs.tar.gz");
    if !tar_command(
        &[
            "czf",
            tarball.to_str().expect("path"),
            "-P",
            "/etc/hostname",
        ],
        temp.path(),
    ) {
        return;
    }
    let entries = list("tar", &tarball).expect("list");
    let problem = check(&entries).expect_err("unsafe");
    assert!(matches!(problem, TarProblem::Unsafe(_)), "{problem:?}");
    assert!(problem.reason().contains("absolute"));
}

#[test]
fn a_dotdot_entry_is_refused() {
    let temp = TempDir::new("tar-dotdot");
    let root = make_tree(&temp);
    let tarball = temp.join("dotdot.tar.gz");
    if !tar_command(
        &[
            "czf",
            tarball.to_str().expect("path"),
            "--transform=s|^pkg-1.0|../pkg-1.0|",
            "pkg-1.0",
        ],
        &root,
    ) {
        return;
    }
    let entries = list("tar", &tarball).expect("list");
    let problem = check(&entries).expect_err("unsafe");
    assert!(problem.reason().contains("escapes"));
}

#[test]
fn a_device_entry_is_refused() {
    let temp = TempDir::new("tar-dev");
    let tarball = temp.join("dev.tar.gz");
    if !tar_command(
        &["czf", tarball.to_str().expect("path"), "-C", "/dev", "null"],
        temp.path(),
    ) {
        return;
    }
    let entries = list("tar", &tarball).expect("list");
    let problem = check(&entries).expect_err("unsafe");
    assert!(problem.reason().contains("special entry type"));
}

#[test]
fn a_fifo_entry_is_refused() {
    let temp = TempDir::new("tar-fifo");
    let root = make_tree(&temp);
    let status = Command::new("mkfifo")
        .arg(root.join("pkg-1.0/fifo"))
        .status()
        .expect("mkfifo");
    if !status.success() {
        eprintln!("SKIP: mkfifo failed");
        return;
    }
    let tarball = temp.join("fifo.tar.gz");
    if !make_tar(&root, "pkg-1.0", &tarball) {
        return;
    }
    let entries = list("tar", &tarball).expect("list");
    let problem = check(&entries).expect_err("unsafe");
    assert!(problem.reason().contains("special entry type"));
}

#[test]
fn an_escaping_symlink_is_refused() {
    let temp = TempDir::new("tar-escape");
    let root = make_tree(&temp);
    std::os::unix::fs::symlink("/etc/passwd", root.join("pkg-1.0/sub/evil")).expect("symlink");
    let tarball = temp.join("escape.tar.gz");
    if !make_tar(&root, "pkg-1.0", &tarball) {
        return;
    }
    let entries = list("tar", &tarball).expect("list");
    let problem = check(&entries).expect_err("unsafe");
    assert!(problem.reason().contains("outside the root"));
}

#[test]
fn a_relative_symlink_that_climbs_out_is_refused() {
    let entries = vec![Entry {
        name: "pkg/link".to_string(),
        kind: EntryKind::Symlink,
        target: Some("../../etc/passwd".to_string()),
    }];
    let problem = check(&entries).expect_err("unsafe");
    assert!(problem.reason().contains("escapes the root"));
}

#[test]
fn two_top_level_entries_are_a_layout_error() {
    let temp = TempDir::new("tar-two");
    let root = temp.join("build");
    fs::create_dir_all(root.join("one")).expect("one");
    fs::create_dir_all(root.join("two")).expect("two");
    write(&root.join("one/a"), "a\n");
    write(&root.join("two/b"), "b\n");
    let tarball = temp.join("two.tar.gz");
    if !tar_command(
        &["czf", tarball.to_str().expect("path"), "one", "two"],
        &root,
    ) {
        return;
    }
    let entries = list("tar", &tarball).expect("list");
    let problem = check(&entries).expect_err("layout");
    assert_eq!(problem.status(), "tarball-layout");
}

#[test]
fn a_directory_only_archive_is_a_layout_error() {
    let entries = vec![Entry {
        name: "pkg".to_string(),
        kind: EntryKind::Directory,
        target: None,
    }];
    let problem = check(&entries).expect_err("layout");
    assert!(problem.reason().contains("no files"));
}

#[test]
fn an_empty_archive_is_a_layout_error() {
    let problem = check(&[]).expect_err("empty");
    assert_eq!(problem.status(), "tarball-layout");
}

#[test]
fn normalize_handles_dot_and_dotdot() {
    assert_eq!(normalize("pkg/./a").as_deref(), Some("pkg/a"));
    assert_eq!(normalize("pkg/sub/../a").as_deref(), Some("pkg/a"));
    assert_eq!(normalize("../pkg/a"), None);
    assert_eq!(normalize("pkg/../../a"), None);
    assert_eq!(normalize("").as_deref(), Some(""));
}

#[test]
fn file_sha256_matches_the_known_digest() {
    let temp = TempDir::new("tar-sha");
    let path = temp.join("empty");
    write(&path, "");
    assert_eq!(
        file_sha256(&path).expect("sha"),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn extract_strips_one_component() {
    let temp = TempDir::new("tar-extract");
    let root = make_tree(&temp);
    let tarball = temp.join("good.tar.gz");
    if !make_tar(&root, "pkg-1.0", &tarball) {
        return;
    }
    let dest = temp.join("dest");
    fs::create_dir_all(&dest).expect("dest");
    extract("tar", &tarball, &dest).expect("extract");
    assert_eq!(
        fs::read_to_string(dest.join("a.txt")).expect("read"),
        "hello\n"
    );
    assert_eq!(
        fs::read_to_string(dest.join("sub/b.txt")).expect("read"),
        "world\n"
    );
}

#[test]
fn a_missing_archive_is_an_error() {
    let temp = TempDir::new("tar-missing");
    assert!(list("tar", &temp.join("nope.tar.gz")).is_err());
}

#[test]
fn parsing_rejects_a_truncated_listing_line() {
    assert!(parse_verbose("-rw-r--r--").is_err());
}
