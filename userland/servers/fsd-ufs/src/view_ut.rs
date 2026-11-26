// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `UfsView` tests over a freshly formatted in-memory UFS2 image.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use raven_proto_fs::{DirEntryIter, S_IFCHR, S_IFIFO, S_IFLNK, S_IFMT, S_IFREG};
use ufs_core::check::{Mode, check};
use ufs_core::mkfs::{MkfsOptions, format};
use ufs_core::{BlockIo, IoError};

use super::*;
use crate::handler::FileSystem;

struct VecDisk {
    data: Vec<u8>,
}

impl VecDisk {
    fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl BlockIo for VecDisk {
    fn size_bytes(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), IoError> {
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buffer.len()).ok_or(IoError)?;
        let source = self.data.get(start..end).ok_or(IoError)?;
        buffer.copy_from_slice(source);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, buffer: &[u8]) -> Result<(), IoError> {
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buffer.len()).ok_or(IoError)?;
        let target = self.data.get_mut(start..end).ok_or(IoError)?;
        target.copy_from_slice(buffer);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

fn options() -> MkfsOptions<'static> {
    MkfsOptions {
        size: 8 * 1024 * 1024,
        version: 2,
        block_size: 4096,
        fragment_size: 512,
        bytes_per_inode: 4096,
        minfree: 5,
        label: "fsd-ufs",
        timestamp: 1_767_225_600,
    }
}

fn view() -> UfsView<VecDisk> {
    let disk = VecDisk::new(vec![0u8; options().size as usize]);
    let mut ufs = format(disk, &options()).expect("format");
    let root = ufs.root();
    let hello = ufs
        .create(root, b"hello.txt", 0o644, 1000, 100)
        .expect("create");
    ufs.write(hello, 0, b"hello\n").expect("write");
    ufs.create(root, b"empty", 0o600, 1000, 100)
        .expect("create");
    let dir = ufs.mkdir(root, b"dir1", 0o755, 1000, 100).expect("mkdir");
    let one = ufs
        .create(dir, b"one.txt", 0o644, 1000, 100)
        .expect("create");
    ufs.write(one, 0, b"one\n").expect("write");
    ufs.symlink(root, b"link", b"dir1/one.txt", 1000, 100)
        .expect("symlink");
    ufs.symlink(root, b"dangling", b"missing", 1000, 100)
        .expect("symlink");
    ufs.mkdir(root, b"emptydir", 0o755, 1000, 100)
        .expect("mkdir");
    let disk = ufs.unmount().expect("unmount");
    UfsView::mount(disk, true).expect("mount")
}

fn rw_view() -> UfsView<VecDisk> {
    let disk = VecDisk::new(vec![0u8; options().size as usize]);
    let ufs = format(disk, &options()).expect("format");
    let disk = ufs.unmount().expect("unmount");
    UfsView::mount(disk, false).expect("mount rw")
}

fn names(buffer: &[u8]) -> Vec<String> {
    DirEntryIter::new(buffer)
        .map(|entry| {
            let entry = entry.expect("entry");
            core::str::from_utf8(entry.name).expect("utf8").to_string()
        })
        .collect()
}

#[test]
fn resolves_files_and_symlinks() {
    let mut view = view();
    let hello = view.resolve("/hello.txt").expect("hello");
    let one = view.resolve("/dir1/one.txt").expect("one");
    let link = view.resolve("/link").expect("link");
    assert_eq!(link, one);
    assert_ne!(view.resolve_nofollow("/link").expect("link inode"), one);
    assert_eq!(view.resolve("/dir1/../hello.txt"), Ok(hello));
    assert!(view.resolve("/").is_ok());
    assert_eq!(view.resolve("/dangling"), Err(Errno::ENOENT));
    assert_eq!(view.resolve("/../hello.txt"), Err(Errno::EINVAL));
    assert_eq!(view.resolve("/missing"), Err(Errno::ENOENT));
    assert_eq!(view.resolve("/hello.txt/child"), Err(Errno::ENOTDIR));
}

