// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Allocator model tests over the B63 golden images.

mod common;

use common::{VecDisk, load_gz, vectors_dir};
use ufs_core::{MountOptions, Ufs};

fn mount_rw(name: &str) -> Ufs<VecDisk> {
    let path = vectors_dir().join("golden").join(format!("{name}.img.gz"));
    let data = load_gz(&path);
    Ufs::mount(VecDisk::new(data), MountOptions { read_only: false }).expect("writable mount")
}

#[test]
fn writable_mount_verifies_ufs2() {
    let mut ufs = mount_rw("ufs2-b4096");
    ufs.verify_allocation().expect("initial allocation state");
}

#[test]
fn writable_mount_verifies_ufs1() {
    let mut ufs = mount_rw("ufs1-b4096");
    ufs.verify_allocation().expect("initial allocation state");
}

#[test]
fn alloc_and_free_block_ufs2() {
    let mut ufs = mount_rw("ufs2-b4096");
    let start = ufs.statfs().blocks_free;
    let address = ufs.alloc_block(2, 0, None, 4096).expect("alloc block");
    assert_ne!(address, 0);
    ufs.verify_allocation().expect("after allocation");
    ufs.free_frags(address, 4096).expect("free block");
    ufs.verify_allocation().expect("after free");
    assert_eq!(ufs.statfs().blocks_free, start);
    let disk = ufs.unmount().expect("unmount");
    let read_only = Ufs::mount(disk, MountOptions { read_only: true }).expect("remount");
    assert!(read_only.is_clean());
    assert_eq!(read_only.statfs().blocks_free, start);
}

#[test]
fn alloc_and_free_fragment_ufs1() {
    let mut ufs = mount_rw("ufs1-b4096");
    let start = ufs.statfs().blocks_free;
    let address = ufs.alloc_block(2, 0, None, 1024).expect("alloc fragment");
    ufs.verify_allocation().expect("after allocation");
    ufs.free_frags(address, 1024).expect("free fragment");
    ufs.verify_allocation().expect("after free");
    assert_eq!(ufs.statfs().blocks_free, start);
}

#[test]
fn realloc_extends_or_moves() {
    let mut ufs = mount_rw("ufs2-b4096");
    let address = ufs.alloc_block(2, 0, None, 1024).expect("alloc fragment");
    let extended = ufs
        .realloc_frags(2, 0, address, 1024, 3072)
        .expect("realloc");
    ufs.verify_allocation().expect("after realloc");
    ufs.free_frags(extended, 3072).expect("free");
    ufs.verify_allocation().expect("after free");
}

#[test]
fn double_free_is_corrupt() {
    let mut ufs = mount_rw("ufs2-b4096");
    let address = ufs.alloc_block(2, 0, None, 4096).expect("alloc");
    ufs.free_frags(address, 4096).expect("free");
    assert_eq!(
        ufs.free_frags(address, 4096),
        Err(ufs_core::UfsError::Corrupt("double free of block"))
    );
}

#[test]
fn out_of_range_free_is_corrupt() {
    let mut ufs = mount_rw("ufs2-b4096");
    assert!(matches!(
        ufs.free_frags(1 << 40, 4096),
        Err(ufs_core::UfsError::Corrupt(_))
    ));
}

#[test]
fn inode_alloc_and_free_ufs2() {
    let mut ufs = mount_rw("ufs2-b4096");
    let start = ufs.statfs().files_free;
    let ino = ufs.alloc_inode(2, 0o100644).expect("alloc inode");
    assert!(ino >= 2);
    let attr = ufs.getattr(ino).expect("getattr new inode");
    assert_eq!(attr.mode, 0o644);
    assert_eq!(attr.nlink, 0);
    ufs.verify_allocation().expect("after inode allocation");
    ufs.free_inode(ino, 0o100644).expect("free inode");
    ufs.verify_allocation().expect("after inode free");
    assert_eq!(ufs.statfs().files_free, start);
}

#[test]
fn double_inode_free_is_corrupt() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = ufs.alloc_inode(2, 0o100644).expect("alloc inode");
    ufs.free_inode(ino, 0o100644).expect("free inode");
    assert_eq!(
        ufs.free_inode(ino, 0o100644),
        Err(ufs_core::UfsError::Corrupt("double free of inode"))
    );
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

    fn range(&mut self, limit: usize) -> usize {
        (self.next() % limit as u64) as usize
    }
}

#[derive(Clone, Copy)]
struct Range {
    addr: u64,
    size: u32,
}

fn overlaps(a: Range, b: Range, fsize: u32) -> bool {
    let a0 = a.addr;
    let a1 = a.addr + u64::from(a.size / fsize);
    let b0 = b.addr;
    let b1 = b.addr + u64::from(b.size / fsize);
    a0 < b1 && b0 < a1
}

