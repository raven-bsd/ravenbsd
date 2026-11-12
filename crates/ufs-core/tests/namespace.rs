// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Namespace operation tests: directories, create, unlink, rename, link and symlink.

mod common;

use common::{VecDisk, load_gz, read_all, vectors_dir};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use ufs_core::{BlockIo, FileType, Ino, IoError, MountOptions, Ufs, UfsError};

fn mount_rw(name: &str) -> Ufs<VecDisk> {
    let path = vectors_dir().join("golden").join(format!("{name}.img.gz"));
    let data = load_gz(&path);
    Ufs::mount(VecDisk::new(data), MountOptions { read_only: false }).expect("writable mount")
}

fn mount_ro(name: &str) -> Ufs<VecDisk> {
    let path = vectors_dir().join("golden").join(format!("{name}.img.gz"));
    let data = load_gz(&path);
    Ufs::mount(VecDisk::new(data), MountOptions { read_only: true }).expect("read-only mount")
}

fn remount(ufs: Ufs<VecDisk>) -> Ufs<VecDisk> {
    let disk = ufs.unmount().expect("unmount");
    Ufs::mount(disk, MountOptions { read_only: false }).expect("remount")
}

fn names(ufs: &mut Ufs<VecDisk>, dir: Ino) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    ufs.readdir(dir, 0, &mut |entry, _| {
        if entry.name != b"." && entry.name != b".." {
            out.push(entry.name.clone());
        }
        true
    })
    .expect("readdir");
    out
}

fn entries(ufs: &mut Ufs<VecDisk>, dir: Ino) -> Vec<(Vec<u8>, Ino, FileType)> {
    let mut out = Vec::new();
    ufs.readdir(dir, 0, &mut |entry, _| {
        if entry.name != b"." && entry.name != b".." {
            out.push((entry.name.clone(), entry.ino, entry.kind));
        }
        true
    })
    .expect("readdir");
    out
}

fn create_file(ufs: &mut Ufs<VecDisk>, dir: Ino, name: &[u8]) -> Ino {
    ufs.create(dir, name, 0o644, 1000, 100)
        .expect("create file")
}

fn create_dir(ufs: &mut Ufs<VecDisk>, dir: Ino, name: &[u8]) -> Ino {
    ufs.mkdir(dir, name, 0o755, 1000, 100)
        .expect("create directory")
}

#[test]
fn create_file_attrs_and_lookup() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let ino = create_file(&mut ufs, root, b"k59-file");
    let attr = ufs.getattr(ino).expect("attr");
    assert_eq!(attr.kind, FileType::Regular);
    assert_eq!(attr.mode, 0o644);
    assert_eq!(attr.uid, 1000);
    assert_eq!(attr.gid, 100);
    assert_eq!(attr.size, 0);
    assert_eq!(attr.nlink, 1);
    assert_eq!(ufs.lookup(root, b"k59-file").expect("lookup"), ino);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn create_existing_name_is_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    create_file(&mut ufs, root, b"k59-dup");
    assert_eq!(
        ufs.create(root, b"k59-dup", 0o644, 0, 0),
        Err(UfsError::Exists)
    );
    assert_eq!(
        ufs.mkdir(root, b"k59-dup", 0o755, 0, 0),
        Err(UfsError::Exists)
    );
}

#[test]
fn create_rejects_dot_names_and_bad_names() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    assert_eq!(ufs.create(root, b".", 0o644, 0, 0), Err(UfsError::Invalid));
    assert_eq!(ufs.create(root, b"..", 0o644, 0, 0), Err(UfsError::Invalid));
    assert_eq!(
        ufs.create(root, b"a/b", 0o644, 0, 0),
        Err(UfsError::Invalid)
    );
    assert_eq!(
        ufs.create(root, b"a\0b", 0o644, 0, 0),
        Err(UfsError::Invalid)
    );
    assert_eq!(ufs.create(root, b"", 0o644, 0, 0), Err(UfsError::Invalid));
    assert_eq!(
        ufs.create(root, &[b'n'; 256], 0o644, 0, 0),
        Err(UfsError::NameTooLong)
    );
}

#[test]
fn create_parent_must_be_directory() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let file = create_file(&mut ufs, root, b"k59-notdir");
    assert_eq!(
        ufs.create(file, b"child", 0o644, 0, 0),
        Err(UfsError::NotDir)
    );
}

#[test]
fn mkdir_writes_dots_and_link_counts() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let before = ufs.getattr(root).expect("attr").nlink;
    let dir = create_dir(&mut ufs, root, b"k59-dir");
    let attr = ufs.getattr(dir).expect("attr");
    assert_eq!(attr.kind, FileType::Directory);
    assert_eq!(attr.nlink, 2);
    assert_eq!(attr.size, 512);
    assert_eq!(ufs.getattr(root).expect("attr").nlink, before + 1);
    assert_eq!(ufs.lookup(dir, b".").expect("dot"), dir);
    assert_eq!(ufs.lookup(dir, b"..").expect("dotdot"), root);
    assert!(names(&mut ufs, dir).is_empty());
    ufs.verify_allocation().expect("verify");
}

#[test]
fn unlink_file_frees_inode() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let ino = create_file(&mut ufs, root, b"k59-gone");
    let free_before = ufs.statfs().files_free;
    ufs.unlink(root, b"k59-gone").expect("unlink");
    assert_eq!(ufs.lookup(root, b"k59-gone"), Err(UfsError::NotFound));
    assert!(ufs.getattr(ino).is_err());
    assert_eq!(ufs.statfs().files_free, free_before + 1);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn unlink_directory_is_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    create_dir(&mut ufs, root, b"k59-dir-unlink");
    assert_eq!(ufs.unlink(root, b"k59-dir-unlink"), Err(UfsError::IsDir));
    ufs.rmdir(root, b"k59-dir-unlink").expect("rmdir");
}

#[test]
fn rmdir_empty_directory() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let before = ufs.getattr(root).expect("attr").nlink;
    create_dir(&mut ufs, root, b"k59-empty");
    ufs.rmdir(root, b"k59-empty").expect("rmdir");
    assert_eq!(ufs.lookup(root, b"k59-empty"), Err(UfsError::NotFound));
    assert_eq!(ufs.getattr(root).expect("attr").nlink, before);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn rmdir_non_empty_is_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let dir = create_dir(&mut ufs, root, b"k59-busy");
    create_file(&mut ufs, dir, b"inside");
    assert_eq!(ufs.rmdir(root, b"k59-busy"), Err(UfsError::NotEmpty));
    ufs.unlink(dir, b"inside").expect("unlink");
    ufs.rmdir(root, b"k59-busy").expect("rmdir");
}

#[test]
fn rmdir_file_and_missing_are_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    create_file(&mut ufs, root, b"k59-regular");
    assert_eq!(ufs.rmdir(root, b"k59-regular"), Err(UfsError::NotDir));
    assert_eq!(ufs.rmdir(root, b"k59-missing"), Err(UfsError::NotFound));
}

