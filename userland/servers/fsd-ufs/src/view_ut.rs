// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `UfsView` tests over a freshly formatted in-memory UFS2 image.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use raven_proto_fs::{DirEntryIter, S_IFLNK, S_IFMT, S_IFREG};
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
    assert_eq!(stat.block_size, 4096);
    assert!(stat.blocks > 0);
    assert!(stat.files > 0);
    assert!(stat.free_blocks <= stat.blocks);
    assert!(stat.free_files <= stat.files);
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
