// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the staged-root metadata tree.

use super::*;

fn tree() -> RootTree {
    RootTree::new(&[
        RootEntry {
            path: b"/compat/linux/bin/busybox".to_vec(),
            size: 804_616,
            mode: 0o755,
        },
        RootEntry {
            path: b"/compat/linux/etc/alpine-release".to_vec(),
            size: 6,
            mode: 0o644,
        },
    ])
}

#[test]
fn implicit_directories_and_files_are_resolved() {
    let tree = tree();
    let root = tree.lookup(b"/").expect("root");
    assert!(root.is_dir());
    assert_eq!(root.mode & 0o777, 0o755);
    let bin = tree.lookup(b"/compat/linux/bin").expect("bin");
    assert!(bin.is_dir());
    let busybox = tree.lookup(b"/compat/linux/bin/busybox").expect("busybox");
    assert!(!busybox.is_dir());
    assert_eq!(busybox.size, 804_616);
    assert_eq!(busybox.mode & 0o777, 0o755);
    assert_eq!(busybox.mode & 0o170000, super::S_IFREG);
    assert!(tree.lookup(b"/compat/linux/none").is_none());
    assert!(tree.lookup(b"compat").is_none());
}

#[test]
fn directories_list_their_direct_children_sorted() {
    let tree = tree();
    let entries = tree.entries(b"/compat/linux").expect("linux root");
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_slice())
            .collect::<Vec<_>>(),
        [b"bin".as_slice(), b"etc".as_slice()]
    );
    assert!(entries.iter().all(|entry| entry.kind == DT_DIR));
    let root = tree.entries(b"/").expect("root");
    assert_eq!(
        root.iter()
            .map(|entry| entry.name.as_slice())
            .collect::<Vec<_>>(),
        [b"compat".as_slice(), b"proc".as_slice(), b"sys".as_slice()]
    );
}

#[test]
fn trailing_slashes_normalize() {
    let tree = tree();
    assert_eq!(
        tree.lookup(b"/compat/linux/").map(|node| node.ino),
        tree.lookup(b"/compat/linux").map(|node| node.ino)
    );
    assert!(tree.entries(b"/compat/linux/etc").is_some());
    assert!(tree.entries(b"/compat/linux/etc/alpine-release").is_none());
}

#[test]
fn inode_numbers_are_unique_and_nonzero() {
    let tree = tree();
    let mut inodes = Vec::new();
    for path in [
        b"/".as_slice(),
        b"/compat",
        b"/compat/linux",
        b"/compat/linux/bin",
        b"/compat/linux/bin/busybox",
        b"/compat/linux/etc",
        b"/compat/linux/etc/alpine-release",
    ] {
        let ino = tree.lookup(path).expect("node").ino;
        assert!(ino != 0);
        assert!(!inodes.contains(&ino), "duplicate inode for {path:?}");
        inodes.push(ino);
    }
}

#[test]
fn an_empty_tree_still_has_the_fixed_root_directories() {
    let tree = RootTree::new(&[]);
    assert!(tree.is_empty());
    assert!(tree.lookup(b"/").is_some());
    assert!(tree.lookup(b"/proc").is_some());
    assert!(tree.lookup(b"/sys").is_some());
}