#[test]
fn link_and_unlink_keep_link_count() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let file = create_file(&mut ufs, root, b"k59-a");
    ufs.link(root, b"k59-b", file).expect("link");
    assert_eq!(ufs.getattr(file).expect("attr").nlink, 2);
    assert_eq!(ufs.lookup(root, b"k59-b").expect("lookup"), file);
    ufs.unlink(root, b"k59-a").expect("unlink a");
    assert_eq!(ufs.getattr(file).expect("attr").nlink, 1);
    ufs.unlink(root, b"k59-b").expect("unlink b");
    assert_eq!(ufs.lookup(root, b"k59-b"), Err(UfsError::NotFound));
    assert!(ufs.getattr(file).is_err());
    ufs.verify_allocation().expect("verify");
}

#[test]
fn link_directory_is_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let dir = create_dir(&mut ufs, root, b"k59-dirlink");
    assert_eq!(ufs.link(root, b"k59-dirlink2", dir), Err(UfsError::Invalid));
    ufs.rmdir(root, b"k59-dirlink").expect("rmdir");
}

#[test]
fn symlink_target_sizes() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    for len in [59usize, 60, 61, 400] {
        let name = format!("k59-link-{len}");
        let target: Vec<u8> = (0..len).map(|index| b'a' + (index % 26) as u8).collect();
        let ino = ufs
            .symlink(root, name.as_bytes(), &target, 1000, 100)
            .expect("symlink");
        let attr = ufs.getattr(ino).expect("attr");
        assert_eq!(attr.kind, FileType::Symlink);
        assert_eq!(attr.nlink, 1);
        let mut buf = vec![0u8; len + 10];
        let got = ufs.readlink(ino, &mut buf).expect("readlink");
        assert_eq!(got, len);
        assert_eq!(&buf[..got], &target[..]);
    }
    ufs.verify_allocation().expect("verify");
}

#[test]
fn symlink_existing_name_is_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    create_file(&mut ufs, root, b"k59-exists");
    assert_eq!(
        ufs.symlink(root, b"k59-exists", b"target", 0, 0),
        Err(UfsError::Exists)
    );
}

#[test]
fn create_and_mkdir_readonly_mounts_fail() {
    let mut ufs = mount_ro("ufs2-b4096");
    let root = ufs.root();
    assert_eq!(
        ufs.create(root, b"k59-ro", 0o644, 0, 0),
        Err(UfsError::ReadOnly)
    );
    assert_eq!(
        ufs.mkdir(root, b"k59-ro-dir", 0o755, 0, 0),
        Err(UfsError::ReadOnly)
    );
    assert_eq!(ufs.unlink(root, b"ten.txt"), Err(UfsError::ReadOnly));
    assert_eq!(
        ufs.symlink(root, b"k59-ro-link", b"x", 0, 0),
        Err(UfsError::ReadOnly)
    );
}

#[test]
fn operations_persist_across_remount() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let dir = create_dir(&mut ufs, root, b"k59-persist");
    let file = create_file(&mut ufs, dir, b"file");
    ufs.write(file, 0, b"content").expect("write");
    ufs.link(dir, b"alias", file).expect("link");
    ufs.symlink(dir, b"link", b"file", 0, 0).expect("symlink");
    let mut ufs = remount(ufs);
    assert_eq!(ufs.lookup(root, b"k59-persist").expect("dir"), dir);
    assert_eq!(ufs.lookup(dir, b"file").expect("file"), file);
    assert_eq!(ufs.lookup(dir, b"alias").expect("alias"), file);
    assert_eq!(read_all(&mut ufs, file, 7).expect("read"), b"content");
    assert_eq!(ufs.getattr(file).expect("attr").nlink, 2);
}

#[test]
fn long_name_255_bytes_round_trips() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let mut name = [b'n'; 255];
    name[254] = b'x';
    let ino = create_file(&mut ufs, root, &name);
    assert_eq!(ufs.lookup(root, &name).expect("lookup"), ino);
    ufs.unlink(root, &name).expect("unlink");
}

#[test]
fn rename_missing_source_is_not_found() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    assert_eq!(
        ufs.rename(root, b"k59-missing", root, b"k59-new"),
        Err(UfsError::NotFound)
    );
}

#[test]
fn rename_same_name_is_noop() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let ino = create_file(&mut ufs, root, b"k59-same");
    ufs.rename(root, b"k59-same", root, b"k59-same")
        .expect("noop");
    assert_eq!(ufs.lookup(root, b"k59-same").expect("lookup"), ino);
    ufs.unlink(root, b"k59-same").expect("unlink");
}

#[test]
fn rename_file_across_directories() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let a = create_dir(&mut ufs, root, b"k59-a");
    let b = create_dir(&mut ufs, root, b"k59-b");
    let file = create_file(&mut ufs, a, b"file");
    ufs.write(file, 0, b"data").expect("write");
    ufs.rename(a, b"file", b, b"moved").expect("rename");
    assert_eq!(ufs.lookup(a, b"file"), Err(UfsError::NotFound));
    assert_eq!(ufs.lookup(b, b"moved").expect("lookup"), file);
    assert_eq!(read_all(&mut ufs, file, 4).expect("read"), b"data");
    assert_eq!(ufs.getattr(file).expect("attr").nlink, 1);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn rename_file_over_file_replaces() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let source = create_file(&mut ufs, root, b"k59-source");
    let target = create_file(&mut ufs, root, b"k59-target");
    ufs.write(source, 0, b"source").expect("write source");
    ufs.write(target, 0, b"target").expect("write target");
    ufs.rename(root, b"k59-source", root, b"k59-target")
        .expect("rename over");
    assert_eq!(ufs.lookup(root, b"k59-source"), Err(UfsError::NotFound));
    assert_eq!(ufs.lookup(root, b"k59-target").expect("lookup"), source);
    assert_eq!(read_all(&mut ufs, source, 6).expect("read"), b"source");
    assert_eq!(ufs.getattr(source).expect("attr").nlink, 1);
    assert!(ufs.getattr(target).is_err());
    ufs.verify_allocation().expect("verify");
}

#[test]
fn rename_hard_link_to_same_inode_is_noop() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let file = create_file(&mut ufs, root, b"k59-link-a");
    ufs.link(root, b"k59-link-b", file).expect("link");
    ufs.rename(root, b"k59-link-a", root, b"k59-link-b")
        .expect("rename same inode");
    assert_eq!(ufs.lookup(root, b"k59-link-a").expect("a"), file);
    assert_eq!(ufs.lookup(root, b"k59-link-b").expect("b"), file);
    assert_eq!(ufs.getattr(file).expect("attr").nlink, 2);
    ufs.unlink(root, b"k59-link-a").expect("unlink a");
    ufs.unlink(root, b"k59-link-b").expect("unlink b");
}

