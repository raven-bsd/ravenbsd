// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end `ufs_core::check` tests on formatted images and byte surgery.

mod common;

use common::VecDisk;
use ufs_core::check::{Mode, Problem, Report, check};
use ufs_core::mkfs::{MkfsOptions, format};
use ufs_core::{FileType, Ino, MountOptions, Ufs};

const MIB: u64 = 1024 * 1024;
const STAMP: i64 = 1_767_225_600;

fn options(size: u64, version: u8) -> MkfsOptions<'static> {
    MkfsOptions {
        size,
        version,
        block_size: 4096,
        fragment_size: 512,
        bytes_per_inode: 4096,
        minfree: 5,
        label: "k57",
        timestamp: STAMP,
    }
}

fn formatted(size: u64, version: u8) -> Ufs<VecDisk> {
    let disk = VecDisk::new(vec![0u8; usize::try_from(size).expect("size")]);
    format(disk, &options(size, version)).expect("format")
}

fn simple_image(size: u64, version: u8) -> VecDisk {
    let mut ufs = formatted(size, version);
    let root = ufs.root();
    let dir = ufs.mkdir(root, b"dir", 0o755, 0, 0).expect("mkdir");
    let file = ufs.create(dir, b"file", 0o644, 0, 0).expect("create");
    ufs.write(file, 0, b"hello, world\n").expect("write");
    let big = ufs.create(root, b"big", 0o600, 0, 0).expect("create");
    ufs.write(big, 0, &vec![0x5a; 20_000]).expect("write big");
    ufs.symlink(root, b"link", b"dir/file", 0, 0)
        .expect("symlink");
    ufs.sync().expect("sync");
    ufs.unmount().expect("unmount")
}

fn read_u16(data: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(data[off..off + 2].try_into().expect("u16"))
}

fn read_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(data[off..off + 4].try_into().expect("u32"))
}

fn read_i32(data: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(data[off..off + 4].try_into().expect("i32"))
}

