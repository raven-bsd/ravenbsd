// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The FreeBSD `fts_misc_test.c` cases, by name, over the fake tree.
#![cfg(feature = "testutil")]

use raven_libc_fts::fake::FakeFs;
use raven_libc_fts::*;

fn walk(fs: &mut FakeFs, roots: &[&[u8]], options: u32) -> Vec<(i32, Vec<u8>, Vec<u8>, i64)> {
    let mut fts = Fts::open(fs, AT_FDCWD, roots, options, None).unwrap();
    let mut out = Vec::new();
    while let Some(node) = fts.read(fs).unwrap() {
        out.push((node.info, node.name.clone(), node.path.clone(), node.level));
    }
    fts.close(fs).unwrap();
    out
}

#[test]
fn fts_unrdir() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.mkdir("dir/unr", 0o100);
    fs.mkdir("dir/unx", 0o400);
    let nodes = walk(&mut fs, &[b"dir"], FTS_PHYSICAL);
    let sequence: Vec<(i32, &[u8])> = nodes
        .iter()
        .map(|(info, name, _, _)| (*info, name.as_slice()))
        .collect();
    assert_eq!(
        sequence,
        vec![
            (FTS_D, b"dir".as_slice()),
            (FTS_D, b"unr"),
            (FTS_DNR, b"unr"),
            (FTS_D, b"unx"),
            (FTS_DP, b"unx"),
            (FTS_DP, b"dir"),
        ]
    );
}

#[test]
fn fts_unrdir_nochdir() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.mkdir("dir/unr", 0o100);
    fs.mkdir("dir/unx", 0o400);
    let nodes = walk(&mut fs, &[b"dir"], FTS_PHYSICAL | FTS_NOCHDIR);
    assert!(
        nodes
            .iter()
            .any(|(info, _, path, _)| *info == FTS_DNR && path == b"dir/unr")
    );
}

#[test]
fn nochdir_app_can_chdir() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/a", 0);
    fs.file("dir/b", 0);
    fs.mkdir("elsewhere", 0o755);
    let mut fts = Fts::open(
        &mut fs,
        AT_FDCWD,
        &[b"dir"],
        FTS_PHYSICAL | FTS_NOCHDIR,
        None,
    )
    .unwrap();
    let first = fts.read(&mut fs).unwrap().unwrap().path.clone();
    // The application changes the fake cwd between two reads; the descriptors held by the
    // walk make that harmless (this is the FTS_NOCHDIR guarantee).
    fs.set_cwd("elsewhere");
    let mut entries = 1;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        assert!(node.path.starts_with(b"dir"));
        entries += 1;
    }
    fts.close(&mut fs).unwrap();
    assert_eq!(first, b"dir");
    assert_eq!(entries, 4);
}

#[test]
fn name_nul_terminated() {
    let mut fs = FakeFs::new();
    fs.mkdir("root", 0o755);
    fs.mkdir("root/sub", 0o755);
    fs.file("root/sub/file.c", 0);
    fs.symlink("root/sub/link", "file.c");
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"root"], FTS_PHYSICAL, None).unwrap();
    while let Some(node) = fts.read(&mut fs).unwrap() {
        assert!(!node.name.contains(&0));
        assert!(node.name.len() <= 255);
        assert!(node.path.ends_with(&node.name));
    }
    fts.close(&mut fs).unwrap();
}

#[test]
fn prepost_order_and_levels() {
    let mut fs = FakeFs::new();
    fs.mkdir("top", 0o755);
    fs.mkdir("top/mid", 0o755);
    fs.mkdir("top/mid/bot", 0o755);
    fs.file("top/mid/bot/leaf", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"top"], FTS_PHYSICAL, None).unwrap();
    let mut stack: Vec<(Vec<u8>, i64)> = Vec::new();
    while let Some(node) = fts.read(&mut fs).unwrap() {
        if node.info == FTS_D {
            stack.push((node.name.clone(), node.level));
        } else if node.info == FTS_DP {
            let (name, level) = stack.pop().expect("matching FTS_D");
            assert_eq!(name, node.name);
            assert_eq!(level, node.level);
        }
        let expected = match node.name.as_slice() {
            b"top" => Some(0),
            b"mid" => Some(1),
            b"bot" => Some(2),
            b"leaf" => Some(3),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(node.level, expected);
        }
    }
    assert!(stack.is_empty());
    fts.close(&mut fs).unwrap();
}