#[test]
fn rename_directory_same_parent() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let parent_links = ufs.getattr(root).expect("attr").nlink;
    let dir = create_dir(&mut ufs, root, b"k59-dir-a");
    ufs.rename(root, b"k59-dir-a", root, b"k59-dir-b")
        .expect("rename dir");
    assert_eq!(ufs.lookup(root, b"k59-dir-a"), Err(UfsError::NotFound));
    assert_eq!(ufs.lookup(root, b"k59-dir-b").expect("lookup"), dir);
    assert_eq!(ufs.lookup(dir, b"..").expect("dotdot"), root);
    assert_eq!(ufs.getattr(root).expect("attr").nlink, parent_links + 1);
    ufs.rmdir(root, b"k59-dir-b").expect("rmdir");
    assert_eq!(ufs.getattr(root).expect("attr").nlink, parent_links);
}

#[test]
fn rename_directory_reparents_and_updates_dotdot() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let a = create_dir(&mut ufs, root, b"k59-move-a");
    let b = create_dir(&mut ufs, root, b"k59-move-b");
    let child = create_dir(&mut ufs, a, b"child");
    let root_links = ufs.getattr(root).expect("attr").nlink;
    ufs.rename(a, b"child", b, b"child").expect("reparent");
    assert_eq!(ufs.lookup(a, b"child"), Err(UfsError::NotFound));
    assert_eq!(ufs.lookup(b, b"child").expect("lookup"), child);
    assert_eq!(ufs.lookup(child, b"..").expect("dotdot"), b);
    assert_eq!(ufs.getattr(root).expect("attr").nlink, root_links);
    ufs.rmdir(b, b"child").expect("rmdir child");
    ufs.rmdir(root, b"k59-move-a").expect("rmdir a");
    ufs.rmdir(root, b"k59-move-b").expect("rmdir b");
    ufs.verify_allocation().expect("verify");
}

#[test]
fn rename_directory_over_empty_same_parent() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let parent_links = ufs.getattr(root).expect("attr").nlink;
    let source = create_dir(&mut ufs, root, b"k59-src-dir");
    let target = create_dir(&mut ufs, root, b"k59-target-dir");
    ufs.rename(root, b"k59-src-dir", root, b"k59-target-dir")
        .expect("rename over dir");
    assert_eq!(ufs.lookup(root, b"k59-src-dir"), Err(UfsError::NotFound));
    assert_eq!(ufs.lookup(root, b"k59-target-dir").expect("lookup"), source);
    assert_eq!(ufs.getattr(source).expect("attr").nlink, 2);
    assert!(ufs.getattr(target).is_err());
    assert_eq!(ufs.getattr(root).expect("attr").nlink, parent_links + 1);
    ufs.rmdir(root, b"k59-target-dir").expect("rmdir");
    assert_eq!(ufs.getattr(root).expect("attr").nlink, parent_links);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn rename_directory_over_empty_across_parents() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let a = create_dir(&mut ufs, root, b"k59-ra");
    let b = create_dir(&mut ufs, root, b"k59-rb");
    let source = create_dir(&mut ufs, a, b"src");
    let target = create_dir(&mut ufs, b, b"target");
    let root_links = ufs.getattr(root).expect("attr").nlink;
    ufs.rename(a, b"src", b, b"target").expect("rename over");
    assert_eq!(ufs.lookup(a, b"src"), Err(UfsError::NotFound));
    assert_eq!(ufs.lookup(b, b"target").expect("lookup"), source);
    assert_eq!(ufs.lookup(source, b"..").expect("dotdot"), b);
    assert_eq!(ufs.getattr(root).expect("attr").nlink, root_links);
    assert!(ufs.getattr(target).is_err());
    ufs.rmdir(b, b"target").expect("rmdir");
    ufs.rmdir(root, b"k59-ra").expect("rmdir a");
    ufs.rmdir(root, b"k59-rb").expect("rmdir b");
    ufs.verify_allocation().expect("verify");
}

#[test]
fn rename_directory_into_subtree_is_invalid() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let a = create_dir(&mut ufs, root, b"k59-subtree");
    let child = create_dir(&mut ufs, a, b"child");
    assert_eq!(
        ufs.rename(root, b"k59-subtree", child, b"moved"),
        Err(UfsError::Invalid)
    );
    assert_eq!(
        ufs.rename(root, b"k59-subtree", a, b"moved"),
        Err(UfsError::Invalid)
    );
    ufs.rmdir(a, b"child").expect("rmdir child");
    ufs.rmdir(root, b"k59-subtree").expect("rmdir");
}

#[test]
fn rename_directory_over_nonempty_is_not_empty() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let source = create_dir(&mut ufs, root, b"k59-ne-src");
    let target = create_dir(&mut ufs, root, b"k59-ne-target");
    create_file(&mut ufs, target, b"inside");
    assert_eq!(
        ufs.rename(root, b"k59-ne-src", root, b"k59-ne-target"),
        Err(UfsError::NotEmpty)
    );
    assert_eq!(ufs.lookup(root, b"k59-ne-src").expect("source"), source);
    ufs.unlink(target, b"inside").expect("unlink");
    ufs.rmdir(root, b"k59-ne-src").expect("rmdir src");
    ufs.rmdir(root, b"k59-ne-target").expect("rmdir target");
}

#[test]
fn rename_type_mismatches_are_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let file = create_file(&mut ufs, root, b"k59-file-x");
    let _dir = create_dir(&mut ufs, root, b"k59-dir-x");
    assert_eq!(
        ufs.rename(root, b"k59-file-x", root, b"k59-dir-x"),
        Err(UfsError::IsDir)
    );
    assert_eq!(
        ufs.rename(root, b"k59-dir-x", root, b"k59-file-x"),
        Err(UfsError::NotDir)
    );
    ufs.unlink(root, b"k59-file-x").expect("unlink file");
    ufs.rmdir(root, b"k59-dir-x").expect("rmdir dir");
    let _ = file;
}

#[test]
fn rename_dot_names_are_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let dir = create_dir(&mut ufs, root, b"k59-dot");
    assert_eq!(ufs.rename(dir, b".", root, b"x"), Err(UfsError::Invalid));
    assert_eq!(ufs.rename(dir, b"..", root, b"x"), Err(UfsError::Invalid));
    assert_eq!(
        ufs.rename(root, b"k59-dot", dir, b"."),
        Err(UfsError::Invalid)
    );
    ufs.rmdir(root, b"k59-dot").expect("rmdir");
}

#[test]
fn rename_readonly_mount_fails() {
    let mut ufs = mount_ro("ufs2-b4096");
    let root = ufs.root();
    assert_eq!(
        ufs.rename(root, b"ten.txt", root, b"k59-renamed"),
        Err(UfsError::ReadOnly)
    );
}

#[test]
fn rename_persists_across_remount() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let dir = create_dir(&mut ufs, root, b"k59-remount");
    let file = create_file(&mut ufs, dir, b"old");
    ufs.rename(dir, b"old", dir, b"new").expect("rename");
    let mut ufs = remount(ufs);
    assert_eq!(ufs.lookup(dir, b"new").expect("lookup"), file);
    assert_eq!(ufs.lookup(dir, b"old"), Err(UfsError::NotFound));
}

