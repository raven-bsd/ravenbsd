// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! File data-path tests: write, truncate, setattr and symlinks.

mod common;

use common::{VecDisk, load_gz, read_all, vectors_dir};
use ufs_core::{FileType, Ino, MountOptions, SetAttr, Timespec, Ufs, UfsError};

fn mount_rw(name: &str) -> Ufs<VecDisk> {
    let path = vectors_dir().join("golden").join(format!("{name}.img.gz"));
    let data = load_gz(&path);
    Ufs::mount(VecDisk::new(data), MountOptions { read_only: false }).expect("writable mount")
}

fn new_file(ufs: &mut Ufs<VecDisk>) -> Ino {
    ufs.alloc_inode(2, 0o100644).expect("alloc file inode")
}

fn pattern(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| (index.wrapping_mul(37) ^ 0x5a) as u8)
        .collect()
}

#[test]
fn write_roundtrip_sizes() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as u64;
    let sizes = [
        1usize,
        511,
        512,
        4095,
        4096,
        4097,
        (12 * bsize) as usize - 1,
        (12 * bsize) as usize,
        (12 * bsize) as usize + 1,
    ];
    for size in sizes {
        let data = pattern(size);
        ufs.truncate(ino, 0).expect("reset");
        assert_eq!(ufs.write(ino, 0, &data).expect("write"), size);
        assert_eq!(read_all(&mut ufs, ino, size as u64).expect("read"), data);
        ufs.verify_allocation().expect("after roundtrip");
    }
}

#[test]
fn write_indirect_and_double_indirect() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    let sizes = [
        600 * bsize - 1,
        600 * bsize,
        600 * bsize + 1,
        3 * 1024 * 1024,
    ];
    for size in sizes {
        let data = pattern(size);
        ufs.truncate(ino, 0).expect("reset");
        ufs.write(ino, 0, &data).expect("write large");
        assert_eq!(read_all(&mut ufs, ino, size as u64).expect("read"), data);
        ufs.verify_allocation().expect("after indirect write");
        ufs.truncate(ino, 0).expect("truncate all");
        ufs.verify_allocation().expect("after indirect truncate");
    }
}

#[test]
fn sparse_write_reads_zeros_and_truncate_grows_hole() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as u64;
    let data = b"tail";
    ufs.write(ino, 2 * bsize + 123, data).expect("sparse write");
    assert_eq!(
        ufs.getattr(ino).expect("attr").size,
        2 * bsize + 123 + data.len() as u64
    );
    let all = read_all(&mut ufs, ino, 2 * bsize + 123 + data.len() as u64).expect("read");
    assert!(
        all[..2 * bsize as usize + 123]
            .iter()
            .all(|byte| *byte == 0)
    );
    assert_eq!(&all[2 * bsize as usize + 123..], data);
    ufs.truncate(ino, 4 * bsize).expect("grow hole");
    assert_eq!(ufs.getattr(ino).expect("attr").size, 4 * bsize);
    let hole = read_all(&mut ufs, ino, 4 * bsize).expect("read hole");
    assert!(
        hole[..2 * bsize as usize + 123]
            .iter()
            .all(|byte| *byte == 0)
    );
    assert_eq!(
        &hole[2 * bsize as usize + 123..2 * bsize as usize + 127],
        data
    );
    assert!(
        hole[2 * bsize as usize + 127..]
            .iter()
            .all(|byte| *byte == 0)
    );
    ufs.verify_allocation().expect("after holes");
}

#[test]
fn truncate_shrink_grow_shrink() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as u64;
    let data = pattern((5 * bsize) as usize);
    ufs.write(ino, 0, &data).expect("write");
    let mut model = data.clone();
    for size in [3 * bsize + 77, bsize, 0, 2 * bsize, 7 * bsize, 1] {
        let old_len = model.len();
        ufs.truncate(ino, size).expect("truncate");
        assert_eq!(ufs.getattr(ino).expect("attr").size, size);
        let got = read_all(&mut ufs, ino, size).expect("read truncated");
        if size as usize <= old_len {
            assert_eq!(got.as_slice(), &model[..size as usize]);
            model.truncate(size as usize);
        } else {
            assert_eq!(&got[..old_len], &model[..]);
            assert!(got[old_len..].iter().all(|byte| *byte == 0));
            model = got.clone();
        }
        ufs.verify_allocation().expect("after truncate");
    }
}