#[test]
fn ftsent_fields() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/file", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    while let Some(node) = fts.read(&mut fs).unwrap() {
        assert_eq!(node.errno, 0);
        if node.info == FTS_D {
            let stat = fs.fstatat(AT_FDCWD, &node.path, true).unwrap();
            assert_eq!(node.dev, stat.st_dev);
            assert_eq!(node.ino, stat.st_ino);
            assert_eq!(node.nlink, u64::from(stat.st_nlink));
        }
    }
    fts.close(&mut fs).unwrap();
}

#[test]
fn symlink_loop_physical() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.symlink("dir/a", "b");
    fs.symlink("dir/b", "a");
    let nodes = walk(&mut fs, &[b"dir"], FTS_PHYSICAL);
    assert!(nodes.len() < 100);
    assert!(
        nodes
            .iter()
            .all(|(info, _, _, _)| matches!(*info, FTS_D | FTS_DP | FTS_SL))
    );
}

#[test]
fn cycle_detection() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.symlink("dir/up", "..");
    let nodes = walk(&mut fs, &[b"dir"], FTS_LOGICAL);
    assert!(nodes.len() < 100);
    assert!(nodes.iter().any(|(info, _, _, _)| *info == FTS_DC));
}

#[test]
fn close_after_root_deleted() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/file", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    let _ = fts.read(&mut fs).unwrap();
    fs.remove("dir/file");
    fs.remove("dir");
    while fts.read(&mut fs).unwrap().is_some() {}
    fts.close(&mut fs).unwrap();
    assert_eq!(fs.open_fd_count(), 0);
}

#[test]
fn close_after_root_moved() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/file", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    let _ = fts.read(&mut fs).unwrap();
    fs.rename("dir", "dir_moved");
    while fts.read(&mut fs).unwrap().is_some() {}
    fts.close(&mut fs).unwrap();
    assert_eq!(fs.open_fd_count(), 0);
}

#[test]
fn nochdir_empty_terminal_dir() {
    let mut fs = FakeFs::new();
    fs.mkdir("parent", 0o755);
    fs.mkdir("parent/empty", 0o755);
    fs.file("parent/sibling", 0);
    let nodes = walk(&mut fs, &[b"parent"], FTS_PHYSICAL | FTS_NOCHDIR);
    let sequence: Vec<(i32, &[u8])> = nodes
        .iter()
        .map(|(info, _, path, _)| (*info, path.as_slice()))
        .collect();
    assert_eq!(
        sequence,
        vec![
            (FTS_D, b"parent".as_slice()),
            (FTS_D, b"parent/empty"),
            (FTS_DP, b"parent/empty"),
            (FTS_F, b"parent/sibling"),
            (FTS_DP, b"parent"),
        ]
    );
}

#[test]
fn ns_errno_set() {
    let mut fs = FakeFs::new();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"nonexistent"], FTS_PHYSICAL, None).unwrap();
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.info, FTS_NS);
    assert_ne!(node.errno, 0);
    fts.close(&mut fs).unwrap();
}

#[test]
fn xdev() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.mkdir("dir/mnt", 0o755);
    fs.file("dir/file", 0);
    fs.file("dir/mnt/inside", 0);
    fs.mount("dir/mnt", 4);
    let nodes = walk(&mut fs, &[b"dir"], FTS_PHYSICAL | FTS_XDEV);
    assert!(!nodes.iter().any(|(_, name, _, _)| name == b"inside"));
}

#[test]
fn no_fd_leak_on_early_close() {
    let mut fs = FakeFs::new();
    fs.mkdir("sub1", 0o755);
    fs.mkdir("sub1/deep", 0o755);
    fs.file("sub1/deep/file", 0);
    let before = fs.open_fd_count();
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"."], FTS_PHYSICAL, None).unwrap();
    assert!(fts.read(&mut fs).unwrap().is_some());
    assert!(fts.read(&mut fs).unwrap().is_some());
    assert!(fts.read(&mut fs).unwrap().is_some());
    fts.close(&mut fs).unwrap();
    assert_eq!(fs.open_fd_count(), before);
}