#[test]
fn reads_file_data_at_explicit_offsets() {
    let mut view = view();
    let ino = view.resolve("/hello.txt").expect("hello");
    let stat = view.stat(ino).expect("stat");
    assert_eq!(stat.mode & S_IFMT, S_IFREG);
    assert_eq!(stat.size, 6);
    let mut buffer = [0u8; 8];
    assert_eq!(view.read(ino, 0, &mut buffer), Ok(6));
    assert_eq!(&buffer[..6], b"hello\n");
    assert_eq!(view.read(ino, 1, &mut buffer[..3]), Ok(3));
    assert_eq!(&buffer[..3], b"ell");
    assert_eq!(view.read(ino, 6, &mut buffer), Ok(0));
    assert_eq!(view.read(ino, 3, &mut buffer[..0]), Ok(0));
}

#[test]
fn directory_cookie_walks_every_entry() {
    let mut view = view();
    let root = view.resolve("/").expect("root");
    let mut cookie = 0u64;
    let mut all = Vec::new();
    loop {
        let mut buffer = [0u8; 4096];
        let (read, next) = view.readdir(root, cookie, &mut buffer).expect("readdir");
        if read == 0 {
            break;
        }
        assert_ne!(next, cookie);
        cookie = next;
        all.extend(names(&buffer[..read]));
    }
    assert!(all.contains(&"hello.txt".to_string()));
    assert!(all.contains(&"dir1".to_string()));
    assert!(all.contains(&"emptydir".to_string()));
    assert!(all.contains(&"link".to_string()));
    assert!(!all.contains(&"missing".to_string()));

    let mut tiny = [0u8; 8];
    assert_eq!(view.readdir(root, 0, &mut tiny), Err(Errno::EINVAL));
}

#[test]
fn readlink_and_stat_keep_the_link_identity() {
    let mut view = view();
    let link = view.resolve_nofollow("/link").expect("link inode");
    let stat = view.stat(link).expect("stat");
    assert_eq!(stat.mode & S_IFMT, S_IFLNK);
    let mut target = [0u8; 64];
    let length = view.readlink(link, &mut target).expect("readlink");
    assert_eq!(&target[..length], b"dir1/one.txt");

    let hello = view.resolve("/hello.txt").expect("hello");
    assert_eq!(view.readlink(hello, &mut target), Err(Errno::EINVAL));
    let hello = view.stat(hello).expect("stat");
    assert_eq!(hello.uid, 1000);
    assert_eq!(hello.gid, 100);
}

#[test]
fn statfs_reports_the_formatted_geometry() {
    let view = view();
    let stat = view.statfs();
    assert_eq!(stat.block_size, 512);
    assert!(stat.blocks > 0);
    let capacity = u64::from(stat.block_size) * stat.blocks;
    let size = options().size;
    assert!(
        capacity >= size / 10 * 9 && capacity <= size,
        "capacity={capacity} size={size}"
    );
    assert!(stat.files > 0);
    assert!(stat.free_blocks <= stat.blocks);
    assert!(stat.free_files <= stat.files);
}

#[test]
fn a_write_cycle_remounts_clean() {
    let disk = VecDisk::new(vec![0u8; options().size as usize]);
    let mut ufs = format(disk, &options()).expect("format");
    let root = ufs.root();
    let file = ufs.create(root, b"a.txt", 0o644, 0, 0).expect("create");
    ufs.write(file, 0, b"payload\n").expect("write");
    let dir = ufs.mkdir(root, b"dir", 0o755, 0, 0).expect("mkdir");
    ufs.rename(root, b"a.txt", dir, b"b.txt").expect("rename");
    ufs.symlink(dir, b"link", b"b.txt", 0, 0).expect("symlink");
    ufs.unlink(dir, b"b.txt").expect("unlink");
    ufs.unlink(dir, b"link").expect("unlink link");
    ufs.rmdir(root, b"dir").expect("rmdir");
    let disk = ufs.unmount().expect("unmount");
    let mut disk = disk;
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(report.clean_after, "{:?}", report.findings);
}

#[test]
fn mount_rejects_a_non_ufs_image() {
    let disk = VecDisk::new(vec![0u8; 8192]);
    assert!(UfsView::mount(disk, true).is_err());
}

#[test]
fn mount_rejects_a_corrupt_superblock() {
    let disk = VecDisk::new(vec![0u8; options().size as usize]);
    let ufs = format(disk, &options()).expect("format");
    let mut disk = ufs.unmount().expect("unmount");
    // The primary UFS2 superblock is at 8192; break its magic.
    disk.data[8192 + 1372] ^= 0xff;
    assert!(UfsView::mount(disk, true).is_err());
}