#[test]
fn fragment_tail_grows_to_full_block() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    let first = pattern(1);
    ufs.write(ino, 0, &first).expect("one byte");
    assert_eq!(read_all(&mut ufs, ino, 1).expect("read"), first);
    let full = pattern(bsize);
    ufs.write(ino, 0, &full).expect("full block");
    assert_eq!(read_all(&mut ufs, ino, bsize as u64).expect("read"), full);
    ufs.verify_allocation().expect("after fragment conversion");
}

#[test]
fn setattr_updates_metadata_and_size() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    ufs.write(ino, 0, &pattern(1000)).expect("write");
    let atime = Timespec { sec: 1234, nsec: 5 };
    let mtime = Timespec { sec: 5678, nsec: 9 };
    ufs.setattr(
        ino,
        SetAttr {
            mode: Some(0o640),
            uid: Some(1000),
            gid: Some(100),
            atime: Some(atime),
            mtime: Some(mtime),
            size: Some(500),
        },
    )
    .expect("setattr");
    let attr = ufs.getattr(ino).expect("attr");
    assert_eq!(attr.mode, 0o640);
    assert_eq!(attr.uid, 1000);
    assert_eq!(attr.gid, 100);
    assert_eq!(attr.atime, atime);
    assert_eq!(attr.mtime, mtime);
    assert_eq!(attr.size, 500);
    ufs.setattr(
        ino,
        SetAttr {
            size: Some(2000),
            ..SetAttr::default()
        },
    )
    .expect("grow");
    assert_eq!(ufs.getattr(ino).expect("attr").size, 2000);
    ufs.verify_allocation().expect("after setattr");
}

#[test]
fn fast_and_slow_symlink_writes() {
    let mut ufs = mount_rw("ufs2-b4096");
    let fast = ufs.alloc_inode(2, 0o120777).expect("fast symlink inode");
    ufs.write(fast, 0, b"fast-target").expect("fast target");
    let mut buffer = [0u8; 64];
    let n = ufs.readlink(fast, &mut buffer).expect("readlink fast");
    assert_eq!(&buffer[..n], b"fast-target");
    assert_eq!(ufs.getattr(fast).expect("attr").kind, FileType::Symlink);

    let slow = ufs.alloc_inode(2, 0o120777).expect("slow symlink inode");
    let target = pattern(200);
    ufs.write(slow, 0, &target).expect("slow target");
    let mut buffer = vec![0u8; 256];
    let n = ufs.readlink(slow, &mut buffer).expect("readlink slow");
    assert_eq!(&buffer[..n], &target[..]);
    ufs.verify_allocation().expect("after symlinks");
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

fn check_file(ufs: &mut Ufs<VecDisk>, ino: Ino, model: &[u8]) {
    assert_eq!(
        ufs.getattr(ino).expect("attr").size,
        model.len() as u64,
        "size"
    );
    let got = read_all(ufs, ino, model.len() as u64).expect("read model");
    assert_eq!(got.as_slice(), model);
}

fn check_model(ufs: &mut Ufs<VecDisk>, ino: Ino, model: &[u8], context: &str) {
    let attr = ufs.getattr(ino).expect("attr");
    let got = read_all(ufs, ino, model.len() as u64).expect("read model");
    if got.as_slice() != model {
        let at = got
            .iter()
            .zip(model)
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| got.len().min(model.len()));
        let from = at.saturating_sub(4);
        let to = (at + 8).min(got.len()).min(model.len());
        panic!(
            "{context}: ino {ino} size {} model {} mismatch at {at}; got {:?} expected {:?}",
            attr.size,
            model.len(),
            &got[from..to],
            &model[from..to]
        );
    }
}