fn write_u16(data: &mut [u8], off: usize, value: u16) {
    data[off..off + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(data: &mut [u8], off: usize, value: u32) {
    data[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i32(data: &mut [u8], off: usize, value: i32) {
    data[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_i16(data: &mut [u8], off: usize, value: i16) {
    data[off..off + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(data: &mut [u8], off: usize, value: u64) {
    data[off..off + 8].copy_from_slice(&value.to_le_bytes());
}

struct Geometry {
    ufs2: bool,
    fsize: u32,
    frag: u32,
    ipg: u32,
    fpg: u32,
    inopb: u32,
    iblkno: i32,
    cblkno: i32,
    size: u64,
    inode_size: usize,
}

impl Geometry {
    fn read(data: &[u8], base: u64) -> Self {
        let base = base as usize;
        let magic = read_u32(data, base + 1372);
        Self {
            ufs2: magic == 0x1954_0119,
            fsize: read_u32(data, base + 52),
            frag: read_u32(data, base + 56),
            ipg: read_u32(data, base + 184),
            fpg: read_u32(data, base + 188),
            inopb: read_u32(data, base + 120),
            iblkno: read_i32(data, base + 16),
            cblkno: read_i32(data, base + 12),
            size: u64::from_le_bytes(data[base + 1080..base + 1088].try_into().expect("size")),
            inode_size: if magic == 0x1954_0119 { 256 } else { 128 },
        }
    }

    fn inode_offset(&self, ino: Ino) -> usize {
        let cg = u64::from(ino) / u64::from(self.ipg);
        let inum = u64::from(ino) % u64::from(self.ipg);
        let block = inum / u64::from(self.inopb);
        let slot = inum % u64::from(self.inopb);
        let frag = cg * u64::from(self.fpg) + self.iblkno as u64 + block * u64::from(self.frag);
        (frag * u64::from(self.fsize) + slot * self.inode_size as u64) as usize
    }

    fn inode_db_offset(&self, ino: Ino, index: usize) -> usize {
        let base = self.inode_offset(ino);
        base + if self.ufs2 {
            112 + index * 8
        } else {
            40 + index * 4
        }
    }

    fn read_ino_db(&self, data: &[u8], ino: Ino, index: usize) -> u64 {
        let off = self.inode_db_offset(ino, index);
        if self.ufs2 {
            u64::from_le_bytes(data[off..off + 8].try_into().expect("db"))
        } else {
            u64::from(read_u32(data, off))
        }
    }

    fn write_ino_db(&self, data: &mut [u8], ino: Ino, index: usize, value: u64) {
        let off = self.inode_db_offset(ino, index);
        if self.ufs2 {
            write_u64(data, off, value);
        } else {
            write_u32(data, off, u32::try_from(value).expect("32-bit block"));
        }
    }

    fn write_nlink(&self, data: &mut [u8], ino: Ino, value: i16) {
        let off = self.inode_offset(ino);
        write_i16(data, off + 2, value);
    }

    fn cg_offset(&self, cg: u32) -> usize {
        ((u64::from(cg) * u64::from(self.fpg) + self.cblkno as u64) * u64::from(self.fsize))
            as usize
    }

    fn cg_free_map_offset(&self, data: &[u8], cg: u32) -> usize {
        let base = self.cg_offset(cg);
        base + read_u32(data, base + 96) as usize
    }

    fn cg_inode_map_offset(&self, data: &[u8], cg: u32) -> usize {
        let base = self.cg_offset(cg);
        base + read_u32(data, base + 92) as usize
    }

    fn dir_block(&self, data: &[u8], ino: Ino) -> usize {
        (self.read_ino_db(data, ino, 0) * u64::from(self.fsize)) as usize
    }
}

fn find_entry(data: &[u8], base: usize, name: &[u8]) -> Option<usize> {
    let mut off = base;
    while off < base + 512 {
        let reclen = read_u16(data, off + 4) as usize;
        let namlen = data[off + 7] as usize;
        let entry_name = data.get(off + 8..off + 8 + namlen)?;
        if entry_name == name {
            return Some(off);
        }
        if reclen == 0 {
            break;
        }
        off += reclen;
    }
    None
}

fn has(report: &Report, problem: Problem) -> bool {
    report
        .findings
        .iter()
        .any(|finding| finding.problem == problem)
}

fn image_hash(disk: &VecDisk) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in &disk.data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn golden(name: &str) -> VecDisk {
    let path = common::vectors_dir()
        .join("golden")
        .join(format!("{name}.img.gz"));
    VecDisk::new(common::load_gz(&path))
}

#[test]
fn golden_images_are_clean() {
    for name in ["ufs1-b4096", "ufs1-b8192", "ufs2-b4096", "ufs2-b8192"] {
        let mut disk = golden(name);
        let report = check(&mut disk, Mode::ReadOnly).expect("check golden");
        assert!(report.findings.is_empty(), "{name}: {:?}", report.findings);
        assert!(report.clean_after);
    }
}

#[test]
fn clean_image_reports_nothing() {
    for version in [1u8, 2] {
        let mut disk = simple_image(16 * MIB, version);
        let before = image_hash(&disk);
        let report = check(&mut disk, Mode::ReadOnly).expect("check");
        assert!(report.findings.is_empty(), "{:?}", report.findings);
        assert!(report.fixed.is_empty());
        assert!(report.clean_after);
        assert_eq!(image_hash(&disk), before);
    }
}

#[test]
fn read_only_never_writes_to_a_corrupt_image() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    geometry.write_nlink(&mut disk.data, 3, 9);
    let before = image_hash(&disk);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(has(&report, Problem::BadLinkCount));
    assert_eq!(image_hash(&disk), before);
}

#[test]
fn read_only_does_not_write_when_an_inode_must_be_cleared() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    geometry.write_ino_db(&mut disk.data, 4, 0, geometry.size + 100);
    let before = image_hash(&disk);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(has(&report, Problem::BadBlock));
    assert_eq!(image_hash(&disk), before);
}

#[test]
fn repair_fixes_a_wrong_link_count() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    geometry.write_nlink(&mut disk.data, 3, 9);
    let report = check(&mut disk, Mode::Repair).expect("repair");
    assert!(has(&report, Problem::BadLinkCount));
    assert!(!report.fixed.is_empty());
    assert!(report.clean_after, "{:?}", report.findings);
    let verify = check(&mut disk, Mode::ReadOnly).expect("verify");
    assert!(verify.findings.is_empty(), "{:?}", verify.findings);
}

#[test]
fn clean_image_repair_is_a_no_op_for_inodes() {
    let mut disk = simple_image(16 * MIB, 2);
    let report = check(&mut disk, Mode::Repair).expect("repair");
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert!(report.clean_after);
}

#[test]
fn detects_a_partially_allocated_inode() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let free = 50;
    let off = geometry.inode_offset(free);
    disk.data[off..off + geometry.inode_size].fill(0);
    write_u64(&mut disk.data, off + 16, 123);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(has(&report, Problem::PartiallyAllocatedInode));
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_an_unknown_inode_type_and_clears_it() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let off = geometry.inode_offset(3);
    write_u16(&mut disk.data, off, 0o150644);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(has(&report, Problem::UnknownInodeType));
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_and_fixes_an_incorrect_block_count() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let off = geometry.inode_offset(3);
    let blocks = if geometry.ufs2 {
        u64::from_le_bytes(disk.data[off + 24..off + 32].try_into().expect("blocks"))
    } else {
        u64::from(read_u32(&disk.data, off + 104))
    };
    if geometry.ufs2 {
        write_u64(&mut disk.data, off + 24, blocks + 1);
    } else {
        write_u32(&mut disk.data, off + 104, blocks as u32 + 1);
    }
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(has(&report, Problem::IncorrectBlockCount));
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_an_out_of_range_block() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    geometry.write_ino_db(&mut disk.data, 4, 0, geometry.size + 100);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(has(&report, Problem::BadBlock));
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_a_directory_entry_to_a_free_inode() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let root_db = geometry.read_ino_db(&disk.data, 2, 0);
    let base = (root_db * u64::from(geometry.fsize)) as usize;
    let mut off = base;
    let mut done = false;
    while off < base + 512 {
        let reclen = read_u16(&disk.data, off + 4) as usize;
        let namlen = disk.data[off + 7] as usize;
        let name = &disk.data[off + 8..off + 8 + namlen];
        if name == b"big" {
            write_u32(&mut disk.data, off, 60);
            done = true;
            break;
        }
        if reclen == 0 {
            break;
        }
        off += reclen;
    }
    assert!(done, "entry not found");
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(
        has(&report, Problem::UnallocatedEntry),
        "{:?}",
        report.findings
    );
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_a_bitmap_bit_flip() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let offset = geometry.cg_free_map_offset(&disk.data, 0);
    let bit = 600u32;
    let byte = offset + (bit / 8) as usize;
    let mask = 1u8 << (bit % 8);
    let before = disk.data[byte];
    disk.data[byte] = if before & mask == 0 {
        before | mask
    } else {
        before & !mask
    };
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(has(&report, Problem::BlockBitmap), "{:?}", report.findings);
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_a_bad_superblock_summary() {
    let mut disk = simple_image(16 * MIB, 2);
    let base = 8192usize;
    let value = read_i32(&disk.data, base + 1008 + 8);
    write_i32(&mut disk.data, base + 1008 + 8, value - 1);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(
        has(&report, Problem::SuperblockSummary),
        "{:?}",
        report.findings
    );
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_an_entry_type_mismatch() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let root_db = geometry.read_ino_db(&disk.data, 2, 0);
    let base = (root_db * u64::from(geometry.fsize)) as usize;
    let mut off = base;
    let mut done = false;
    while off < base + 512 {
        let reclen = read_u16(&disk.data, off + 4) as usize;
        let namlen = disk.data[off + 7] as usize;
        let name = &disk.data[off + 8..off + 8 + namlen];
        if name == b"big" {
            disk.data[off + 6] = 4;
            done = true;
            break;
        }
        if reclen == 0 {
            break;
        }
        off += reclen;
    }
    assert!(done, "entry not found");
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(has(&report, Problem::BadEntryType), "{:?}", report.findings);
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_an_orphaned_directory_and_reconnects_it() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let root_db = geometry.read_ino_db(&disk.data, 2, 0);
    let base = (root_db * u64::from(geometry.fsize)) as usize;
    let mut off = base;
    let mut done = false;
    while off < base + 512 {
        let reclen = read_u16(&disk.data, off + 4) as usize;
        let namlen = disk.data[off + 7] as usize;
        let name = &disk.data[off + 8..off + 8 + namlen];
        if name == b"dir" {
            write_u32(&mut disk.data, off, 0);
            done = true;
            break;
        }
        if reclen == 0 {
            break;
        }
        off += reclen;
    }
    assert!(done, "entry not found");
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(
        has(&report, Problem::OrphanedDirectory),
        "{:?}",
        report.findings
    );
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("mount");
    let lf = ufs.lookup(ufs.root(), b"lost+found").expect("lost+found");
    let mut found = false;
    ufs.readdir(lf, 0, &mut |entry, _| {
        if entry.kind == FileType::Directory && entry.name.starts_with(b"#") {
            found = true;
        }
        true
    })
    .expect("readdir");
    assert!(found, "orphan not reconnected");
}

#[test]
fn detects_unknown_directory_entry_target() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let root_db = geometry.read_ino_db(&disk.data, 2, 0);
    let base = (root_db * u64::from(geometry.fsize)) as usize;
    write_u32(&mut disk.data, base, 0xffff_fff0);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(
        has(&report, Problem::BadEntryInode),
        "{:?}",
        report.findings
    );
}

#[test]
fn detects_a_leaked_inode_and_clears_it() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    // File inode 6 ("big"): remove its directory entry and set nlink to zero.
    let root_db = geometry.read_ino_db(&disk.data, 2, 0);
    let base = (root_db * u64::from(geometry.fsize)) as usize;
    let mut off = base;
    while off < base + 512 {
        let reclen = read_u16(&disk.data, off + 4) as usize;
        let namlen = disk.data[off + 7] as usize;
        let name = &disk.data[off + 8..off + 8 + namlen];
        if name == b"big" {
            write_u32(&mut disk.data, off, 0);
            break;
        }
        if reclen == 0 {
            break;
        }
        off += reclen;
    }
    geometry.write_nlink(&mut disk.data, 6, 0);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(
        has(&report, Problem::UnreferencedInode),
        "{:?}",
        report.findings
    );
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_duplicate_blocks() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let first = geometry.read_ino_db(&disk.data, 4, 0);
    let second = geometry.read_ino_db(&disk.data, 5, 0);
    if first != second {
        geometry.write_ino_db(&mut disk.data, 5, 0, first);
    }
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(
        has(&report, Problem::DuplicateBlock),
        "{:?}",
        report.findings
    );
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn random_single_byte_mutations_never_panic() {
    let base = simple_image(8 * MIB, 2);
    let mut state = 0x1234_5678_9abc_def0u64;
    for _ in 0..2_000 {
        let mut disk = VecDisk::new(base.data.clone());
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let index = (state as usize) % disk.data.len();
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        disk.data[index] ^= (state as u8) | 1;
        let _ = check(&mut disk, Mode::ReadOnly);
    }
}

#[test]
fn missing_lost_found_is_reported_unrepairable() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    // Remove the root entry for lost+found and create an orphan directory.
    let root = geometry.dir_block(&disk.data, 2);
    let offset = find_entry(&disk.data, root, b"lost+found").expect("lost+found entry");
    write_u32(&mut disk.data, offset, 0);
    let offset = find_entry(&disk.data, root, b"dir").expect("dir entry");
    write_u32(&mut disk.data, offset, 0);
    let report = check(&mut disk, Mode::Repair).expect("repair");
    assert!(
        has(&report, Problem::LostFoundMissing),
        "{:?}",
        report.findings
    );
    assert!(!report.clean_after, "{:?}", report.findings);
}

#[test]
fn detects_and_reinserts_a_missing_dot_entry() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let base = geometry.dir_block(&disk.data, 4);
    let offset = find_entry(&disk.data, base, b".").expect("dot entry");
    write_u32(&mut disk.data, offset, 0);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(
        has(&report, Problem::MissingDotEntry),
        "{:?}",
        report.findings
    );
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_and_reinserts_a_missing_dotdot_entry() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let base = geometry.dir_block(&disk.data, 4);
    let offset = find_entry(&disk.data, base, b"..").expect("dotdot entry");
    write_u32(&mut disk.data, offset, 0);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(
        has(&report, Problem::MissingDotDotEntry),
        "{:?}",
        report.findings
    );
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_and_fixes_a_bad_dotdot_entry() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let base = geometry.dir_block(&disk.data, 4);
    let offset = find_entry(&disk.data, base, b"..").expect("dotdot entry");
    write_u32(&mut disk.data, offset, 3);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(
        has(&report, Problem::BadDotDotEntry),
        "{:?}",
        report.findings
    );
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_an_inode_bitmap_bit_flip() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let offset = geometry.cg_inode_map_offset(&disk.data, 0);
    let bit = 5u32;
    let byte = offset + (bit / 8) as usize;
    let mask = 1u8 << (bit % 8);
    disk.data[byte] &= !mask;
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(has(&report, Problem::InodeBitmap), "{:?}", report.findings);
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn detects_a_leaked_block() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let offset = geometry.cg_free_map_offset(&disk.data, 0);
    let mut bit = 1_000u32;
    while (disk.data[offset + (bit / 8) as usize] >> (bit % 8)) & 1 == 0 {
        bit += 1;
        assert!(bit < 20_000, "no free block found");
    }
    let byte = offset + (bit / 8) as usize;
    disk.data[byte] &= !(1u8 << (bit % 8));
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(has(&report, Problem::BlockBitmap), "{:?}", report.findings);
    let repair = check(&mut disk, Mode::Repair).expect("repair");
    assert!(repair.clean_after, "{:?}", repair.findings);
}

#[test]
fn ufs1_mutations_are_repaired_too() {
    let mut disk = simple_image(16 * MIB, 1);
    let geometry = Geometry::read(&disk.data, 8192);
    geometry.write_nlink(&mut disk.data, 5, 7);
    let report = check(&mut disk, Mode::Repair).expect("repair ufs1");
    assert!(has(&report, Problem::BadLinkCount));
    assert!(report.clean_after, "{:?}", report.findings);
}

#[test]
fn root_not_a_directory_is_reported() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let off = geometry.inode_offset(2);
    write_u16(&mut disk.data, off, 0o100644);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(
        has(&report, Problem::RootNotDirectory),
        "{:?}",
        report.findings
    );
}

#[test]
fn bad_directory_size_is_reported() {
    let mut disk = simple_image(16 * MIB, 2);
    let geometry = Geometry::read(&disk.data, 8192);
    let off = geometry.inode_offset(4);
    write_u64(&mut disk.data, off + 16, 513);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(
        has(&report, Problem::BadDirectorySize),
        "{:?}",
        report.findings
    );
}

#[test]
fn b8192_images_are_clean() {
    for version in [1u8, 2] {
        let size = 16 * MIB;
        let opts = MkfsOptions {
            size,
            version,
            block_size: 8192,
            fragment_size: 1024,
            bytes_per_inode: 4096,
            minfree: 5,
            label: "",
            timestamp: STAMP,
        };
        let disk = VecDisk::new(vec![0u8; size as usize]);
        let ufs = format(disk, &opts).expect("format");
        let mut disk = ufs.unmount().expect("unmount");
        let report = check(&mut disk, Mode::ReadOnly).expect("check");
        assert!(
            report.findings.is_empty(),
            "v{version}: {:?}",
            report.findings
        );
    }
}

#[test]
fn repair_then_read_only_is_clean_for_a_golden_image() {
    let mut disk = golden("ufs2-b4096");
    let report = check(&mut disk, Mode::Repair).expect("repair");
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert!(report.clean_after);
}

#[test]
fn oracle_agrees_when_present() {
    let Some(binary) = std::env::var_os("UFS_FSCK_FFS")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let dir = std::path::PathBuf::from(std::env::var_os("UFS_ORACLE_DIR")?);
            [dir.join("bin/fsck_ffs"), dir.join("fsck_ffs")]
                .into_iter()
                .find(|path| path.is_file())
        })
    else {
        println!("ORACLE ABSENT: fsck_ffs not installed; skipped");
        return;
    };
    for version in [1u8, 2] {
        for size in [8 * MIB, 16 * MIB, 64 * MIB] {
            let _ = (version, size);
        }
    }
    let mut disk = simple_image(16 * MIB, 2);
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(report.clean_after);
    let dir = std::env::temp_dir().join(format!("ufs-check-oracle-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("image.img");
    std::fs::write(&path, &disk.data).expect("write image");
    let output = std::process::Command::new(binary)
        .arg("-n")
        .arg(&path)
        .output()
        .expect("fsck_ffs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "fsck_ffs disagreed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
