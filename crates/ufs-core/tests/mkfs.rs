// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ufs-core::mkfs` formatting tests.

mod common;

use common::VecDisk;
use ufs_core::mkfs::{MkfsOptions, format};
use ufs_core::{FileType, MountOptions, Ufs};

const MIB: u64 = 1024 * 1024;

fn options(size: u64, version: u8) -> MkfsOptions<'static> {
    MkfsOptions {
        size,
        version,
        block_size: 4096,
        fragment_size: 512,
        bytes_per_inode: 4096,
        minfree: 5,
        label: "k56",
        timestamp: 1_767_225_600,
    }
}

fn format_image(size: u64, version: u8) -> VecDisk {
    let disk = VecDisk::new(vec![0u8; usize::try_from(size).expect("size")]);
    let ufs = format(disk, &options(size, version)).expect("format");
    ufs.unmount().expect("unmount")
}

#[test]
fn formats_ufs2_16m_and_creates_lost_found() {
    let disk = format_image(16 * MIB, 2);
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("mount");
    let root = ufs.getattr(ufs.root()).expect("root attr");
    assert_eq!(root.kind, FileType::Directory);
    assert_eq!(root.nlink, 3);
    let lost = ufs.lookup(ufs.root(), b"lost+found").expect("lost+found");
    let lost = ufs.getattr(lost).expect("lost+found attr");
    assert_eq!(lost.kind, FileType::Directory);
    assert_eq!(lost.mode, 0o700);
    assert_eq!(lost.nlink, 2);
}

#[test]
fn formats_ufs1_16m_and_creates_lost_found() {
    let disk = format_image(16 * MIB, 1);
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("mount");
    let root = ufs.getattr(ufs.root()).expect("root attr");
    assert_eq!(root.kind, FileType::Directory);
    assert_eq!(root.nlink, 3);
    let lost = ufs.lookup(ufs.root(), b"lost+found").expect("lost+found");
    assert_eq!(ufs.getattr(lost).expect("attr").kind, FileType::Directory);
}

#[test]
fn formatted_image_passes_allocation_check() {
    let disk = VecDisk::new(vec![0u8; 16 * MIB as usize]);
    let mut ufs = format(disk, &options(16 * MIB, 2)).expect("format");
    ufs.verify_allocation().expect("allocation check");
    ufs.sync().expect("sync");
}

#[test]
fn formatted_image_remounts_with_consistent_summaries() {
    let disk = VecDisk::new(vec![0u8; 16 * MIB as usize]);
    let ufs = format(disk, &options(16 * MIB, 2)).expect("format");
    let disk = ufs.unmount().expect("unmount");
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: false }).expect("remount");
    ufs.verify_allocation().expect("remount allocation check");
}

#[test]
fn primary_and_backup_superblocks_match() {
    let disk = format_image(16 * MIB, 2);
    let data = &disk.data;
    let base = 8192;
    let sbsize = read_u32(data, base + 104) as usize;
    let sblkno = read_i32(data, base + 8) as u64;
    let fsize = read_u32(data, base + 52) as u64;
    let backup = (sblkno * fsize) as usize;
    assert_eq!(&data[base..base + sbsize], &data[backup..backup + sbsize]);
}

fn read_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(data[off..off + 4].try_into().expect("u32"))
}

fn read_i32(data: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(data[off..off + 4].try_into().expect("i32"))
}

#[test]
fn geometry_fields_are_internally_consistent() {
    let disk = format_image(16 * MIB, 2);
    let data = &disk.data;
    let base = 8192;
    assert_eq!(read_u32(data, base + 1372), 0x1954_0119);
    assert_eq!(read_u32(data, base + 48), 4096);
    assert_eq!(read_u32(data, base + 52), 512);
    assert_eq!(read_u32(data, base + 56), 8);
    assert_eq!(read_i32(data, base + 8), 32);
    assert_eq!(read_i32(data, base + 12), 48);
    assert_eq!(read_i32(data, base + 16), 56);
    let ncg = read_u32(data, base + 44);
    let ipg = read_u32(data, base + 184);
    let fpg = read_u32(data, base + 188);
    assert!(ncg > 0);
    assert!(ipg > 0);
    assert!(fpg > 0);
    println!(
        "ufs2 16m: ncg={ncg} ipg={ipg} fpg={fpg} dblkno={} cgsize={} sbsize={}",
        read_i32(data, base + 20),
        read_u32(data, base + 160),
        read_u32(data, base + 104)
    );
}

fn golden(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/ufs-vectors/golden")
        .join(format!("{name}.img.gz"));
    let output = std::process::Command::new("gzip")
        .arg("-dc")
        .arg(path)
        .output()
        .expect("gzip");
    assert!(output.status.success());
    output.stdout
}

#[test]
fn geometry_table_against_makefs() {
    for (version, block, fragment, name) in [
        (1u8, 4096u32, 512u32, "ufs1-b4096"),
        (1, 8192, 1024, "ufs1-b8192"),
        (2, 4096, 512, "ufs2-b4096"),
        (2, 8192, 1024, "ufs2-b8192"),
    ] {
        let opts = MkfsOptions {
            size: 16 * MIB,
            version,
            block_size: block,
            fragment_size: fragment,
            bytes_per_inode: 4096,
            minfree: 5,
            label: "",
            timestamp: 1_767_225_600,
        };
        let disk = VecDisk::new(vec![0u8; 16 * MIB as usize]);
        let ufs = format(disk, &opts).expect("format");
        let disk = ufs.unmount().expect("unmount");
        let generated = &disk.data;
        let golden = golden(name);
        let base = 8192;
        for (field, off) in [
            ("magic", 1372usize),
            ("bsize", 48),
            ("fsize", 52),
            ("frag", 56),
            ("sblkno", 8),
            ("cblkno", 12),
            ("iblkno", 16),
            ("sbsize", 104),
            ("maxcontig", 88),
            ("maxbpg", 92),
            ("maxbsize", 860),
            ("sblockloc", 1000),
            ("nindir", 116),
            ("inopb", 120),
            ("cgsize", 160),
            ("old_cpg", 180),
            ("contigsum", 1316),
            ("maxsymlinklen", 1320),
            ("old_flags", 211),
        ] {
            if field == "old_flags" {
                assert_eq!(generated[base + off], golden[base + off], "{name} {field}");
            } else {
                assert_eq!(
                    read_u32(generated, base + off),
                    read_u32(&golden, base + off),
                    "{name} {field}"
                );
            }
        }
        println!(
            "{name}: generated ncg={} ipg={} fpg={} dblkno={}; golden ncg={} ipg={} fpg={} dblkno={} (density-dependent)",
            read_u32(generated, base + 44),
            read_u32(generated, base + 184),
            read_u32(generated, base + 188),
            read_i32(generated, base + 20),
            read_u32(&golden, base + 44),
            read_u32(&golden, base + 184),
            read_u32(&golden, base + 188),
            read_i32(&golden, base + 20),
        );
    }
}