#[test]
fn unlink_missing_is_not_found() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    assert_eq!(ufs.unlink(root, b"k59-nope"), Err(UfsError::NotFound));
}

#[test]
fn unlink_and_rmdir_reject_dot_names() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let dir = create_dir(&mut ufs, root, b"k59-dots");
    assert_eq!(ufs.unlink(dir, b"."), Err(UfsError::Invalid));
    assert_eq!(ufs.unlink(dir, b".."), Err(UfsError::Invalid));
    assert_eq!(ufs.rmdir(dir, b"."), Err(UfsError::Invalid));
    assert_eq!(ufs.rmdir(dir, b".."), Err(UfsError::Invalid));
    ufs.rmdir(root, b"k59-dots").expect("rmdir");
}

#[test]
fn link_missing_target_and_existing_name() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    assert_eq!(
        ufs.link(root, b"k59-missing-link", 999_999),
        Err(UfsError::NotFound)
    );
    let file = create_file(&mut ufs, root, b"k59-link-src");
    create_file(&mut ufs, root, b"k59-link-name");
    assert_eq!(
        ufs.link(root, b"k59-link-name", file),
        Err(UfsError::Exists)
    );
}

#[test]
fn link_to_symlink_keeps_target() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let link = ufs
        .symlink(root, b"k59-sym-src", b"destination", 0, 0)
        .expect("symlink");
    ufs.link(root, b"k59-sym-hard", link).expect("hard link");
    assert_eq!(ufs.getattr(link).expect("attr").nlink, 2);
    let mut buf = [0u8; 32];
    let got = ufs.readlink(link, &mut buf).expect("readlink");
    assert_eq!(&buf[..got], b"destination");
    ufs.unlink(root, b"k59-sym-src").expect("unlink");
    assert_eq!(ufs.getattr(link).expect("attr").nlink, 1);
}

#[test]
fn rename_file_over_symlink_replaces_it() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let file = create_file(&mut ufs, root, b"k59-r-file");
    let link = ufs
        .symlink(root, b"k59-r-link", b"old-target", 0, 0)
        .expect("symlink");
    ufs.write(file, 0, b"payload").expect("write");
    ufs.rename(root, b"k59-r-file", root, b"k59-r-link")
        .expect("rename over symlink");
    assert_eq!(ufs.lookup(root, b"k59-r-link").expect("lookup"), file);
    assert_eq!(read_all(&mut ufs, file, 7).expect("read"), b"payload");
    assert!(ufs.getattr(link).is_err());
    ufs.verify_allocation().expect("verify");
}

#[test]
fn rename_target_with_extra_hard_link_keeps_inode() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let source = create_file(&mut ufs, root, b"k59-h-source");
    let target = create_file(&mut ufs, root, b"k59-h-target");
    ufs.link(root, b"k59-h-extra", target).expect("link target");
    ufs.rename(root, b"k59-h-source", root, b"k59-h-target")
        .expect("rename");
    assert_eq!(ufs.lookup(root, b"k59-h-target").expect("lookup"), source);
    assert_eq!(ufs.getattr(target).expect("target").nlink, 1);
    assert_eq!(ufs.lookup(root, b"k59-h-extra").expect("extra"), target);
}

#[test]
fn mkdir_and_symlink_attrs() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let dir = ufs
        .mkdir(root, b"k59-attrs-dir", 0o750, 42, 43)
        .expect("mkdir");
    let attr = ufs.getattr(dir).expect("attr");
    assert_eq!(attr.mode, 0o750);
    assert_eq!(attr.uid, 42);
    assert_eq!(attr.gid, 43);
    let link = ufs
        .symlink(root, b"k59-attrs-link", b"x", 44, 45)
        .expect("symlink");
    let attr = ufs.getattr(link).expect("attr");
    assert_eq!(attr.mode, 0o777);
    assert_eq!(attr.uid, 44);
    assert_eq!(attr.gid, 45);
}

#[test]
fn nested_directories_create_and_remove() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let mut parent = root;
    let mut made = Vec::new();
    for depth in 0..40 {
        let name = format!("k59-depth-{depth}");
        let dir = create_dir(&mut ufs, parent, name.as_bytes());
        assert_eq!(ufs.lookup(dir, b"..").expect("dotdot"), parent);
        made.push((parent, name.into_bytes(), dir));
        parent = dir;
    }
    let leaf = create_file(&mut ufs, parent, b"leaf");
    assert_eq!(ufs.lookup(parent, b"leaf").expect("leaf"), leaf);
    ufs.unlink(parent, b"leaf").expect("unlink leaf");
    for (parent, name, _) in made.iter().rev() {
        ufs.rmdir(*parent, name).expect("rmdir nested");
    }
    ufs.verify_allocation().expect("verify");
}

#[test]
fn directory_grows_for_3000_entries_and_lists_all() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let free_before = ufs.statfs().blocks_free;
    let dir = create_dir(&mut ufs, root, b"k59-large-dir");
    let file = create_file(&mut ufs, dir, b"base");
    for index in 0..3000 {
        let name = format!("e{index:04}");
        ufs.link(dir, name.as_bytes(), file).expect("link entry");
    }
    assert_eq!(ufs.getattr(file).expect("attr").nlink, 3001);
    let listed = entries(&mut ufs, dir);
    assert_eq!(listed.len(), 3001);
    let attr = ufs.getattr(dir).expect("dir attr");
    let fsize = ufs.statfs().fragment_size as u64;
    let used = free_before - ufs.statfs().blocks_free;
    assert_eq!(
        attr.blocks * 512,
        used * fsize,
        "directory block accounting"
    );
    ufs.verify_allocation().expect("verify");
}

#[test]
fn directory_remove_half_and_reuse_space() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let dir = create_dir(&mut ufs, root, b"k59-reuse-dir");
    let file = create_file(&mut ufs, dir, b"base");
    for index in 0..3000 {
        let name = format!("e{index:04}");
        ufs.link(dir, name.as_bytes(), file).expect("link entry");
    }
    for index in (0..3000).step_by(2) {
        let name = format!("e{index:04}");
        ufs.unlink(dir, name.as_bytes()).expect("unlink half");
    }
    assert_eq!(entries(&mut ufs, dir).len(), 1501);
    for index in 0..1500 {
        let name = format!("n{index:04}");
        ufs.link(dir, name.as_bytes(), file).expect("reuse slot");
    }
    assert_eq!(entries(&mut ufs, dir).len(), 3001);
    assert_eq!(ufs.getattr(file).expect("attr").nlink, 3001);
    ufs.verify_allocation().expect("verify");
    let names = entries(&mut ufs, dir);
    assert!(names.iter().any(|(name, _, _)| name == b"e0001"));
    assert!(names.iter().any(|(name, _, _)| name == b"n1499"));
}