#[test]
fn write_roundtrip_ufs1_b4096() {
    let mut ufs = mount_rw("ufs1-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    for size in [1usize, 511, 512, 4095, 4096, 4097, 12 * bsize + 1] {
        let data = pattern(size);
        ufs.truncate(ino, 0).expect("reset");
        ufs.write(ino, 0, &data).expect("write");
        check_file(&mut ufs, ino, &data);
        ufs.verify_allocation().expect("verify");
    }
}

#[test]
fn write_roundtrip_ufs1_b8192() {
    let mut ufs = mount_rw("ufs1-b8192");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    for size in [1usize, 511, 512, 8191, 8192, 8193, 12 * bsize + 1] {
        let data = pattern(size);
        ufs.truncate(ino, 0).expect("reset");
        ufs.write(ino, 0, &data).expect("write");
        check_file(&mut ufs, ino, &data);
        ufs.verify_allocation().expect("verify");
    }
}

#[test]
fn write_roundtrip_ufs2_b8192() {
    let mut ufs = mount_rw("ufs2-b8192");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    for size in [1usize, 511, 512, 8191, 8192, 8193, 12 * bsize + 1] {
        let data = pattern(size);
        ufs.truncate(ino, 0).expect("reset");
        ufs.write(ino, 0, &data).expect("write");
        check_file(&mut ufs, ino, &data);
        ufs.verify_allocation().expect("verify");
    }
}

#[test]
fn write_at_offset_keeps_size() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let mut model = pattern(5000);
    ufs.write(ino, 0, &model).expect("write");
    ufs.write(ino, 100, b"HELLO").expect("overwrite");
    model[100..105].copy_from_slice(b"HELLO");
    assert_eq!(ufs.getattr(ino).expect("attr").size, 5000);
    check_file(&mut ufs, ino, &model);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn write_inside_partial_tail_keeps_allocation() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let mut model = pattern(2000);
    ufs.write(ino, 0, &model).expect("write");
    let before = ufs.getattr(ino).expect("attr");
    let free = ufs.statfs().blocks_free;
    ufs.write(ino, 100, b"0123456789").expect("overwrite");
    model[100..110].copy_from_slice(b"0123456789");
    let after = ufs.getattr(ino).expect("attr");
    assert_eq!(after.size, before.size);
    assert_eq!(after.blocks, before.blocks);
    assert_eq!(ufs.statfs().blocks_free, free);
    check_file(&mut ufs, ino, &model);
    ufs.truncate(ino, 500).expect("shrink");
    assert_eq!(ufs.getattr(ino).expect("attr").blocks, 1);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn write_cross_block_boundary() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    let data = pattern(bsize + 200);
    let offset = bsize - 100;
    ufs.write(ino, offset as u64, &data).expect("write");
    let mut model = vec![0u8; offset];
    model.extend_from_slice(&data);
    check_file(&mut ufs, ino, &model);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn write_extends_fragment_within_block() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    ufs.write(ino, 0, &pattern(100)).expect("first");
    ufs.write(ino, 3500, b"abcdefghij").expect("second");
    let mut model = pattern(100);
    model.resize(3500, 0);
    model.extend_from_slice(b"abcdefghij");
    check_file(&mut ufs, ino, &model);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn write_gap_in_existing_fragment_zeroes_stale_bytes() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let marker: Vec<u8> = (0..32u8).map(|i| 0xa5 ^ i.wrapping_mul(37)).collect();
    ufs.write(ino, 0, &marker).expect("write marker");
    ufs.sync().expect("sync");
    let mut disk = ufs.into_inner();
    let at = disk
        .data
        .windows(marker.len())
        .position(|window| window == marker.as_slice())
        .expect("marker in image");
    disk.data[at + 40..at + 50].fill(0xaa);
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: false }).expect("remount");
    ufs.write(ino, 50, b"0123456789")
        .expect("extend in fragment");
    let got = read_all(&mut ufs, ino, 60).expect("read");
    assert_eq!(&got[..32], &marker[..]);
    assert!(got[32..50].iter().all(|byte| *byte == 0));
    assert_eq!(&got[50..60], b"0123456789");
    ufs.verify_allocation().expect("verify");
}