#[test]
fn hard_links_share_the_inode_and_link_count() {
    let mut view = rw_view();
    let ino = view.create_file("/a", 0o644, 0, 0, true).expect("create");
    view.write(ino, 0, b"payload").expect("write");
    view.link("/b", "/a").expect("link");
    assert_eq!(view.resolve("/b"), Ok(ino));
    assert_eq!(view.stat(ino).expect("stat").nlink, 2);
    view.unlink("/a").expect("unlink a");
    assert_eq!(view.stat(ino).expect("stat").nlink, 1);
    let mut data = [0u8; 7];
    assert_eq!(view.read(ino, 0, &mut data), Ok(7));
    assert_eq!(&data, b"payload");
    assert_eq!(view.link("/c", "/"), Err(Errno::EPERM));
}

#[test]
fn create_records_the_requested_owner() {
    let mut view = rw_view();
    let ino = view
        .create_file("/owned-file", 0o640, 1000, 100, true)
        .expect("create");
    let stat = view.stat(ino).expect("stat");
    assert_eq!((stat.uid, stat.gid), (1000, 100));
    assert_eq!(stat.mode & 0o7777, 0o640);
}

#[test]
fn chown_follows_or_keeps_the_symlink_identity() {
    let mut view = rw_view();
    let ino = view
        .create_file("/owned", 0o6755, 0, 0, true)
        .expect("create");
    view.chown("/owned", 1000, 100, true).expect("chown");
    let stat = view.stat(ino).expect("stat");
    assert_eq!((stat.uid, stat.gid), (1000, 100));
    assert_eq!(stat.mode & 0o7777, 0o755);
    view.symlink("/link", b"owned", 0, 0).expect("symlink");
    let link = view.resolve_nofollow("/link").expect("link");
    view.chown("/link", 2000, 200, false).expect("lchown");
    assert_eq!(view.stat(link).expect("link stat").uid, 2000);
    assert_eq!(view.stat(ino).expect("target stat").uid, 1000);
    view.fchown(ino, u32::MAX, 300).expect("fchown");
    assert_eq!(view.stat(ino).expect("stat").gid, 300);
}

#[test]
fn utimens_sets_the_requested_timestamps() {
    let mut view = rw_view();
    let ino = view
        .create_file("/times", 0o644, 0, 0, true)
        .expect("create");
    view.utimens("/times", Some(111), Some(222), true)
        .expect("utimens");
    let stat = view.stat(ino).expect("stat");
    assert_eq!((stat.atime, stat.mtime), (111, 222));
    view.utimens("/times", None, Some(333), true)
        .expect("utimens omit atime");
    let stat = view.stat(ino).expect("stat");
    assert_eq!((stat.atime, stat.mtime), (111, 333));
}

#[test]
fn sparse_extension_keeps_block_accounting_small() {
    let mut view = rw_view();
    let ino = view
        .create_file("/sparse", 0o644, 0, 0, true)
        .expect("create");
    view.truncate(ino, 1024 * 1024).expect("truncate");
    let stat = view.stat(ino).expect("stat");
    assert_eq!(stat.size, 1024 * 1024);
    assert!(
        stat.blocks * 512 < stat.size / 4,
        "sparse blocks={} size={}",
        stat.blocks,
        stat.size
    );
    let mut buffer = [0xffu8; 4096];
    assert_eq!(view.read(ino, 512 * 1024, &mut buffer), Ok(4096));
    assert!(buffer.iter().all(|byte| *byte == 0));
    assert_eq!(view.write(ino, 512 * 1024, b"data"), Ok(4));
    let stat = view.stat(ino).expect("stat after write");
    assert!(stat.blocks * 512 < stat.size / 4);
}

#[test]
fn mkfifo_and_mknod_create_special_inodes() {
    let mut view = rw_view();
    view.mkfifo("/pipe", 0o640, 0, 0).expect("mkfifo");
    let pipe = view.resolve("/pipe").expect("pipe");
    assert_eq!(view.stat(pipe).expect("stat").mode & S_IFMT, S_IFIFO);
    view.mknod("/null", 0o020600, 0x1234, 0, 0).expect("mknod");
    let node = view.resolve("/null").expect("node");
    assert_eq!(view.stat(node).expect("stat").mode & S_IFMT, S_IFCHR);
    assert_eq!(view.mknod("/bad", 0o100644, 0, 0, 0), Err(Errno::EINVAL));
}