#[test]
fn directory_entries_fill_blocks_without_straddling() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let dir = create_dir(&mut ufs, root, b"k59-align-dir");
    let file = create_file(&mut ufs, dir, b"base");
    for index in 0..120 {
        let len = 1 + (index * 7) % 60;
        let name: Vec<u8> = (0..len)
            .map(|pos| b'a' + ((index + pos) % 26) as u8)
            .collect();
        let mut name = name;
        name.extend_from_slice(format!("-{index}").as_bytes());
        ufs.link(dir, &name, file).expect("link");
    }
    let listed = entries(&mut ufs, dir);
    assert_eq!(listed.len(), 121);
    for (name, ino, kind) in listed {
        assert_eq!(ino, file);
        assert_eq!(kind, FileType::Regular);
        assert!(!name.is_empty());
    }
    ufs.verify_allocation().expect("verify");
}

#[test]
fn create_on_ufs1_and_8k_images() {
    let mut ufs = mount_rw("ufs1-b4096");
    let root = ufs.root();
    let dir = create_dir(&mut ufs, root, b"k59-ufs1-dir");
    let file = create_file(&mut ufs, dir, b"file");
    ufs.write(file, 0, &pattern(5000)).expect("write");
    ufs.rename(dir, b"file", root, b"k59-ufs1-file")
        .expect("rename");
    assert_eq!(ufs.lookup(root, b"k59-ufs1-file").expect("lookup"), file);
    ufs.verify_allocation().expect("verify");

    let mut ufs = mount_rw("ufs2-b8192");
    let root = ufs.root();
    let dir = create_dir(&mut ufs, root, b"k59-8k-dir");
    let file = create_file(&mut ufs, dir, b"file");
    ufs.write(file, 0, &pattern(9000)).expect("write");
    ufs.rename(dir, b"file", root, b"k59-8k-file")
        .expect("rename");
    assert_eq!(ufs.lookup(root, b"k59-8k-file").expect("lookup"), file);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn rename_parents_must_be_directories() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let file = create_file(&mut ufs, root, b"k59-parent-file");
    let dir = create_dir(&mut ufs, root, b"k59-parent-dir");
    assert_eq!(ufs.rename(file, b"x", root, b"y"), Err(UfsError::NotDir));
    assert_eq!(
        ufs.rename(root, b"k59-parent-dir", file, b"y"),
        Err(UfsError::NotDir)
    );
    ufs.unlink(root, b"k59-parent-file").expect("unlink");
    let _ = dir;
}

#[test]
fn hard_link_preserves_content_after_unlink() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let file = create_file(&mut ufs, root, b"k59-content-a");
    ufs.write(file, 0, b"shared content").expect("write");
    ufs.link(root, b"k59-content-b", file).expect("link");
    ufs.unlink(root, b"k59-content-a").expect("unlink");
    assert_eq!(ufs.getattr(file).expect("attr").nlink, 1);
    assert_eq!(
        read_all(&mut ufs, file, 14).expect("read"),
        b"shared content"
    );
}

#[test]
fn create_after_unlink_reuses_name() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let first = create_file(&mut ufs, root, b"k59-recreate");
    ufs.write(first, 0, b"first").expect("write");
    ufs.unlink(root, b"k59-recreate").expect("unlink");
    let second = create_file(&mut ufs, root, b"k59-recreate");
    assert_eq!(ufs.lookup(root, b"k59-recreate").expect("lookup"), second);
    assert_eq!(ufs.getattr(second).expect("attr").size, 0);
}

#[test]
fn symlink_empty_target() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let link = ufs
        .symlink(root, b"k59-empty-link", b"", 0, 0)
        .expect("symlink");
    assert_eq!(ufs.getattr(link).expect("attr").kind, FileType::Symlink);
    let mut buf = [0u8; 8];
    assert_eq!(ufs.readlink(link, &mut buf).expect("readlink"), 0);
    ufs.unlink(root, b"k59-empty-link").expect("unlink");
}

#[test]
fn rmdir_after_moving_child_out() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let source = create_dir(&mut ufs, root, b"k59-move-out");
    let target = create_dir(&mut ufs, root, b"k59-move-in");
    let child = create_dir(&mut ufs, source, b"child");
    assert_eq!(ufs.getattr(source).expect("attr").nlink, 3);
    ufs.rename(source, b"child", target, b"child")
        .expect("move child");
    assert_eq!(ufs.getattr(source).expect("attr").nlink, 2);
    ufs.rmdir(root, b"k59-move-out")
        .expect("rmdir empty parent");
    assert_eq!(ufs.lookup(target, b"child").expect("child"), child);
}

#[test]
fn create_setuid_mode_preserved() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let ino = ufs
        .create(root, b"k59-setuid", 0o4755, 0, 0)
        .expect("create");
    assert_eq!(ufs.getattr(ino).expect("attr").mode, 0o4755);
}

#[test]
fn rename_long_names_between_directories() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let a = create_dir(&mut ufs, root, b"k59-long-a");
    let b = create_dir(&mut ufs, root, b"k59-long-b");
    let mut name = [b'x'; 200];
    name[199] = b'z';
    let file = create_file(&mut ufs, a, &name);
    let mut new_name = [b'y'; 255];
    new_name[254] = b'q';
    ufs.rename(a, &name, b, &new_name).expect("rename");
    assert_eq!(ufs.lookup(b, &new_name).expect("lookup"), file);
    assert_eq!(ufs.lookup(a, &name), Err(UfsError::NotFound));
}