#[test]
fn write_multiple_appends() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let mut model = Vec::new();
    for size in [3usize, 500, 4096, 77, 8193] {
        let chunk = pattern(size);
        ufs.write(ino, model.len() as u64, &chunk).expect("append");
        model.extend_from_slice(&chunk);
    }
    check_file(&mut ufs, ino, &model);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn write_empty_is_noop() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    assert_eq!(ufs.write(ino, 0, &[]).expect("empty write"), 0);
    assert_eq!(ufs.getattr(ino).expect("attr").size, 0);
    assert_eq!(ufs.getattr(ino).expect("attr").blocks, 0);
}

#[test]
fn write_zero_length_at_offset_no_change() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    ufs.write(ino, 0, &pattern(100)).expect("write");
    assert_eq!(ufs.write(ino, 5000, &[]).expect("empty"), 0);
    assert_eq!(ufs.getattr(ino).expect("attr").size, 100);
}

#[test]
fn write_sparse_allocation() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    ufs.write(ino, 1000, b"Z").expect("sparse write");
    let attr = ufs.getattr(ino).expect("attr");
    assert_eq!(attr.size, 1001);
    assert_eq!(attr.blocks, 2, "one fragment of 1024 bytes");
    let got = read_all(&mut ufs, ino, 1001).expect("read");
    assert!(got[..1000].iter().all(|byte| *byte == 0));
    assert_eq!(got[1000], b'Z');
    ufs.verify_allocation().expect("verify");
}

#[test]
fn write_after_truncate_zero_reuses() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as u64;
    ufs.write(ino, 0, &pattern((5 * bsize) as usize))
        .expect("write");
    ufs.truncate(ino, 0).expect("truncate");
    let data = pattern((2 * bsize) as usize);
    ufs.write(ino, 0, &data).expect("rewrite");
    check_file(&mut ufs, ino, &data);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn truncate_to_zero_frees_all() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    let before = ufs.statfs().blocks_free;
    ufs.write(ino, 0, &pattern(10 * bsize)).expect("write");
    assert!(ufs.statfs().blocks_free < before);
    ufs.truncate(ino, 0).expect("truncate");
    assert_eq!(ufs.statfs().blocks_free, before);
    let attr = ufs.getattr(ino).expect("attr");
    assert_eq!(attr.size, 0);
    assert_eq!(attr.blocks, 0);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn truncate_grow_allocates_last_block_only() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as u64;
    ufs.truncate(ino, 5 * bsize + 123).expect("grow");
    let attr = ufs.getattr(ino).expect("attr");
    let fsize = ufs.statfs().fragment_size as u64;
    assert_eq!(attr.size, 5 * bsize + 123);
    assert_eq!(attr.blocks, fsize / 512);
    let got = read_all(&mut ufs, ino, attr.size).expect("read");
    assert!(got.iter().all(|byte| *byte == 0));
    ufs.verify_allocation().expect("verify");
}

#[test]
fn truncate_same_size_is_noop() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    ufs.write(ino, 0, &pattern(10_000)).expect("write");
    let before = ufs.getattr(ino).expect("attr");
    ufs.truncate(ino, before.size).expect("same size");
    let after = ufs.getattr(ino).expect("attr");
    assert_eq!(after.size, before.size);
    assert_eq!(after.blocks, before.blocks);
    check_file(&mut ufs, ino, &pattern(10_000));
}