fn model_test(name: &str, seed: u64) {
    let mut ufs = mount_rw(name);
    let fsize = ufs.statfs().fragment_size;
    let bsize = ufs.statfs().block_size;
    let start_free = ufs.statfs().blocks_free;
    let sizes = [fsize, 2 * fsize, 3 * fsize, bsize];
    let mut rng = Rng(seed);
    let mut active: Vec<Range> = Vec::new();

    for op in 0..20_000usize {
        match rng.range(10) {
            0..=5 => {
                let size = sizes[rng.range(sizes.len())];
                match ufs.alloc_block(2, op as u64, None, size) {
                    Ok(addr) => {
                        let candidate = Range { addr, size };
                        assert!(
                            !active
                                .iter()
                                .any(|existing| overlaps(candidate, *existing, fsize)),
                            "{name}: allocation {addr} overlaps an active range"
                        );
                        active.push(candidate);
                    }
                    Err(ufs_core::UfsError::NoSpace) => {}
                    Err(error) => panic!("{name}: alloc failed: {error:?}"),
                }
            }
            6..=8 if !active.is_empty() => {
                let index = rng.range(active.len());
                let range = active.swap_remove(index);
                ufs.free_frags(range.addr, range.size)
                    .unwrap_or_else(|error| panic!("{name}: free failed: {error:?}"));
            }
            9 if !active.is_empty() => {
                let index = rng.range(active.len());
                let old = active[index];
                let new_size = sizes[rng.range(sizes.len())];
                if new_size == old.size {
                    continue;
                }
                let new_addr =
                    match ufs.realloc_frags(2, index as u64, old.addr, old.size, new_size) {
                        Ok(addr) => addr,
                        Err(ufs_core::UfsError::NoSpace) => continue,
                        Err(error) => panic!("{name}: realloc failed: {error:?}"),
                    };
                let candidate = Range {
                    addr: new_addr,
                    size: new_size,
                };
                let others: Vec<Range> = active
                    .iter()
                    .enumerate()
                    .filter(|(other, _)| *other != index)
                    .map(|(_, range)| *range)
                    .collect();
                assert!(
                    !others
                        .iter()
                        .any(|existing| overlaps(candidate, *existing, fsize)),
                    "{name}: realloc target {new_addr} overlaps"
                );
                active[index] = candidate;
            }
            _ => {}
        }
        if op % 500 == 499 {
            ufs.verify_allocation()
                .unwrap_or_else(|error| panic!("{name}: verify at op {op}: {error}"));
        }
    }

    for range in active {
        ufs.free_frags(range.addr, range.size)
            .unwrap_or_else(|error| panic!("{name}: final free failed: {error:?}"));
    }
    ufs.verify_allocation().expect("final allocation state");
    assert_eq!(ufs.statfs().blocks_free, start_free);
    let disk = ufs.unmount().expect("unmount model image");
    let read_only = Ufs::mount(disk, MountOptions { read_only: true }).expect("remount");
    assert!(read_only.is_clean());
    assert_eq!(read_only.statfs().blocks_free, start_free);
}

#[test]
fn model_20k_ufs1_b4096() {
    model_test("ufs1-b4096", 0x1234_5678_9abc_def1);
}

#[test]
fn model_20k_ufs1_b8192() {
    model_test("ufs1-b8192", 0x1234_5678_9abc_def2);
}

#[test]
fn model_20k_ufs2_b4096() {
    model_test("ufs2-b4096", 0x1234_5678_9abc_def3);
}

#[test]
fn model_20k_ufs2_b8192() {
    model_test("ufs2-b8192", 0x1234_5678_9abc_def4);
}

fn fill_and_empty(name: &str) {
    let mut ufs = mount_rw(name);
    let fsize = ufs.statfs().fragment_size;
    let bsize = ufs.statfs().block_size;
    let start_free = ufs.statfs().blocks_free;
    let mut allocated = Vec::new();
    for _ in 0..100_000 {
        match ufs.alloc_block(2, 0, None, bsize) {
            Ok(addr) => allocated.push(addr),
            Err(ufs_core::UfsError::NoSpace) => break,
            Err(error) => panic!("{name}: fill failed: {error:?}"),
        }
    }
    assert!(
        !allocated.is_empty(),
        "{name}: no blocks allocated before NoSpace"
    );
    ufs.verify_allocation().expect("filled allocation state");
    for addr in allocated {
        ufs.free_frags(addr, bsize).expect("free filled block");
    }
    ufs.verify_allocation().expect("emptied allocation state");
    assert_eq!(ufs.statfs().blocks_free, start_free);
    let _ = fsize;
}

#[test]
fn fill_to_nospace_and_free_all_ufs2() {
    fill_and_empty("ufs2-b4096");
}

#[test]
fn fill_to_nospace_and_free_all_ufs1() {
    fill_and_empty("ufs1-b8192");
}