#[test]
fn unlink_last_link_frees_file_blocks() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let ino = create_file(&mut ufs, root, b"k59-data-file");
    let free_before = ufs.statfs().blocks_free;
    ufs.write(ino, 0, &pattern(3 * 4096 + 100)).expect("write");
    assert!(ufs.statfs().blocks_free < free_before);
    ufs.link(root, b"k59-data-alias", ino).expect("link");
    ufs.unlink(root, b"k59-data-file").expect("unlink first");
    assert!(ufs.statfs().blocks_free < free_before);
    ufs.unlink(root, b"k59-data-alias").expect("unlink last");
    assert_eq!(ufs.statfs().blocks_free, free_before);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn rmdir_indirect_directory_frees_blocks() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let free_before = ufs.statfs().blocks_free;
    let dir = create_dir(&mut ufs, root, b"k59-indirect-dir");
    let file = create_file(&mut ufs, dir, b"base");
    for index in 0..3500 {
        let name = format!("i{index:04}");
        ufs.link(dir, name.as_bytes(), file).expect("link");
    }
    assert!(ufs.getattr(dir).expect("attr").size > 12 * 4096);
    for index in 0..3500 {
        let name = format!("i{index:04}");
        ufs.unlink(dir, name.as_bytes()).expect("unlink entry");
    }
    ufs.unlink(dir, b"base").expect("unlink base");
    ufs.rmdir(root, b"k59-indirect-dir").expect("rmdir");
    assert_eq!(ufs.statfs().blocks_free, free_before);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn rmdir_large_directory_frees_blocks() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let free_before = ufs.statfs().blocks_free;
    let dir = create_dir(&mut ufs, root, b"k59-big-rmdir");
    let file = create_file(&mut ufs, dir, b"base");
    for index in 0..1200 {
        let name = format!("b{index:04}");
        ufs.link(dir, name.as_bytes(), file).expect("link");
    }
    let listed = entries(&mut ufs, dir);
    for (name, ino, kind) in listed {
        if name == b"base" {
            continue;
        }
        assert_eq!(ino, file);
        assert_eq!(kind, FileType::Regular);
        ufs.unlink(dir, &name).expect("unlink entry");
    }
    ufs.unlink(dir, b"base").expect("unlink base");
    ufs.rmdir(root, b"k59-big-rmdir").expect("rmdir large");
    assert_eq!(ufs.statfs().blocks_free, free_before);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn unlink_special_files_from_golden_image() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let cdev = ufs.lookup(root, b"cdev").expect("cdev");
    let fifo = ufs.lookup(root, b"fifo").expect("fifo");
    ufs.unlink(root, b"cdev").expect("unlink cdev");
    ufs.unlink(root, b"fifo").expect("unlink fifo");
    assert_eq!(ufs.lookup(root, b"cdev"), Err(UfsError::NotFound));
    assert_eq!(ufs.lookup(root, b"fifo"), Err(UfsError::NotFound));
    assert!(ufs.getattr(cdev).is_err());
    assert!(ufs.getattr(fifo).is_err());
    ufs.verify_allocation().expect("verify");
}

#[test]
fn link_to_freed_inode_is_not_found() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let ino = create_file(&mut ufs, root, b"k59-freed");
    ufs.unlink(root, b"k59-freed").expect("unlink");
    assert_eq!(
        ufs.link(root, b"k59-resurrect", ino),
        Err(UfsError::NotFound)
    );
}

#[test]
fn rename_file_over_special_file() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let fifo = ufs.lookup(root, b"fifo").expect("fifo");
    let file = create_file(&mut ufs, root, b"k59-over-fifo");
    ufs.rename(root, b"k59-over-fifo", root, b"fifo")
        .expect("rename over fifo");
    assert_eq!(ufs.lookup(root, b"fifo").expect("lookup"), file);
    assert!(ufs.getattr(fifo).is_err());
}