#[test]
fn truncate_shrink_to_block_boundary() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    let data = pattern(5 * bsize);
    ufs.write(ino, 0, &data).expect("write");
    ufs.truncate(ino, (2 * bsize) as u64).expect("shrink");
    let attr = ufs.getattr(ino).expect("attr");
    assert_eq!(attr.size, (2 * bsize) as u64);
    assert_eq!(attr.blocks, (2 * bsize / 512) as u64);
    check_file(&mut ufs, ino, &data[..2 * bsize]);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn truncate_shrink_within_last_block() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    let data = pattern(5 * bsize);
    ufs.write(ino, 0, &data).expect("write");
    let keep = 2 * bsize + 10;
    ufs.truncate(ino, keep as u64).expect("shrink");
    let attr = ufs.getattr(ino).expect("attr");
    assert_eq!(attr.size, keep as u64);
    assert_eq!(attr.blocks, keep.div_ceil(512) as u64);
    check_file(&mut ufs, ino, &data[..keep]);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn truncate_grow_then_write_into_hole() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as u64;
    ufs.truncate(ino, 10 * bsize).expect("grow");
    ufs.write(ino, 5 * bsize + 5, b"middle")
        .expect("write middle");
    let mut model = vec![0u8; (10 * bsize) as usize];
    model[(5 * bsize + 5) as usize..(5 * bsize + 11) as usize].copy_from_slice(b"middle");
    check_file(&mut ufs, ino, &model);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn write_double_indirect_ufs2_4k() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    let size = 1100 * bsize + 123;
    let data = pattern(size);
    assert_eq!(ufs.write(ino, 0, &data).expect("write"), size);
    check_file(&mut ufs, ino, &data);
    ufs.verify_allocation().expect("verify");
    ufs.truncate(ino, 0).expect("truncate");
    ufs.verify_allocation().expect("verify after truncate");
}

#[test]
fn write_double_indirect_ufs1_4k() {
    let mut ufs = mount_rw("ufs1-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    let size = 1100 * bsize + 123;
    let data = pattern(size);
    assert_eq!(ufs.write(ino, 0, &data).expect("write"), size);
    check_file(&mut ufs, ino, &data);
    ufs.verify_allocation().expect("verify");
    ufs.truncate(ino, 0).expect("truncate");
    ufs.verify_allocation().expect("verify after truncate");
}

#[test]
fn truncate_frees_double_indirect() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as u64;
    let size = 1100 * bsize + 123;
    ufs.write(ino, 0, &pattern(size as usize)).expect("write");
    ufs.truncate(ino, 20 * bsize).expect("truncate to single");
    let attr = ufs.getattr(ino).expect("attr");
    assert_eq!(attr.size, 20 * bsize);
    assert_eq!(attr.blocks, (20 * bsize + bsize) / 512);
    ufs.verify_allocation().expect("verify");
    ufs.truncate(ino, 0).expect("truncate all");
    assert_eq!(ufs.getattr(ino).expect("attr").blocks, 0);
    ufs.verify_allocation().expect("verify empty");
}

#[test]
fn setattr_mode_only() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    ufs.setattr(
        ino,
        SetAttr {
            mode: Some(0o600),
            ..SetAttr::default()
        },
    )
    .expect("setattr");
    let attr = ufs.getattr(ino).expect("attr");
    assert_eq!(attr.mode, 0o600);
    assert_eq!(attr.kind, FileType::Regular);
}

#[test]
fn setattr_uid_gid() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    ufs.setattr(
        ino,
        SetAttr {
            uid: Some(4321),
            gid: Some(8765),
            ..SetAttr::default()
        },
    )
    .expect("setattr");
    let attr = ufs.getattr(ino).expect("attr");
    assert_eq!(attr.uid, 4321);
    assert_eq!(attr.gid, 8765);
}

#[test]
fn setattr_times() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let atime = Timespec { sec: 100, nsec: 1 };
    let mtime = Timespec { sec: 200, nsec: 2 };
    ufs.setattr(
        ino,
        SetAttr {
            atime: Some(atime),
            mtime: Some(mtime),
            ..SetAttr::default()
        },
    )
    .expect("setattr");
    let attr = ufs.getattr(ino).expect("attr");
    assert_eq!(attr.atime, atime);
    assert_eq!(attr.mtime, mtime);
}