#[test]
fn drop_without_unmount_leaves_dirty() {
    let ufs = mount_rw("ufs2-b4096");
    let disk = ufs.into_inner();
    let read_only = Ufs::mount(disk, MountOptions { read_only: true }).expect("remount");
    assert!(!read_only.is_clean());
}

#[test]
fn explicit_sync_marks_clean() {
    let mut ufs = mount_rw("ufs2-b4096");
    ufs.sync().expect("sync");
    let disk = ufs.into_inner();
    let read_only = Ufs::mount(disk, MountOptions { read_only: true }).expect("remount");
    assert!(read_only.is_clean());
}

#[test]
fn read_only_allocator_is_refused() {
    let path = vectors_dir().join("golden/ufs2-b4096.img.gz");
    let data = load_gz(&path);
    let mut ufs = Ufs::mount(VecDisk::new(data), MountOptions { read_only: true }).expect("ro");
    assert_eq!(
        ufs.alloc_block(2, 0, None, 4096),
        Err(ufs_core::UfsError::ReadOnly)
    );
    assert_eq!(
        ufs.free_frags(4096, 4096),
        Err(ufs_core::UfsError::ReadOnly)
    );
}

#[test]
fn invalid_allocation_sizes_are_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    for size in [0u32, 500, 4097, 8192] {
        assert_eq!(
            ufs.alloc_block(2, 0, None, size),
            Err(ufs_core::UfsError::Invalid),
            "size {size}"
        );
    }
}

#[test]
fn realloc_shrink_keeps_address() {
    let mut ufs = mount_rw("ufs2-b4096");
    let address = ufs.alloc_block(2, 0, None, 4096).expect("alloc");
    let shrunk = ufs
        .realloc_frags(2, 0, address, 4096, 1024)
        .expect("shrink");
    assert_eq!(shrunk, address);
    ufs.verify_allocation().expect("after shrink");
    ufs.free_frags(shrunk, 1024).expect("free");
    ufs.verify_allocation().expect("after free");
}

#[test]
fn blkpref_prefers_contiguous_and_group() {
    let mut ufs = mount_rw("ufs1-b4096");
    let first = ufs.blkpref(2, 0, 0, &[]);
    assert!(first > 0);
    assert_eq!(ufs.blkpref(2, 1, 1, &[1000]), 1008);
}

#[test]
fn lazy_inode_initialization_ufs2_b8192() {
    let mut ufs = mount_rw("ufs2-b8192");
    let start = ufs.statfs().files_free;
    let mut inodes = Vec::new();
    for _ in 0..300 {
        match ufs.alloc_inode(2, 0o100644) {
            Ok(ino) => inodes.push(ino),
            Err(ufs_core::UfsError::NoSpace) => break,
            Err(error) => panic!("alloc inode: {error:?}"),
        }
    }
    assert!(!inodes.is_empty(), "no inode allocated");
    ufs.verify_allocation()
        .expect("after lazy inode allocation");
    for ino in &inodes {
        assert_eq!(ufs.getattr(*ino).expect("new inode").mode, 0o644);
    }
    for ino in inodes {
        ufs.free_inode(ino, 0o100644).expect("free inode");
    }
    ufs.verify_allocation().expect("after lazy inode free");
    assert_eq!(ufs.statfs().files_free, start);
}

#[test]
fn directory_inodes_are_directories() {
    let mut ufs = mount_rw("ufs2-b4096");
    let mut inodes = Vec::new();
    for _ in 0..4 {
        let ino = ufs.alloc_inode(2, 0o040755).expect("alloc directory");
        assert_eq!(
            ufs.getattr(ino).expect("attr").kind,
            ufs_core::FileType::Directory
        );
        inodes.push(ino);
    }
    ufs.verify_allocation().expect("after directory allocation");
    for ino in inodes {
        ufs.free_inode(ino, 0o040755).expect("free directory");
    }
    ufs.verify_allocation().expect("after directory free");
}

#[test]
fn oracle_fsck_if_present() {
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    let dir = env::var("UFS_ORACLE_DIR").unwrap_or_else(|_| "/tmp/opencode/ufs-oracle".to_string());
    let fsck = Path::new(&dir).join("bin/fsck_ffs");
    if !fsck.is_file() {
        println!("ORACLE ABSENT");
        return;
    }
    let mut ufs = mount_rw("ufs2-b4096");
    let address = ufs.alloc_block(2, 0, None, 4096).expect("alloc");
    ufs.free_frags(address, 4096).expect("free");
    let disk = ufs.unmount().expect("unmount");
    let image = Path::new(&dir).join("k52-oracle.img");
    fs::write(&image, &disk.data).expect("write image");
    let output = Command::new(&fsck)
        .args(["-n", "-F"])
        .arg(&image)
        .output()
        .expect("run fsck_ffs");
    let _ = fs::remove_file(&image);
    assert!(
        output.status.success(),
        "fsck_ffs failed: {:?}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}