#[test]
fn oracle_fsck_namespace_when_present() {
    let oracle = std::path::Path::new("/tmp/opencode/ufs-oracle/bin/fsck_ffs");
    if !oracle.exists() {
        println!("ORACLE ABSENT: fsck_ffs not installed; skipped");
        return;
    }
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    let dir = create_dir(&mut ufs, root, b"k59-fsck-dir");
    let file = create_file(&mut ufs, dir, b"file");
    ufs.write(file, 0, &pattern(5000)).expect("write");
    ufs.link(dir, b"hard", file).expect("link");
    ufs.symlink(dir, b"sym", b"target", 0, 0).expect("symlink");
    ufs.mkdir(dir, b"sub", 0o755, 0, 0).expect("mkdir");
    ufs.rename(dir, b"file", root, b"k59-fsck-file")
        .expect("rename");
    let disk = ufs.unmount().expect("unmount");
    let path = std::env::temp_dir().join(format!("k59-fsck-{}.img", std::process::id()));
    std::fs::write(&path, &disk.data).expect("write image");
    let output = std::process::Command::new(oracle)
        .arg("-n")
        .arg(&path)
        .output()
        .expect("run fsck_ffs");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "fsck_ffs failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn pattern(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| (index.wrapping_mul(37) ^ 0x5a) as u8)
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModelKind {
    Dir,
    File,
    Link,
}

#[derive(Clone)]
struct ModelNode {
    ino: Ino,
    kind: ModelKind,
    target: Vec<u8>,
}

struct Model {
    /// Path relative to the model root; the root itself is the empty path.
    nodes: BTreeMap<Vec<u8>, ModelNode>,
}

fn path_join(parent: &[u8], name: &[u8]) -> Vec<u8> {
    let mut path = Vec::from(parent);
    if !path.is_empty() {
        path.push(b'/');
    }
    path.extend_from_slice(name);
    path
}

fn path_split(path: &[u8]) -> (&[u8], &[u8]) {
    match path.iter().rposition(|byte| *byte == b'/') {
        Some(index) => (&path[..index], &path[index + 1..]),
        None => (&[], path),
    }
}

fn model_dirs(model: &Model) -> Vec<Vec<u8>> {
    model
        .nodes
        .iter()
        .filter(|(_, node)| node.kind == ModelKind::Dir)
        .map(|(path, _)| path.clone())
        .collect()
}

fn model_files(model: &Model) -> Vec<Vec<u8>> {
    model
        .nodes
        .iter()
        .filter(|(_, node)| node.kind != ModelKind::Dir)
        .map(|(path, _)| path.clone())
        .collect()
}

fn model_children(model: &Model, dir: &[u8]) -> Vec<Vec<u8>> {
    model
        .nodes
        .iter()
        .filter(|(path, _)| !path.is_empty() && path_split(path).0 == dir)
        .map(|(path, _)| path.clone())
        .collect()
}

fn actual_tree(
    ufs: &mut Ufs<VecDisk>,
    dir: Ino,
    path: &[u8],
    out: &mut BTreeMap<Vec<u8>, (Ino, FileType, Vec<u8>)>,
) {
    let mut children = Vec::new();
    ufs.readdir(dir, 0, &mut |entry, _| {
        if entry.name != b"." && entry.name != b".." {
            children.push((entry.name.clone(), entry.ino, entry.kind));
        }
        true
    })
    .expect("walk readdir");
    for (name, ino, kind) in children {
        let child = path_join(path, &name);
        let mut target = Vec::new();
        if kind == FileType::Symlink {
            let size = ufs.getattr(ino).expect("link attr").size as usize;
            let mut buf = vec![0u8; size];
            let got = ufs.readlink(ino, &mut buf).expect("walk readlink");
            assert_eq!(got, size);
            target = buf;
        }
        out.insert(child.clone(), (ino, kind, target));
        if kind == FileType::Directory {
            actual_tree(ufs, ino, &child, out);
        }
    }
}

fn verify_model(ufs: &mut Ufs<VecDisk>, root: Ino, model: &Model, label: &str) {
    let mut actual = BTreeMap::new();
    actual.insert(Vec::new(), (root, FileType::Directory, Vec::new()));
    actual_tree(ufs, root, b"", &mut actual);
    assert_eq!(actual.len(), model.nodes.len(), "{label}: tree size");
    for (path, node) in &model.nodes {
        let (ino, kind, target) = actual.get(path).unwrap_or_else(|| {
            panic!(
                "{label}: missing actual path {:?}",
                String::from_utf8_lossy(path)
            )
        });
        assert_eq!(
            *ino,
            node.ino,
            "{label}: inode for {:?}",
            String::from_utf8_lossy(path)
        );
        assert_eq!(
            *kind,
            match node.kind {
                ModelKind::Dir => FileType::Directory,
                ModelKind::File => FileType::Regular,
                ModelKind::Link => FileType::Symlink,
            },
            "{label}: kind for {:?}",
            String::from_utf8_lossy(path)
        );
        if node.kind == ModelKind::Link {
            assert_eq!(
                *target,
                node.target,
                "{label}: target for {:?}",
                String::from_utf8_lossy(path)
            );
        }
    }

    let mut regular_links: BTreeMap<Ino, u32> = BTreeMap::new();
    for node in model
        .nodes
        .values()
        .filter(|node| node.kind != ModelKind::Dir)
    {
        *regular_links.entry(node.ino).or_default() += 1;
    }
    for (ino, count) in regular_links {
        assert_eq!(ufs.getattr(ino).expect("link inode").nlink, count);
    }
    let dirs = model_dirs(model);
    for dir in &dirs {
        let node = model.nodes.get(dir).expect("dir node");
        let child_dirs: Vec<String> = model
            .nodes
            .iter()
            .filter(|(path, child)| {
                !path.is_empty()
                    && child.kind == ModelKind::Dir
                    && path_split(path).0 == dir.as_slice()
            })
            .map(|(path, _)| String::from_utf8_lossy(path).into_owned())
            .collect();
        let expected = 2 + child_dirs.len() as u32;
        assert_eq!(
            ufs.getattr(node.ino).expect("dir inode").nlink,
            expected,
            "dir link count for {:?} children {:?} ({label})",
            String::from_utf8_lossy(dir),
            child_dirs
        );
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }
}

#[test]
fn randomized_namespace_model_10k() {
    let mut ufs = mount_rw("ufs2-b4096");
    let image_root = ufs.root();
    let root = create_dir(&mut ufs, image_root, b"k59-model");
    let mut model = Model {
        nodes: BTreeMap::new(),
    };
    model.nodes.insert(
        Vec::new(),
        ModelNode {
            ino: root,
            kind: ModelKind::Dir,
            target: Vec::new(),
        },
    );
    let mut rng = Rng(0x51ed_270b_9e37_79b9);
    let mut sequence = 0u32;
    for step in 0..10_000u32 {
        sequence += 1;
        let dirs = model_dirs(&model);
        let files = model_files(&model);
        let choice = if model.nodes.len() > 180 {
            4 + rng.next() % 5
        } else {
            rng.next() % 9
        };
        match choice {
            0 => {
                // create file
                let dir = dirs[(rng.next() as usize) % dirs.len()].clone();
                let name = format!("f{sequence}");
                let path = path_join(&dir, name.as_bytes());
                if model.nodes.contains_key(&path) {
                    continue;
                }
                let parent = model.nodes.get(&dir).expect("dir").ino;
                let ino = ufs
                    .create(parent, name.as_bytes(), 0o600, 1, 2)
                    .expect("model create");
                model.nodes.insert(
                    path,
                    ModelNode {
                        ino,
                        kind: ModelKind::File,
                        target: Vec::new(),
                    },
                );
            }
            1 => {
                // mkdir
                let dir = dirs[(rng.next() as usize) % dirs.len()].clone();
                let name = format!("d{sequence}");
                let path = path_join(&dir, name.as_bytes());
                if model.nodes.contains_key(&path) {
                    continue;
                }
                let parent = model.nodes.get(&dir).expect("dir").ino;
                let ino = ufs
                    .mkdir(parent, name.as_bytes(), 0o700, 1, 2)
                    .expect("model mkdir");
                model.nodes.insert(
                    path,
                    ModelNode {
                        ino,
                        kind: ModelKind::Dir,
                        target: Vec::new(),
                    },
                );
            }
            2 => {
                // symlink
                let dir = dirs[(rng.next() as usize) % dirs.len()].clone();
                let name = format!("s{sequence}");
                let path = path_join(&dir, name.as_bytes());
                if model.nodes.contains_key(&path) {
                    continue;
                }
                let target = format!("target-{sequence}").into_bytes();
                let parent = model.nodes.get(&dir).expect("dir").ino;
                let ino = ufs
                    .symlink(parent, name.as_bytes(), &target, 1, 2)
                    .expect("model symlink");
                model.nodes.insert(
                    path,
                    ModelNode {
                        ino,
                        kind: ModelKind::Link,
                        target,
                    },
                );
            }
            3 => {
                // hard link to a file or symlink
                if files.is_empty() {
                    continue;
                }
                let source = files[(rng.next() as usize) % files.len()].clone();
                let dir = dirs[(rng.next() as usize) % dirs.len()].clone();
                let name = format!("h{sequence}");
                let path = path_join(&dir, name.as_bytes());
                if model.nodes.contains_key(&path) {
                    continue;
                }
                let parent = model.nodes.get(&dir).expect("dir").ino;
                let source_ino = model.nodes.get(&source).expect("source").ino;
                ufs.link(parent, name.as_bytes(), source_ino)
                    .expect("model link");
                let kind = model.nodes.get(&source).expect("source").kind;
                let target = model.nodes.get(&source).expect("source").target.clone();
                model.nodes.insert(
                    path,
                    ModelNode {
                        ino: source_ino,
                        kind,
                        target,
                    },
                );
            }
            4 => {
                // unlink a file or symlink
                if files.is_empty() {
                    continue;
                }
                let path = files[(rng.next() as usize) % files.len()].clone();
                let (parent, name) = path_split(&path);
                let parent_ino = model.nodes.get(parent).expect("parent").ino;
                ufs.unlink(parent_ino, name).expect("model unlink");
                model.nodes.remove(&path);
            }
            5 => {
                // rmdir an empty directory
                let candidates: Vec<Vec<u8>> = dirs
                    .iter()
                    .filter(|dir| !dir.is_empty())
                    .filter(|dir| model_children(&model, dir).is_empty())
                    .cloned()
                    .collect();
                if candidates.is_empty() {
                    continue;
                }
                let path = candidates[(rng.next() as usize) % candidates.len()].clone();
                let (parent, name) = path_split(&path);
                let parent_ino = model.nodes.get(parent).expect("parent").ino;
                ufs.rmdir(parent_ino, name).expect("model rmdir");
                model.nodes.remove(&path);
            }
            6..=8 => {
                // rename, replacing an empty directory or a non-directory
                let candidates: Vec<Vec<u8>> = model
                    .nodes
                    .keys()
                    .filter(|path| !path.is_empty())
                    .cloned()
                    .collect();
                if candidates.is_empty() {
                    continue;
                }
                let source = candidates[(rng.next() as usize) % candidates.len()].clone();
                let target_dir = dirs[(rng.next() as usize) % dirs.len()].clone();
                let new_name = format!("r{sequence}");
                let destination = path_join(&target_dir, new_name.as_bytes());
                if destination == source {
                    continue;
                }
                let source_node = model.nodes.get(&source).expect("source").clone();
                if source_node.kind == ModelKind::Dir
                    && (destination == source
                        || destination.starts_with(&{
                            let mut prefix = source.clone();
                            prefix.push(b'/');
                            prefix
                        }))
                {
                    continue;
                }
                if let Some(target) = model.nodes.get(&destination) {
                    if target.ino == source_node.ino {
                        continue;
                    }
                    if (source_node.kind == ModelKind::Dir) != (target.kind == ModelKind::Dir) {
                        continue;
                    }
                    if target.kind == ModelKind::Dir
                        && !model_children(&model, &destination).is_empty()
                    {
                        continue;
                    }
                    model.nodes.remove(&destination);
                }
                let (source_parent, source_name) = path_split(&source);
                let source_parent_ino = model.nodes.get(source_parent).expect("parent").ino;
                let target_parent_ino = model.nodes.get(&target_dir).expect("dir").ino;
                ufs.rename(
                    source_parent_ino,
                    source_name,
                    target_parent_ino,
                    new_name.as_bytes(),
                )
                .expect("model rename");
                if source_node.kind == ModelKind::Dir {
                    let mut prefix = source.clone();
                    prefix.push(b'/');
                    let moves: Vec<(Vec<u8>, ModelNode)> = model
                        .nodes
                        .iter()
                        .filter(|(path, _)| {
                            path.as_slice() == source.as_slice() || path.starts_with(&prefix)
                        })
                        .map(|(path, node)| (path.clone(), node.clone()))
                        .collect();
                    for (path, _) in &moves {
                        model.nodes.remove(path);
                    }
                    let mut new_prefix = destination.clone();
                    new_prefix.push(b'/');
                    for (path, node) in moves {
                        let moved = if path.as_slice() == source.as_slice() {
                            destination.clone()
                        } else {
                            let suffix = &path[source.len() + 1..];
                            let mut moved = new_prefix.clone();
                            moved.extend_from_slice(suffix);
                            moved
                        };
                        model.nodes.insert(moved, node);
                    }
                } else {
                    model.nodes.remove(&source);
                    model.nodes.insert(destination, source_node);
                }
            }
            _ => unreachable!(),
        }
        if step % 500 == 499 {
            verify_model(&mut ufs, root, &model, &format!("step {step}"));
            ufs.verify_allocation().expect("periodic verify");
        }
    }
    verify_model(&mut ufs, root, &model, "final");
    ufs.verify_allocation().expect("final verify");
}

struct CrashDisk {
    data: Vec<u8>,
    budget: Rc<Cell<u64>>,
    writes: Rc<Cell<u64>>,
}

impl BlockIo for CrashDisk {
    fn size_bytes(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let src = self.data.get(start..end).ok_or(IoError)?;
        buf.copy_from_slice(src);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        if self.writes.get() >= self.budget.get() {
            return Err(IoError);
        }
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let dst = self.data.get_mut(start..end).ok_or(IoError)?;
        dst.copy_from_slice(buf);
        self.writes.set(self.writes.get() + 1);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

fn crash_sequence(ufs: &mut Ufs<CrashDisk>) -> Result<(), UfsError> {
    let root = ufs.root();
    let dir = ufs.mkdir(root, b"k59-crash-dir", 0o755, 0, 0)?;
    let file = ufs.create(dir, b"file", 0o644, 0, 0)?;
    ufs.write(file, 0, &pattern(300))?;
    ufs.link(dir, b"hard", file)?;
    ufs.symlink(dir, b"sym", b"target", 0, 0)?;
    ufs.mkdir(dir, b"sub", 0o755, 0, 0)?;
    ufs.rename(dir, b"file", root, b"k59-crash-file")?;
    ufs.unlink(root, b"k59-crash-file")?;
    ufs.unlink(dir, b"hard")?;
    ufs.unlink(dir, b"sym")?;
    ufs.rmdir(dir, b"sub")?;
    ufs.rmdir(root, b"k59-crash-dir")?;
    Ok(())
}

#[test]
fn crash_simulation_records_each_write_cut() {
    let path = vectors_dir().join("golden").join("ufs2-b4096.img.gz");
    let image = load_gz(&path);
    let total_writes = {
        let budget = Rc::new(Cell::new(u64::MAX));
        let writes = Rc::new(Cell::new(0u64));
        let disk = CrashDisk {
            data: image.clone(),
            budget,
            writes: writes.clone(),
        };
        let mut ufs = Ufs::mount(disk, MountOptions { read_only: false }).expect("probe mount");
        crash_sequence(&mut ufs).expect("probe sequence");
        writes.get()
    };
    let mut lines = Vec::new();
    for cut in 0..=total_writes {
        let budget = Rc::new(Cell::new(u64::MAX));
        let writes = Rc::new(Cell::new(0u64));
        let disk = CrashDisk {
            data: image.clone(),
            budget: budget.clone(),
            writes: writes.clone(),
        };
        let mut ufs = Ufs::mount(disk, MountOptions { read_only: false }).expect("mount");
        budget.set(cut);
        let operation = crash_sequence(&mut ufs);
        let disk = ufs.into_inner();
        let image_data = disk.data.clone();
        let oracle_status = {
            let oracle = std::path::Path::new("/tmp/opencode/ufs-oracle/bin/fsck_ffs");
            if oracle.exists() {
                let temp = std::env::temp_dir().join(format!("k59-cut-{cut}.img"));
                std::fs::write(&temp, &image_data).expect("write crash image");
                let output = std::process::Command::new(oracle)
                    .arg("-n")
                    .arg(&temp)
                    .output()
                    .expect("run fsck_ffs");
                let _ = std::fs::remove_file(&temp);
                if output.status.success() {
                    "fsck-ok".to_string()
                } else {
                    format!("fsck-exit-{:?}", output.status.code())
                }
            } else {
                "oracle-absent".to_string()
            }
        };
        let mut remounted = Ufs::mount(VecDisk::new(disk.data), MountOptions { read_only: false })
            .unwrap_or_else(|error| panic!("cut {cut}: remount failed: {error:?}"));
        let walk = remounted
            .readdir(remounted.root(), 0, &mut |_, _| true)
            .map(|()| "walk-ok".to_string())
            .unwrap_or_else(|error| format!("walk-{error:?}"));
        let verify = remounted
            .verify_allocation()
            .map(|()| "verify-ok".to_string())
            .unwrap_or_else(|error| format!("verify-{error}"));
        lines.push(format!(
            "{cut}\t{}\t{walk}\t{verify}\t{oracle_status}",
            match operation {
                Ok(()) => "completed".to_string(),
                Err(error) => format!("{error:?}"),
            }
        ));
    }
    for line in &lines {
        println!("{line}");
    }
    assert!(lines.iter().any(|line| line.contains("\tverify-ok\t")));
}