#[test]
fn setattr_size_grow_zeroes() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let data = pattern(700);
    ufs.write(ino, 0, &data).expect("write");
    ufs.setattr(
        ino,
        SetAttr {
            size: Some(9000),
            ..SetAttr::default()
        },
    )
    .expect("grow");
    let mut model = data;
    model.resize(9000, 0);
    check_file(&mut ufs, ino, &model);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn setattr_size_shrink_preserves_prefix() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let data = pattern(9000);
    ufs.write(ino, 0, &data).expect("write");
    ufs.setattr(
        ino,
        SetAttr {
            size: Some(1234),
            ..SetAttr::default()
        },
    )
    .expect("shrink");
    check_file(&mut ufs, ino, &data[..1234]);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn setattr_no_change_advances_ctime() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let before = ufs.getattr(ino).expect("attr").ctime;
    ufs.setattr(ino, SetAttr::default()).expect("setattr");
    let after = ufs.getattr(ino).expect("attr").ctime;
    assert!(after.sec > before.sec);
}

#[test]
fn write_updates_times() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let before = ufs.getattr(ino).expect("attr");
    ufs.write(ino, 0, b"time").expect("write");
    let after = ufs.getattr(ino).expect("attr");
    assert!(after.mtime.sec > before.mtime.sec);
    assert_eq!(after.mtime, after.ctime);
}

#[test]
fn truncate_updates_times() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    ufs.write(ino, 0, &pattern(5000)).expect("write");
    let before = ufs.getattr(ino).expect("attr");
    ufs.truncate(ino, 100).expect("truncate");
    let after = ufs.getattr(ino).expect("attr");
    assert!(after.mtime.sec > before.mtime.sec);
    assert_eq!(after.mtime, after.ctime);
}

#[test]
fn sync_inode_and_fsync_keep_content() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let data = pattern(7000);
    ufs.write(ino, 0, &data).expect("write");
    ufs.sync_inode(ino).expect("sync inode");
    ufs.fsync(ino).expect("fsync");
    check_file(&mut ufs, ino, &data);
}

#[test]
fn write_persists_across_remount() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let data = pattern(3 * 4096 + 17);
    ufs.write(ino, 0, &data).expect("write");
    let mut ufs = remount(ufs);
    check_file(&mut ufs, ino, &data);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn truncate_persists_across_remount() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    let data = pattern(5 * bsize);
    ufs.write(ino, 0, &data).expect("write");
    ufs.truncate(ino, (2 * bsize + 33) as u64)
        .expect("truncate");
    let mut ufs = remount(ufs);
    check_file(&mut ufs, ino, &data[..2 * bsize + 33]);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn symlink_overwrite_fast_to_slow() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = ufs.alloc_inode(2, 0o120777).expect("symlink inode");
    ufs.write(ino, 0, b"short").expect("fast");
    let target = pattern(150);
    ufs.write(ino, 0, &target).expect("slow");
    let mut buffer = vec![0u8; 256];
    let n = ufs.readlink(ino, &mut buffer).expect("readlink");
    assert_eq!(&buffer[..n], &target[..]);
    assert_eq!(ufs.getattr(ino).expect("attr").size, 150);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn symlink_overwrite_slow_to_fast() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = ufs.alloc_inode(2, 0o120777).expect("symlink inode");
    ufs.write(ino, 0, &pattern(150)).expect("slow");
    ufs.write(ino, 0, b"tiny").expect("fast");
    let mut buffer = [0u8; 16];
    let n = ufs.readlink(ino, &mut buffer).expect("readlink");
    assert_eq!(&buffer[..n], b"tiny");
    assert_eq!(ufs.getattr(ino).expect("attr").blocks, 0);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn symlink_truncate_frees_block() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = ufs.alloc_inode(2, 0o120777).expect("symlink inode");
    ufs.write(ino, 0, &pattern(200)).expect("slow");
    assert!(ufs.getattr(ino).expect("attr").blocks > 0);
    ufs.truncate(ino, 0).expect("truncate symlink");
    let attr = ufs.getattr(ino).expect("attr");
    assert_eq!(attr.size, 0);
    assert_eq!(attr.blocks, 0);
    let mut buffer = [0u8; 8];
    assert_eq!(ufs.readlink(ino, &mut buffer).expect("readlink"), 0);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn symlink_readlink_short_buffer() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = ufs.alloc_inode(2, 0o120777).expect("symlink inode");
    let target = pattern(200);
    ufs.write(ino, 0, &target).expect("slow");
    let mut buffer = [0u8; 10];
    let n = ufs.readlink(ino, &mut buffer).expect("readlink");
    assert_eq!(n, 10);
    assert_eq!(&buffer[..], &target[..10]);
}

#[test]
fn write_readonly_mount_fails() {
    let mut ufs = mount_ro("ufs2-b4096");
    let ino = ufs.lookup(ufs.root(), b"ten.txt").expect("lookup");
    assert_eq!(ufs.write(ino, 0, b"x"), Err(UfsError::ReadOnly));
}

#[test]
fn truncate_readonly_mount_fails() {
    let mut ufs = mount_ro("ufs2-b4096");
    let ino = ufs.lookup(ufs.root(), b"ten.txt").expect("lookup");
    assert_eq!(ufs.truncate(ino, 0), Err(UfsError::ReadOnly));
}

#[test]
fn setattr_readonly_mount_fails() {
    let mut ufs = mount_ro("ufs2-b4096");
    let ino = ufs.lookup(ufs.root(), b"ten.txt").expect("lookup");
    assert_eq!(
        ufs.setattr(ino, SetAttr::default()),
        Err(UfsError::ReadOnly)
    );
}

#[test]
fn fsync_readonly_ok() {
    let mut ufs = mount_ro("ufs2-b4096");
    let ino = ufs.lookup(ufs.root(), b"ten.txt").expect("lookup");
    ufs.fsync(ino).expect("read-only fsync");
}

#[test]
fn write_directory_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    assert_eq!(ufs.write(root, 0, b"x"), Err(UfsError::IsDir));
}

#[test]
fn truncate_directory_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let root = ufs.root();
    assert_eq!(ufs.truncate(root, 0), Err(UfsError::IsDir));
}

#[test]
fn write_device_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = ufs.lookup(ufs.root(), b"cdev").expect("lookup");
    assert_eq!(ufs.write(ino, 0, b"x"), Err(UfsError::Invalid));
}

#[test]
fn write_fifo_rejected() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = ufs.lookup(ufs.root(), b"fifo").expect("lookup");
    assert_eq!(ufs.write(ino, 0, b"x"), Err(UfsError::Invalid));
}

#[test]
fn write_fragment_growth_accounting() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let fsize = ufs.statfs().fragment_size as u64;
    ufs.write(ino, 0, b"a").expect("one byte");
    assert_eq!(ufs.getattr(ino).expect("attr").blocks, fsize / 512);
    let bsize = ufs.statfs().block_size as usize;
    ufs.write(ino, 0, &pattern(bsize)).expect("full block");
    assert_eq!(
        ufs.getattr(ino).expect("attr").blocks,
        ufs.statfs().block_size as u64 / 512
    );
    ufs.verify_allocation().expect("verify");
}

#[test]
fn fragment_tail_grows_to_full_block_8k() {
    let mut ufs = mount_rw("ufs2-b8192");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    ufs.write(ino, 0, b"x").expect("one byte");
    let full = pattern(bsize);
    ufs.write(ino, 0, &full).expect("full block");
    check_file(&mut ufs, ino, &full);
    ufs.verify_allocation().expect("verify");
}

/// Deterministic xorshift for the randomised model test.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[test]
fn randomized_model_10k() {
    let mut ufs = mount_rw("ufs2-b4096");
    let files: Vec<Ino> = (0..20).map(|_| new_file(&mut ufs)).collect();
    let mut models: Vec<Vec<u8>> = vec![Vec::new(); files.len()];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for step in 0..10_000u32 {
        let file = (rng.next() % files.len() as u64) as usize;
        let ino = files[file];
        let choice = rng.next() % 10;
        let mut context = format!("step {step} file {file} op {choice}");
        match choice {
            0..=5 => {
                let len = 1 + (rng.next() % 3000) as usize;
                let offset = (rng.next() % 32_000) as usize;
                context = format!("{context} write off {offset} len {len}");
                let data: Vec<u8> = (0..len).map(|_| (rng.next() & 0xff) as u8).collect();
                let model = &mut models[file];
                if model.len() < offset {
                    model.resize(offset, 0);
                }
                let end = offset + len;
                if model.len() < end {
                    model.resize(end, 0);
                }
                model[offset..end].copy_from_slice(&data);
                ufs.write(ino, offset as u64, &data).expect("model write");
            }
            6..=8 => {
                let size = (rng.next() % 33_000) as usize;
                context = format!("{context} truncate {size}");
                let model = &mut models[file];
                model.resize(size, 0);
                ufs.truncate(ino, size as u64).expect("model truncate");
            }
            _ => {
                let uid = (rng.next() & 0xffff) as u32;
                context = format!("{context} uid {uid}");
                ufs.setattr(
                    ino,
                    SetAttr {
                        uid: Some(uid),
                        ..SetAttr::default()
                    },
                )
                .expect("model setattr");
                assert_eq!(ufs.getattr(ino).expect("attr").uid, uid);
            }
        }
        check_model(&mut ufs, ino, &models[file], &context);
        if step % 250 == 0 {
            ufs.verify_allocation().expect("periodic verify");
        }
    }
    ufs.verify_allocation().expect("final verify");
}

#[test]
fn enospc_midwrite_consistent() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let chunk = pattern(64 * 1024);
    let mut written = Vec::new();
    let error = loop {
        match ufs.write(ino, written.len() as u64, &chunk) {
            Ok(n) => {
                assert_eq!(n, chunk.len());
                written.extend_from_slice(&chunk);
            }
            Err(error) => break error,
        }
        assert!(written.len() < 24 * 1024 * 1024, "filesystem never filled");
    };
    assert_eq!(error, UfsError::NoSpace);
    let size = ufs.getattr(ino).expect("attr").size as usize;
    assert!(size >= written.len());
    let mut expected = written;
    let extra = size - expected.len();
    expected.extend_from_slice(&chunk[..extra]);
    let got = read_all(&mut ufs, ino, size as u64).expect("read partial");
    assert_eq!(got, expected);
    ufs.verify_allocation().expect("consistent after ENOSPC");
    ufs.truncate(ino, 0).expect("truncate");
    ufs.verify_allocation().expect("consistent after free");
    ufs.write(ino, 0, &chunk).expect("write after free");
    ufs.verify_allocation().expect("final verify");
}

#[test]
fn write_grows_fragment_in_double_indirect() {
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    let offset = 1100 * bsize + 100;
    let mut model = pattern(offset + 1);
    ufs.write(ino, 0, &model).expect("write");
    let tail = pattern(500);
    ufs.write(ino, offset as u64, &tail).expect("grow tail");
    model.truncate(offset);
    model.extend_from_slice(&tail);
    check_file(&mut ufs, ino, &model);
    ufs.verify_allocation().expect("verify");
}

#[test]
fn oracle_fsck_when_present() {
    let oracle = std::path::Path::new("/tmp/opencode/ufs-oracle/bin/fsck_ffs");
    if !oracle.exists() {
        println!("ORACLE ABSENT: fsck_ffs not installed; skipped");
        return;
    }
    let mut ufs = mount_rw("ufs2-b4096");
    let ino = new_file(&mut ufs);
    let bsize = ufs.statfs().block_size as usize;
    ufs.write(ino, 0, &pattern(13 * bsize + 7)).expect("write");
    ufs.truncate(ino, (2 * bsize + 9) as u64).expect("truncate");
    let disk = ufs.unmount().expect("unmount");
    let path = std::env::temp_dir().join(format!("k53-fsck-{}.img", std::process::id()));
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
