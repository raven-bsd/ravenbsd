// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Behaviour follows NetBSD sbin/fsck_ffs (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01). B63 marks those files
// reference-only, so this module is written from the on-disk definitions and
// the observed phase behaviour; no fsck_ffs code is copied.

//! Filesystem check and repair in the classic FFS phase order.
//!
//! [`check`] scans a UFS1/UFS2 image through a [`BlockIo`] device. In
//! [`Mode::ReadOnly`] it reports every inconsistency and never writes; in
//! [`Mode::Repair`] it applies the repairs that upstream's preen mode applies
//! and verifies the result with a second read-only pass.
//!
//! The phases are:
//! 1. inode type, size and block pointers (with duplicate and out-of-range
//!    detection);
//! 2. directory entries, `.` and `..`, and entries that point at free inodes;
//! 3. connectivity: unreferenced directories are reconnected to `/lost+found`;
//! 4. link counts and unreferenced inodes;
//! 5. cylinder-group bitmaps, free counters and the superblock summary.

use alloc_crate::vec;
use alloc_crate::vec::Vec;

use crate::blockio::BlockIo;
use crate::dinode::Inode;
use crate::dir;
use crate::dirwrite::{dirsiz, dtype};
use crate::error::UfsError;
use crate::ffs_bitmap;
use crate::ffs_vfsops;
use crate::fs::{
    Csum, Csum32, FFS_MAXNAMLEN, FS_FLAGS_UPDATED, FS_ISCLEAN, Superblock, UFS_DIRBLKSIZ,
    UFS_ROOTINO,
};
use crate::types::{FileType, Ino, Timespec};
use crate::ufs_bmap;

const UFS_NDADDR: usize = 12;
const UFS_NIADDR: usize = 3;
const UFS_MAXDIRSIZE: u64 = (1 << 31) - 1;

const CG_MAGIC: u32 = 0x0009_0255;
const CG_MAGIC_OFF: usize = 4;
const CG_OLD_TIME_OFF: usize = 8;
const CG_CGX_OFF: usize = 12;
const CG_NDBLK_OFF: usize = 20;
const CG_CS_OFF: usize = 24;
const CG_ROTOR_OFF: usize = 40;
const CG_FROTOR_OFF: usize = 44;
const CG_IROTOR_OFF: usize = 48;
const CG_FRSUM_OFF: usize = 52;
const CG_OLD_BTOTOFF_OFF: usize = 84;
const CG_OLD_BOOFF_OFF: usize = 88;
const CG_IUSEDOFF_OFF: usize = 92;
const CG_FREEOFF_OFF: usize = 96;
const CG_NEXTFREEOFF_OFF: usize = 100;
const CG_CLUSTERSUMOFF_OFF: usize = 104;
const CG_CLUSTEROFF_OFF: usize = 108;
const CG_NCLUSTERBLKS_OFF: usize = 112;
const CG_NIBLK_OFF: usize = 116;
const CG_INITEDIBLK_OFF: usize = 120;
const CG_TIME_OFF: usize = 136;
const CG_UFS1_NIBLK_OFF: usize = 18;

/// How [`check`] treats the filesystem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Report findings only; the device is never written.
    ReadOnly,
    /// Repair what preen mode repairs and verify the result.
    Repair,
}

/// One class of inconsistency found by [`check`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Problem {
    /// An inode with no type but non-zero contents.
    PartiallyAllocatedInode,
    /// An inode with an unknown file-type field.
    UnknownInodeType,
    /// An inode whose size or block count is outside the filesystem limits.
    BadInodeSize,
    /// A directory whose size is not a multiple of the directory block size.
    BadDirectorySize,
    /// A block pointer outside the data area or in reserved metadata.
    BadBlock,
    /// A block referenced by more than one inode.
    DuplicateBlock,
    /// A block pointer beyond the size of the file.
    PartiallyTruncatedInode,
    /// The inode block count does not match the allocated blocks.
    IncorrectBlockCount,
    /// A malformed directory block or record.
    BadDirectoryEntry,
    /// A directory entry pointing outside the inode range or at a cleared inode.
    BadEntryInode,
    /// A directory entry pointing at an unallocated inode.
    UnallocatedEntry,
    /// A directory entry whose `d_type` disagrees with the inode.
    BadEntryType,
    /// A duplicate `.` or `..` entry.
    ExtraDotEntry,
    /// A `.` entry that does not point at its directory.
    BadDotEntry,
    /// A directory without a `.` entry.
    MissingDotEntry,
    /// A `..` entry that does not point at the parent.
    BadDotDotEntry,
    /// A directory without a `..` entry.
    MissingDotDotEntry,
    /// A directory with more than one parent entry.
    ExtraneousDirectoryLink,
    /// A directory that is not reachable from the root.
    OrphanedDirectory,
    /// An allocated inode that no directory entry references.
    UnreferencedInode,
    /// The stored link count differs from the number of references.
    BadLinkCount,
    /// The cylinder-group inode bitmap differs from the scan.
    InodeBitmap,
    /// The cylinder-group free-block bitmap differs from the scan.
    BlockBitmap,
    /// A cylinder-group header or counter is wrong.
    CylinderGroupSummary,
    /// The superblock's aggregate summary counters are wrong.
    SuperblockSummary,
    /// The root inode is not a directory.
    RootNotDirectory,
    /// `/lost+found` is missing or is not a directory.
    LostFoundMissing,
    /// An inode could not be read or decoded.
    BadInode,
}

/// One finding, with the inode and/or block it concerns when known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// Phase number (1, 2, 3, 4 or 5) that produced the finding.
    pub phase: u8,
    /// What is wrong.
    pub problem: Problem,
    /// Inode the finding concerns.
    pub inode: Option<Ino>,
    /// Block fragment or directory byte offset the finding concerns.
    pub block: Option<u64>,
}

/// Result of a [`check`] run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// Every inconsistency found during the first pass.
    pub findings: Vec<Finding>,
    /// The subset of findings repaired (empty in [`Mode::ReadOnly`]).
    pub fixed: Vec<Finding>,
    /// Whether a read-only pass over the resulting image is clean.
    pub clean_after: bool,
}

impl Report {
    /// Whether the first pass found nothing.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Check (and optionally repair) a UFS1/UFS2 filesystem.
pub fn check<D: BlockIo>(disk: &mut D, mode: Mode) -> Result<Report, UfsError> {
    let mut checker = Checker::new(disk, mode)?;
    checker.run()?;
    let (findings, fixed) = checker.finish();

    if mode == Mode::ReadOnly {
        let clean_after = findings.is_empty();
        return Ok(Report {
            findings,
            fixed,
            clean_after,
        });
    }

    let mut verify = Checker::new(disk, Mode::ReadOnly)?;
    verify.run()?;
    let (mut residual, _) = verify.finish();
    let clean_after = residual.is_empty();
    let mut findings = findings;
    findings.append(&mut residual);

    let mut fs = ffs_vfsops::probe(disk)?;
    fs.set_clean_raw(if clean_after { FS_ISCLEAN } else { 0 });
    fs.clear_fmod_raw();
    ffs_vfsops::write_superblock(disk, &fs)?;
    disk.flush().map_err(|_| UfsError::Io)?;

    Ok(Report {
        findings,
        fixed,
        clean_after,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InoState {
    Free,
    File,
    Dir,
    Clear,
}

#[derive(Clone, Copy, Debug)]
struct InodeState {
    state: InoState,
    kind: FileType,
    nlink: i32,
}

impl InodeState {
    const fn free() -> Self {
        Self {
            state: InoState::Free,
            kind: FileType::Regular,
            nlink: 0,
        }
    }
}

struct IndirectWalk {
    ino: Ino,
    size: u64,
    next_lbn: u64,
    expected: u64,
    clear: bool,
}

#[derive(Clone, Debug)]
struct RawInode {
    mode: u16,
    nlink: i32,
    uid: u32,
    gid: u32,
    size: u64,
    blocks: u64,
    flags: u32,
    db: [u64; UFS_NDADDR],
    ib: [u64; UFS_NIADDR],
}

impl RawInode {
    fn is_empty(&self) -> bool {
        self.mode == 0
            && self.size == 0
            && self.blocks == 0
            && self.db.iter().all(|value| *value == 0)
            && self.ib.iter().all(|value| *value == 0)
    }

    fn file_type(&self) -> Option<FileType> {
        match self.mode & 0o170000 {
            0o100000 => Some(FileType::Regular),
            0o040000 => Some(FileType::Directory),
            0o120000 => Some(FileType::Symlink),
            0o020000 => Some(FileType::CharDevice),
            0o060000 => Some(FileType::BlockDevice),
            0o010000 => Some(FileType::Fifo),
            0o140000 => Some(FileType::Socket),
            _ => None,
        }
    }

    fn as_inode(&self) -> Inode {
        Inode {
            raw: Vec::new(),
            mode: self.mode,
            nlink: self.nlink.max(0) as u32,
            uid: self.uid,
            gid: self.gid,
            size: self.size,
            blocks: self.blocks,
            atime: Timespec::default(),
            mtime: Timespec::default(),
            ctime: Timespec::default(),
            flags: self.flags,
            r#gen: 0,
            rdev: 0,
            db: self.db,
            ib: self.ib,
        }
    }
}

struct Checker<'a, D: BlockIo> {
    disk: &'a mut D,
    fs: Superblock,
    repair: bool,
    states: Vec<InodeState>,
    parents: Vec<Ino>,
    dotdots: Vec<Ino>,
    dotdot_offsets: Vec<u64>,
    refs: Vec<u32>,
    owners: Vec<u16>,
    reserved: Vec<u8>,
    findings: Vec<Finding>,
    fixed: Vec<Finding>,
    inode_cache: Option<(u64, Vec<u8>)>,
    lf: Option<Ino>,
    maxino: Ino,
}

impl<'a, D: BlockIo> Checker<'a, D> {
    fn new(disk: &'a mut D, mode: Mode) -> Result<Self, UfsError> {
        let fs = ffs_vfsops::probe(disk)?;
        ffs_vfsops::validate(&fs, disk.size_bytes())?;
        if !fs.ufs2 && fs.old_flags & FS_FLAGS_UPDATED == 0 {
            return Err(UfsError::Unsupported("old UFS1 cylinder groups"));
        }
        let maxino_u64 = u64::from(fs.ncg)
            .checked_mul(u64::from(fs.ipg))
            .ok_or(UfsError::BadSuperblock("inode count overflow"))?;
        let max_possible = fs
            .size
            .checked_mul(u64::from(fs.bsize))
            .and_then(|value| value.checked_div(fs.inode_size() as u64))
            .ok_or(UfsError::BadSuperblock("inode count overflow"))?;
        if maxino_u64 > max_possible.saturating_add(1) || maxino_u64 > u64::from(u32::MAX) {
            return Err(UfsError::BadSuperblock("too many inodes"));
        }
        let maxino = u32::try_from(maxino_u64).map_err(|_| UfsError::BadSuperblock("inodes"))?;
        let frags = usize::try_from(fs.size).map_err(|_| UfsError::BadSuperblock("size"))?;
        let inodes = usize::try_from(maxino).map_err(|_| UfsError::BadSuperblock("inodes"))?;
        let mut checker = Self {
            disk,
            fs,
            repair: mode == Mode::Repair,
            states: vec![InodeState::free(); inodes],
            parents: vec![0; inodes],
            dotdots: vec![0; inodes],
            dotdot_offsets: vec![0; inodes],
            refs: vec![0; inodes],
            owners: vec![0; frags],
            reserved: vec![0; frags.div_ceil(8)],
            findings: Vec::new(),
            fixed: Vec::new(),
            inode_cache: None,
            lf: None,
            maxino,
        };
        checker.reserve_metadata()?;
        Ok(checker)
    }

    fn run(&mut self) -> Result<(), UfsError> {
        self.reserve_metadata()?;
        self.scan_inodes()?;
        self.scan_directories()?;
        self.fix_dotdots()?;
        self.reconnect_orphans()?;
        self.fix_links_and_clear()?;
        self.rebuild_metadata()?;
        Ok(())
    }

    fn finish(self) -> (Vec<Finding>, Vec<Finding>) {
        (self.findings, self.fixed)
    }

    // --- reporting ---------------------------------------------------------

    fn finding(
        &mut self,
        phase: u8,
        problem: Problem,
        inode: Option<Ino>,
        block: Option<u64>,
    ) -> Finding {
        let finding = Finding {
            phase,
            problem,
            inode,
            block,
        };
        self.findings.push(finding.clone());
        finding
    }

    fn mark_fixed(&mut self, finding: &Finding) {
        self.fixed.push(finding.clone());
    }

    // --- block map helpers -------------------------------------------------

    fn reserve_metadata(&mut self) -> Result<(), UfsError> {
        for frag in self.reserved.iter_mut() {
            *frag = 0;
        }
        for cg in 0..self.fs.ncg {
            let start = if cg == 0 {
                self.fs.cgbase(cg)?
            } else {
                self.fs
                    .cgstart(cg)?
                    .checked_add(self.fs.sblkno as u64)
                    .ok_or(UfsError::BadSuperblock("cylinder group"))?
            };
            let end = self
                .fs
                .cgstart(cg)?
                .checked_add(self.fs.dblkno as u64)
                .ok_or(UfsError::BadSuperblock("cylinder group"))?;
            for frag in start..end {
                if frag < self.fs.size {
                    self.set_reserved(frag)?;
                }
            }
        }
        let start = u64::from(self.fs.csaddr);
        let summary_frags = u64::from(self.fs.cssize).div_ceil(u64::from(self.fs.fsize));
        let end = start
            .checked_add(summary_frags)
            .ok_or(UfsError::BadSuperblock("summary area"))?;
        for frag in start..end {
            if frag < self.fs.size {
                self.set_reserved(frag)?;
            }
        }
        Ok(())
    }

    fn set_reserved(&mut self, frag: u64) -> Result<(), UfsError> {
        let index = usize::try_from(frag).map_err(|_| UfsError::Corrupt("block index"))?;
        ffs_bitmap::set_bit(&mut self.reserved, index)
    }

    fn is_reserved(&self, frag: u64) -> bool {
        usize::try_from(frag)
            .ok()
            .map(|index| ffs_bitmap::bit_is_set(&self.reserved, index))
            .unwrap_or(true)
    }

    fn valid_fragment(&self, frag: u64) -> bool {
        frag > 0 && frag < self.fs.size && !self.is_reserved(frag)
    }

    fn reserve_block(&mut self, frag: u64) {
        if let Some(slot) = self.owners.get_mut(frag as usize) {
            *slot = slot.saturating_add(1);
        }
    }

    fn release_block(&mut self, frag: u64) {
        if let Some(slot) = self.owners.get_mut(frag as usize) {
            *slot = slot.saturating_sub(1);
        }
    }

    // --- inode access ------------------------------------------------------

    fn read_inode(&mut self, ino: Ino) -> Result<RawInode, UfsError> {
        let (fsba, slot) = self.fs.ino_to_fsba(ino)?;
        let inode_size = self.fs.inode_size();
        let need = fsba
            != self
                .inode_cache
                .as_ref()
                .map(|(key, _)| *key)
                .unwrap_or(u64::MAX);
        if need {
            let offset = self.fs.frag_offset(fsba)?;
            let end = offset
                .checked_add(u64::from(self.fs.bsize))
                .ok_or(UfsError::Corrupt("inode block"))?;
            if end > self.disk.size_bytes() {
                return Err(UfsError::Corrupt("inode block beyond device"));
            }
            let mut data = vec![0u8; self.fs.bsize as usize];
            self.disk
                .read_at(offset, &mut data)
                .map_err(|_| UfsError::Io)?;
            self.inode_cache = Some((fsba, data));
        }
        let data = self
            .inode_cache
            .as_ref()
            .map(|(_, data)| data.as_slice())
            .ok_or(UfsError::Corrupt("inode cache"))?;
        let offset = slot
            .checked_mul(inode_size)
            .ok_or(UfsError::Corrupt("inode slot"))?;
        let end = offset
            .checked_add(inode_size)
            .ok_or(UfsError::Corrupt("inode slot"))?;
        let raw = data
            .get(offset..end)
            .ok_or(UfsError::Corrupt("inode outside block"))?;
        decode_inode(self.fs.ufs2, raw)
    }

    fn write_inode(&mut self, ino: Ino, inode: &RawInode) -> Result<(), UfsError> {
        let (fsba, slot) = self.fs.ino_to_fsba(ino)?;
        let inode_size = self.fs.inode_size();
        let offset = self
            .fs
            .frag_offset(fsba)?
            .checked_add((slot * inode_size) as u64)
            .ok_or(UfsError::Corrupt("inode offset"))?;
        let mut raw = vec![0u8; inode_size];
        if fsba
            == self
                .inode_cache
                .as_ref()
                .map(|(key, _)| *key)
                .unwrap_or(u64::MAX)
        {
            if let Some((_, data)) = self.inode_cache.as_ref() {
                let start = slot * inode_size;
                if let Some(src) = data.get(start..start + inode_size) {
                    raw.copy_from_slice(src);
                }
            }
        } else {
            self.disk
                .read_at(offset, &mut raw)
                .map_err(|_| UfsError::Io)?;
        }
        encode_inode(self.fs.ufs2, &mut raw, inode)?;
        self.disk.write_at(offset, &raw).map_err(|_| UfsError::Io)?;
        if let Some((key, data)) = self.inode_cache.as_mut()
            && *key == fsba
        {
            let start = slot * inode_size;
            if let Some(dst) = data.get_mut(start..start + inode_size) {
                dst.copy_from_slice(&raw);
            }
        }
        Ok(())
    }

    fn clear_inode_raw(&mut self, ino: Ino) -> Result<(), UfsError> {
        let (fsba, slot) = self.fs.ino_to_fsba(ino)?;
        let inode_size = self.fs.inode_size();
        let offset = self
            .fs
            .frag_offset(fsba)?
            .checked_add((slot * inode_size) as u64)
            .ok_or(UfsError::Corrupt("inode offset"))?;
        let zero = vec![0u8; inode_size];
        self.disk
            .write_at(offset, &zero)
            .map_err(|_| UfsError::Io)?;
        if let Some((key, data)) = self.inode_cache.as_mut()
            && *key == fsba
        {
            let start = slot * inode_size;
            if let Some(dst) = data.get_mut(start..start + inode_size) {
                dst.fill(0);
            }
        }
        Ok(())
    }

    // --- block walking -----------------------------------------------------

    fn block_frags(&self, size: u64, lbn: u64) -> u32 {
        let offset = lbn.saturating_mul(u64::from(self.fs.bsize));
        if offset >= size {
            return 0;
        }
        let len = (size - offset).min(u64::from(self.fs.bsize));
        u32::try_from(len.div_ceil(u64::from(self.fs.fsize)))
            .unwrap_or(self.fs.frag)
            .min(self.fs.frag)
    }

    fn account_block(
        &mut self,
        ino: Ino,
        frag: u64,
        count: u32,
        expected: &mut u64,
        clear: &mut bool,
    ) {
        *expected = expected.saturating_add(u64::from(count));
        let mut bad = false;
        let mut duplicate = false;
        for index in 0..u64::from(count) {
            let block = frag.saturating_add(index);
            if !self.valid_fragment(block) {
                bad = true;
                continue;
            }
            if self.owners.get(block as usize).copied().unwrap_or(0) > 0 {
                duplicate = true;
            }
            self.reserve_block(block);
        }
        if bad {
            self.finding(1, Problem::BadBlock, Some(ino), Some(frag));
            *clear = true;
        }
        if duplicate {
            self.finding(1, Problem::DuplicateBlock, Some(ino), Some(frag));
            *clear = true;
        }
    }

    fn release_extent(&mut self, frag: u64, count: u32) {
        for index in 0..u64::from(count) {
            self.release_block(frag.saturating_add(index));
        }
    }

    fn inode_bmap(&mut self, ip: &RawInode, lbn: u64) -> Result<Option<u64>, UfsError> {
        let plan = ufs_bmap::plan(lbn, self.fs.nindir())?;
        let disk = &mut *self.disk;
        let fs = &self.fs;
        let mut read_ptr = |frag: u64, index: u32| read_pointer(disk, fs, frag, index);
        let inode = ip.as_inode();
        ufs_bmap::follow(&inode, plan, &mut read_ptr)
    }

    fn walk_indirect(
        &mut self,
        state: &mut IndirectWalk,
        level: u8,
        block: u64,
        needed: u64,
    ) -> Result<(), UfsError> {
        let ino = state.ino;
        if !self.valid_fragment(block) {
            self.finding(1, Problem::BadBlock, Some(ino), Some(block));
            state.clear = true;
            return Ok(());
        }
        self.account_block(
            ino,
            block,
            self.fs.frag,
            &mut state.expected,
            &mut state.clear,
        );

        let data = match self.read_fs_block(block) {
            Ok(data) => data,
            Err(_) => {
                self.finding(1, Problem::BadBlock, Some(ino), Some(block));
                state.clear = true;
                return Ok(());
            }
        };
        let nindir = self.fs.nindir();
        let child_capacity = nindir.saturating_pow(u32::from(level) - 1);
        let needed_children = needed.div_ceil(child_capacity).min(nindir);
        let mut remaining = needed;
        let mut extra: Option<Finding> = None;
        for index in 0..nindir {
            let ptr = read_pointer_from(&data, index, self.fs.ptr_size())?;
            if index < needed_children {
                let child_needed = remaining.min(child_capacity);
                if ptr != 0 {
                    if level == 1 {
                        let frags = self.block_frags(state.size, state.next_lbn);
                        self.account_block(ino, ptr, frags, &mut state.expected, &mut state.clear);
                        state.next_lbn = state.next_lbn.saturating_add(1);
                    } else {
                        self.walk_indirect(state, level - 1, ptr, child_needed)?;
                    }
                } else if level == 1 {
                    state.next_lbn = state.next_lbn.saturating_add(1);
                } else {
                    state.next_lbn = state.next_lbn.saturating_add(child_needed);
                }
                remaining = remaining.saturating_sub(child_needed);
            } else if ptr != 0 && extra.is_none() {
                extra =
                    Some(self.finding(1, Problem::PartiallyTruncatedInode, Some(ino), Some(block)));
            }
        }
        if let Some(finding) = extra
            && self.repair
        {
            let mut updated = data;
            for index in needed_children..nindir {
                if read_pointer_from(&updated, index, self.fs.ptr_size())? != 0 {
                    write_pointer_to(&mut updated, index, 0, self.fs.ptr_size())?;
                }
            }
            self.write_fs_block(block, &updated)?;
            self.mark_fixed(&finding);
        }
        Ok(())
    }

    fn walk_indirect_release(
        &mut self,
        level: u8,
        block: u64,
        needed: u64,
        next_lbn: &mut u64,
    ) -> Result<(), UfsError> {
        if !self.valid_fragment(block) {
            return Ok(());
        }
        let data = match self.read_fs_block(block) {
            Ok(data) => data,
            Err(_) => return Ok(()),
        };
        let nindir = self.fs.nindir();
        let child_capacity = nindir.saturating_pow(u32::from(level) - 1);
        let needed_children = needed.div_ceil(child_capacity).min(nindir);
        let mut remaining = needed;
        for index in 0..needed_children {
            let ptr = read_pointer_from(&data, index, self.fs.ptr_size())?;
            let child_needed = remaining.min(child_capacity);
            if ptr != 0 {
                if level == 1 {
                    self.release_block(ptr);
                    *next_lbn = next_lbn.saturating_add(1);
                } else {
                    self.walk_indirect_release(level - 1, ptr, child_needed, next_lbn)?;
                }
            } else if level == 1 {
                *next_lbn = next_lbn.saturating_add(1);
            } else {
                *next_lbn = next_lbn.saturating_add(child_needed);
            }
            remaining = remaining.saturating_sub(child_needed);
        }
        Ok(())
    }

    // --- phase 1 -----------------------------------------------------------

    fn scan_inodes(&mut self) -> Result<(), UfsError> {
        for ino in 2..self.maxino {
            let inode = match self.read_inode(ino) {
                Ok(inode) => inode,
                Err(_) => {
                    self.finding(1, Problem::BadInode, Some(ino), None);
                    continue;
                }
            };
            if inode.is_empty() {
                self.states[ino as usize] = InodeState::free();
                continue;
            }
            if inode.mode & 0o170000 == 0 && inode.nlink == 0 {
                let finding = self.finding(1, Problem::PartiallyAllocatedInode, Some(ino), None);
                if self.repair {
                    self.clear_inode_raw(ino)?;
                    self.states[ino as usize] = InodeState::free();
                    self.mark_fixed(&finding);
                }
                continue;
            }
            let Some(kind) = inode.file_type() else {
                let finding = self.finding(1, Problem::UnknownInodeType, Some(ino), None);
                if self.repair {
                    self.clear_inode_raw(ino)?;
                    self.states[ino as usize] = InodeState::free();
                    self.mark_fixed(&finding);
                } else {
                    self.states[ino as usize] = InodeState {
                        state: InoState::Clear,
                        kind: FileType::Regular,
                        nlink: inode.nlink,
                    };
                }
                continue;
            };
            let mut state = InodeState {
                state: if kind == FileType::Directory {
                    InoState::Dir
                } else {
                    InoState::File
                },
                kind,
                nlink: inode.nlink,
            };
            self.states[ino as usize] = state;

            let mut clear = false;
            let mut skip_blocks = false;
            let mut expected = 0u64;
            if inode.size > self.max_file_size() {
                let finding = self.finding(1, Problem::BadInodeSize, Some(ino), None);
                clear = true;
                skip_blocks = true;
                if self.repair {
                    self.mark_fixed(&finding);
                }
            }
            if kind == FileType::Directory && (inode.size == 0 || inode.size > UFS_MAXDIRSIZE) {
                let finding = self.finding(1, Problem::BadInodeSize, Some(ino), None);
                clear = true;
                skip_blocks = true;
                if self.repair {
                    self.mark_fixed(&finding);
                }
            }
            if kind == FileType::Directory && !inode.size.is_multiple_of(UFS_DIRBLKSIZ as u64) {
                // Rounding the size up without allocating the new space could make
                // fsck read a neighbouring file as directory data, so leave it for
                // a human and report it again in the verification pass.
                self.finding(1, Problem::BadDirectorySize, Some(ino), None);
            }

            let fast_symlink = kind == FileType::Symlink
                && (inode.size < self.fs.maxsymlinklen as u64
                    || (self.fs.maxsymlinklen == 0 && inode.blocks == 0));
            let special = matches!(
                kind,
                FileType::CharDevice | FileType::BlockDevice | FileType::Fifo | FileType::Socket
            );
            let mut inode = inode;
            if skip_blocks {
                state.state = InoState::Clear;
                self.states[ino as usize] = state;
                continue;
            }
            if !fast_symlink && !special {
                let ndb = inode.size.div_ceil(u64::from(self.fs.bsize));
                let mut next_lbn = 0u64;
                for index in 0..UFS_NDADDR {
                    if next_lbn >= ndb {
                        if inode.db[index] != 0 {
                            let finding = self.finding(
                                1,
                                Problem::PartiallyTruncatedInode,
                                Some(ino),
                                Some(inode.db[index]),
                            );
                            if self.repair {
                                inode.db[index] = 0;
                                self.write_inode(ino, &inode)?;
                                self.mark_fixed(&finding);
                            }
                        }
                        continue;
                    }
                    let frags = self.block_frags(inode.size, next_lbn);
                    if inode.db[index] != 0 {
                        self.account_block(ino, inode.db[index], frags, &mut expected, &mut clear);
                    } else if kind == FileType::Directory {
                        let finding = self.finding(
                            1,
                            Problem::BadDirectoryEntry,
                            Some(ino),
                            Some(next_lbn * u64::from(self.fs.bsize)),
                        );
                        if self.repair {
                            self.mark_fixed(&finding);
                        }
                    }
                    next_lbn += 1;
                }
                let mut remaining = ndb.saturating_sub(UFS_NDADDR as u64);
                let mut capacity = self.fs.nindir();
                let mut needed_levels = 0u8;
                let mut need = remaining;
                while need > 0 && needed_levels < UFS_NIADDR as u8 {
                    needed_levels += 1;
                    need = need.saturating_sub(capacity);
                    capacity = capacity.saturating_mul(self.fs.nindir());
                }
                let mut walk = IndirectWalk {
                    ino,
                    size: inode.size,
                    next_lbn,
                    expected,
                    clear,
                };
                for index in 0..UFS_NIADDR {
                    let level = (index + 1) as u8;
                    let used = level <= needed_levels;
                    if used {
                        let child_capacity = self.fs.nindir().saturating_pow(u32::from(level));
                        let child_needed = remaining.min(child_capacity);
                        if inode.ib[index] != 0 {
                            self.walk_indirect(&mut walk, level, inode.ib[index], child_needed)?;
                        } else if kind == FileType::Directory {
                            let finding = self.finding(
                                1,
                                Problem::BadDirectoryEntry,
                                Some(ino),
                                Some(walk.next_lbn * u64::from(self.fs.bsize)),
                            );
                            if self.repair {
                                self.mark_fixed(&finding);
                            }
                        }
                        remaining = remaining.saturating_sub(child_needed);
                    } else if inode.ib[index] != 0 {
                        let finding = self.finding(
                            1,
                            Problem::PartiallyTruncatedInode,
                            Some(ino),
                            Some(inode.ib[index]),
                        );
                        if self.repair {
                            inode.ib[index] = 0;
                            self.write_inode(ino, &inode)?;
                            self.mark_fixed(&finding);
                        }
                    }
                }
                expected = walk.expected;
                clear = walk.clear;
                let blocks = expected.saturating_mul(u64::from(self.fs.fsize / 512));
                if inode.blocks != blocks {
                    let finding =
                        self.finding(1, Problem::IncorrectBlockCount, Some(ino), Some(blocks));
                    if self.repair {
                        inode.blocks = blocks;
                        self.write_inode(ino, &inode)?;
                        self.mark_fixed(&finding);
                    }
                }
            }

            if clear {
                state.state = InoState::Clear;
            }
            self.states[ino as usize] = state;
        }
        if self
            .states
            .get(UFS_ROOTINO as usize)
            .map(|state| state.state)
            != Some(InoState::Dir)
        {
            self.finding(2, Problem::RootNotDirectory, Some(UFS_ROOTINO), None);
        }
        Ok(())
    }

    fn max_file_size(&self) -> u64 {
        read_super_u64(&self.fs.raw, 1328).unwrap_or_else(|_| {
            let bsize = u64::from(self.fs.bsize);
            let nindir = self.fs.nindir();
            let mut total = bsize * UFS_NDADDR as u64;
            let mut per_block = bsize;
            for _ in 0..3 {
                per_block = per_block.saturating_mul(nindir);
                total = total.saturating_add(per_block);
            }
            total.saturating_sub(1)
        })
    }

    // --- phase 2 -----------------------------------------------------------

    fn scan_directories(&mut self) -> Result<(), UfsError> {
        for ino in 2..self.maxino {
            if self.states[ino as usize].state != InoState::Dir {
                continue;
            }
            let inode = self.read_inode(ino)?;
            if inode.size == 0 || !inode.size.is_multiple_of(UFS_DIRBLKSIZ as u64) {
                continue;
            }
            let mut dot_seen = false;
            let mut dotdot_seen = false;
            let bsize = u64::from(self.fs.bsize);
            let mut cached_lbn = u64::MAX;
            let mut cached_frag = 0u64;
            let mut data: Vec<u8> = Vec::new();
            let mut dirty = false;
            let mut offset = 0u64;
            while offset < inode.size {
                let lbn = offset / bsize;
                if lbn != cached_lbn {
                    if dirty {
                        self.write_extent(cached_frag, &data)?;
                        dirty = false;
                    }
                    cached_lbn = lbn;
                    data.clear();
                    cached_frag = 0;
                    match self.inode_bmap(&inode, lbn)? {
                        Some(frag) => {
                            let cover = self.block_cover(&inode, lbn)?;
                            match self.read_extent(frag, cover) {
                                Ok(read) => {
                                    cached_frag = frag;
                                    data = read;
                                }
                                Err(_) => {
                                    let finding = self.finding(
                                        2,
                                        Problem::BadDirectoryEntry,
                                        Some(ino),
                                        Some(offset),
                                    );
                                    if self.repair {
                                        self.mark_fixed(&finding);
                                    }
                                }
                            }
                        }
                        None => {
                            let finding = self.finding(
                                2,
                                Problem::BadDirectoryEntry,
                                Some(ino),
                                Some(offset),
                            );
                            if self.repair {
                                self.mark_fixed(&finding);
                            }
                        }
                    }
                }
                let block_start = (offset % bsize) as usize;
                if cached_frag == 0 || block_start + UFS_DIRBLKSIZ > data.len() {
                    let finding =
                        self.finding(2, Problem::BadDirectoryEntry, Some(ino), Some(offset));
                    if self.repair {
                        self.mark_fixed(&finding);
                    }
                    offset += UFS_DIRBLKSIZ as u64;
                    continue;
                }
                let mut live_index = 0usize;
                let mut within = 0usize;
                while within < UFS_DIRBLKSIZ {
                    let start = block_start + within;
                    let entry = match dir::parse(&data[start..], UFS_DIRBLKSIZ - within) {
                        Ok(Some(entry)) => entry,
                        Ok(None) => break,
                        Err(_) => {
                            let finding = self.finding(
                                2,
                                Problem::BadDirectoryEntry,
                                Some(ino),
                                Some(offset + within as u64),
                            );
                            if self.repair {
                                self.mark_fixed(&finding);
                            }
                            break;
                        }
                    };
                    let next = within
                        .checked_add(entry.next)
                        .ok_or(UfsError::Corrupt("directory offset"))?;
                    if next > UFS_DIRBLKSIZ {
                        let finding = self.finding(
                            2,
                            Problem::BadDirectoryEntry,
                            Some(ino),
                            Some(offset + within as u64),
                        );
                        if self.repair {
                            self.mark_fixed(&finding);
                        }
                        break;
                    }
                    if entry.ino == 0 {
                        within = next;
                        continue;
                    }
                    let absolute = offset + within as u64;
                    let mut target = entry.ino;
                    let mut keep = target < self.maxino
                        && target >= UFS_ROOTINO
                        && self.states[target as usize].state != InoState::Free
                        && self.states[target as usize].state != InoState::Clear;
                    if target >= self.maxino || target < UFS_ROOTINO {
                        let finding =
                            self.finding(2, Problem::BadEntryInode, Some(ino), Some(absolute));
                        keep = false;
                        if self.repair {
                            self.mark_fixed(&finding);
                        }
                    } else if self.states[target as usize].state == InoState::Free {
                        let finding =
                            self.finding(2, Problem::UnallocatedEntry, Some(ino), Some(absolute));
                        keep = false;
                        if self.repair {
                            self.mark_fixed(&finding);
                        }
                    } else if self.states[target as usize].state == InoState::Clear {
                        let finding =
                            self.finding(2, Problem::BadEntryInode, Some(ino), Some(absolute));
                        keep = false;
                        if self.repair {
                            self.mark_fixed(&finding);
                        }
                    }
                    if keep {
                        if entry.name == b"." {
                            if target != ino {
                                let finding = self.finding(
                                    2,
                                    Problem::BadDotEntry,
                                    Some(ino),
                                    Some(absolute),
                                );
                                if self.repair {
                                    write_u32_at(&mut data, start, ino)?;
                                    dirty = true;
                                    target = ino;
                                    self.mark_fixed(&finding);
                                } else {
                                    keep = false;
                                }
                            } else if dot_seen {
                                let finding = self.finding(
                                    2,
                                    Problem::ExtraDotEntry,
                                    Some(ino),
                                    Some(absolute),
                                );
                                if self.repair {
                                    write_u32_at(&mut data, start, 0)?;
                                    dirty = true;
                                    self.mark_fixed(&finding);
                                }
                                keep = false;
                            } else {
                                dot_seen = true;
                            }
                        } else if entry.name == b".." {
                            if dotdot_seen {
                                let finding = self.finding(
                                    2,
                                    Problem::ExtraDotEntry,
                                    Some(ino),
                                    Some(absolute),
                                );
                                if self.repair {
                                    write_u32_at(&mut data, start, 0)?;
                                    dirty = true;
                                    self.mark_fixed(&finding);
                                }
                                keep = false;
                            } else {
                                dotdot_seen = true;
                                self.dotdots[ino as usize] = target;
                                self.dotdot_offsets[ino as usize] = absolute;
                            }
                        } else if self.states[target as usize].kind == FileType::Directory {
                            let parent = self.parents[target as usize];
                            if parent != 0 && parent != ino {
                                let finding = self.finding(
                                    2,
                                    Problem::ExtraneousDirectoryLink,
                                    Some(ino),
                                    Some(absolute),
                                );
                                keep = false;
                                if self.repair {
                                    self.mark_fixed(&finding);
                                }
                            } else {
                                self.parents[target as usize] = ino;
                            }
                        }
                        if keep {
                            let target_kind = self.states[target as usize].kind;
                            if entry.dtype != 0 && entry.dtype != dtype(target_kind) {
                                let finding = self.finding(
                                    2,
                                    Problem::BadEntryType,
                                    Some(ino),
                                    Some(absolute),
                                );
                                if self.repair {
                                    if let Some(slot) = data.get_mut(start + 6) {
                                        *slot = dtype(target_kind);
                                        dirty = true;
                                    }
                                    self.mark_fixed(&finding);
                                }
                            }
                            if let Some(slot) = self.refs.get_mut(target as usize) {
                                *slot = slot.saturating_add(1);
                            }
                        }
                    }
                    if !keep && self.repair {
                        write_u32_at(&mut data, start, 0)?;
                        dirty = true;
                    }
                    if keep {
                        live_index += 1;
                    }
                    within = next;
                }
                let _ = live_index;
                offset += UFS_DIRBLKSIZ as u64;
            }
            if dirty {
                self.write_extent(cached_frag, &data)?;
            }
            if !dot_seen {
                let finding = self.finding(2, Problem::MissingDotEntry, Some(ino), None);
                if self.repair
                    && self.insert_entry(ino, &inode, b".", ino, dtype(FileType::Directory))?
                {
                    if let Some(slot) = self.refs.get_mut(ino as usize) {
                        *slot = slot.saturating_add(1);
                    }
                    self.mark_fixed(&finding);
                }
            }
            if !dotdot_seen {
                let finding = self.finding(2, Problem::MissingDotDotEntry, Some(ino), None);
                let parent = self.parents[ino as usize];
                if self.repair
                    && parent != 0
                    && self.insert_entry(ino, &inode, b"..", parent, dtype(FileType::Directory))?
                {
                    if let Some(slot) = self.refs.get_mut(parent as usize) {
                        *slot = slot.saturating_add(1);
                    }
                    self.dotdots[ino as usize] = parent;
                    self.mark_fixed(&finding);
                }
            }
        }
        Ok(())
    }

    fn fix_dotdots(&mut self) -> Result<(), UfsError> {
        for ino in 2..self.maxino {
            if self.states[ino as usize].state != InoState::Dir {
                continue;
            }
            let expected = if ino == UFS_ROOTINO {
                ino
            } else {
                self.parents[ino as usize]
            };
            if expected == 0 {
                continue;
            }
            let current = self.dotdots[ino as usize];
            if current == expected {
                continue;
            }
            let problem = if current == 0 {
                Problem::MissingDotDotEntry
            } else {
                Problem::BadDotDotEntry
            };
            let finding = self.finding(2, problem, Some(ino), None);
            if !self.repair {
                continue;
            }
            let offset = self.dotdot_offsets[ino as usize];
            let inode = self.read_inode(ino)?;
            let fixed = if offset != 0 {
                self.rewrite_entry(ino, &inode, offset, expected, dtype(FileType::Directory))
            } else {
                self.insert_entry(ino, &inode, b"..", expected, dtype(FileType::Directory))
            };
            if fixed.unwrap_or(false) {
                if current != 0
                    && current < self.maxino
                    && let Some(slot) = self.refs.get_mut(current as usize)
                {
                    *slot = slot.saturating_sub(1);
                }
                if let Some(slot) = self.refs.get_mut(expected as usize) {
                    *slot = slot.saturating_add(1);
                }
                self.dotdots[ino as usize] = expected;
                self.mark_fixed(&finding);
            }
        }
        Ok(())
    }

    // --- phase 3 -----------------------------------------------------------

    fn reconnect_orphans(&mut self) -> Result<(), UfsError> {
        if !self.repair {
            let reachable = self.reachable_dirs()?;
            for ino in 2..self.maxino {
                if self.states[ino as usize].state == InoState::Dir
                    && !reachable.get(ino as usize).copied().unwrap_or(false)
                {
                    self.finding(3, Problem::OrphanedDirectory, Some(ino), None);
                }
            }
            return Ok(());
        }
        loop {
            let reachable = self.reachable_dirs()?;
            let mut orphan = None;
            for ino in 2..self.maxino {
                if self.states[ino as usize].state == InoState::Dir
                    && !reachable.get(ino as usize).copied().unwrap_or(false)
                {
                    orphan = Some(ino);
                    break;
                }
            }
            let Some(ino) = orphan else {
                return Ok(());
            };
            let finding = self.finding(3, Problem::OrphanedDirectory, Some(ino), None);
            if self.attach_to_lost_found(ino)? {
                self.mark_fixed(&finding);
            } else {
                // Without a usable lost+found the orphan stays where it is; the
                // verification pass reports it again and clean_after stays false.
                return Ok(());
            }
        }
    }

    fn reachable_dirs(&self) -> Result<Vec<bool>, UfsError> {
        let mut reachable = vec![false; self.maxino as usize];
        let mut children: Vec<Vec<Ino>> = vec![Vec::new(); self.maxino as usize];
        for ino in 2..self.maxino {
            if self.states[ino as usize].state == InoState::Dir {
                let parent = self.parents[ino as usize];
                if parent != 0 && parent < self.maxino {
                    children[parent as usize].push(ino);
                }
            }
        }
        let mut queue = vec![UFS_ROOTINO];
        reachable[UFS_ROOTINO as usize] = true;
        while let Some(dir) = queue.pop() {
            if let Some(list) = children.get(dir as usize) {
                for child in list {
                    if !reachable[*child as usize] {
                        reachable[*child as usize] = true;
                        queue.push(*child);
                    }
                }
            }
        }
        Ok(reachable)
    }

    fn ensure_lost_found(&mut self) -> Result<Option<Ino>, UfsError> {
        if let Some(lf) = self.lf {
            return Ok(Some(lf));
        }
        if let Some(ino) = self.lookup_entry(UFS_ROOTINO, b"lost+found")?
            && self.states.get(ino as usize).map(|state| state.state) == Some(InoState::Dir)
        {
            self.lf = Some(ino);
            return Ok(Some(ino));
        }
        self.finding(3, Problem::LostFoundMissing, Some(UFS_ROOTINO), None);
        Ok(None)
    }

    fn attach_to_lost_found(&mut self, ino: Ino) -> Result<bool, UfsError> {
        let Some(lf) = self.ensure_lost_found()? else {
            return Ok(false);
        };
        let inode = self.read_inode(ino)?;
        let kind = self.states[ino as usize].kind;
        let name = alloc_crate::format!("#{ino}");
        let lf_inode = self.read_inode(lf)?;
        if !self.insert_entry(lf, &lf_inode, name.as_bytes(), ino, dtype(kind))? {
            return Ok(false);
        }
        if kind == FileType::Directory {
            let old = self.dotdots[ino as usize];
            let offset = self.dotdot_offsets[ino as usize];
            let changed = if offset != 0 {
                self.rewrite_entry(ino, &inode, offset, lf, dtype(FileType::Directory))?
            } else {
                self.insert_entry(ino, &inode, b"..", lf, dtype(FileType::Directory))?
            };
            if !changed {
                return Ok(false);
            }
            if old != 0
                && old < self.maxino
                && old != lf
                && let Some(slot) = self.refs.get_mut(old as usize)
            {
                *slot = slot.saturating_sub(1);
            }
            if let Some(slot) = self.refs.get_mut(lf as usize) {
                *slot = slot.saturating_add(1);
            }
            self.dotdots[ino as usize] = lf;
            self.parents[ino as usize] = lf;
        }
        if let Some(slot) = self.refs.get_mut(ino as usize) {
            *slot = slot.saturating_add(1);
        }
        Ok(true)
    }

    // --- phase 4 -----------------------------------------------------------

    fn fix_links_and_clear(&mut self) -> Result<(), UfsError> {
        for ino in 2..self.maxino {
            let state = self.states[ino as usize];
            if state.state == InoState::Free || state.state == InoState::Clear {
                continue;
            }
            if self.refs[ino as usize] == 0 {
                let finding = self.finding(4, Problem::UnreferencedInode, Some(ino), None);
                if !self.repair {
                    continue;
                }
                if state.nlink <= 0 {
                    let inode = self.read_inode(ino)?;
                    self.release_inode_blocks(ino, &inode)?;
                    self.clear_inode_raw(ino)?;
                    self.states[ino as usize] = InodeState::free();
                    self.mark_fixed(&finding);
                } else if self.attach_to_lost_found(ino)? {
                    self.mark_fixed(&finding);
                }
                continue;
            }
            let expected = self.refs[ino as usize];
            if expected > i16::MAX as u32 {
                continue;
            }
            let inode = self.read_inode(ino)?;
            if inode.nlink as u32 != expected {
                let finding = self.finding(
                    4,
                    Problem::BadLinkCount,
                    Some(ino),
                    Some(u64::from(expected)),
                );
                if self.repair {
                    let mut fixed = inode;
                    fixed.nlink = expected as i32;
                    self.write_inode(ino, &fixed)?;
                    self.mark_fixed(&finding);
                }
            }
        }
        for ino in 2..self.maxino {
            if !self.repair {
                break;
            }
            if self.states[ino as usize].state != InoState::Clear {
                continue;
            }
            let inode = self.read_inode(ino)?;
            self.release_inode_blocks(ino, &inode)?;
            self.clear_inode_raw(ino)?;
            self.states[ino as usize] = InodeState::free();
        }
        Ok(())
    }

    fn release_inode_blocks(&mut self, _ino: Ino, inode: &RawInode) -> Result<(), UfsError> {
        if inode.size > self.max_file_size() {
            // Phase 1 skipped the block walk for this inode, so its blocks were
            // never marked allocated; phase 5 will mark them free.
            return Ok(());
        }
        let special = matches!(
            inode.file_type(),
            Some(FileType::CharDevice | FileType::BlockDevice | FileType::Fifo | FileType::Socket)
        );
        let fast = inode.file_type() == Some(FileType::Symlink)
            && (inode.size < self.fs.maxsymlinklen as u64
                || (self.fs.maxsymlinklen == 0 && inode.blocks == 0));
        if special || fast {
            return Ok(());
        }
        let ndb = inode.size.div_ceil(u64::from(self.fs.bsize));
        let mut next_lbn = 0u64;
        for index in 0..UFS_NDADDR {
            if next_lbn >= ndb {
                break;
            }
            if inode.db[index] != 0 && self.valid_fragment(inode.db[index]) {
                self.release_extent(inode.db[index], self.block_frags(inode.size, next_lbn));
            }
            next_lbn += 1;
        }
        let mut remaining = ndb.saturating_sub(UFS_NDADDR as u64);
        for index in 0..UFS_NIADDR {
            if remaining == 0 {
                break;
            }
            let level = (index + 1) as u8;
            if inode.ib[index] != 0 && self.valid_fragment(inode.ib[index]) {
                self.release_extent(inode.ib[index], self.fs.frag);
                let child_capacity = self.fs.nindir().saturating_pow(u32::from(level));
                let child_needed = remaining.min(child_capacity);
                self.walk_indirect_release(level, inode.ib[index], child_needed, &mut next_lbn)?;
                remaining = remaining.saturating_sub(child_needed);
            } else {
                let child_capacity = self.fs.nindir().saturating_pow(u32::from(level));
                let child_needed = remaining.min(child_capacity);
                remaining = remaining.saturating_sub(child_needed);
                next_lbn = next_lbn.saturating_add(child_needed);
            }
        }
        Ok(())
    }

    // --- phase 5 -----------------------------------------------------------

    fn rebuild_metadata(&mut self) -> Result<(), UfsError> {
        let mut summaries: Vec<Csum32> = Vec::with_capacity(self.fs.ncg as usize);
        let mut total = Csum::default();
        for cg in 0..self.fs.ncg {
            let built = self.build_cg(cg)?;
            let old = self.read_cg_raw(cg)?;
            let mut cg_findings: Vec<Finding> = Vec::new();
            let header_bad = old
                .raw
                .get(..CG_IUSEDOFF_OFF)
                .zip(built.raw.get(..CG_IUSEDOFF_OFF))
                .map(|(old, new)| old != new)
                .unwrap_or(true);
            if header_bad {
                let finding =
                    self.finding(5, Problem::CylinderGroupSummary, None, Some(u64::from(cg)));
                cg_findings.push(finding);
            }
            if old.inode_map != built.inode_map {
                let finding = self.finding(5, Problem::InodeBitmap, None, Some(u64::from(cg)));
                cg_findings.push(finding);
            }
            if old.block_map != built.block_map {
                let finding = self.finding(5, Problem::BlockBitmap, None, Some(u64::from(cg)));
                cg_findings.push(finding);
            }
            if old.cs != built.cs || old.frsum != read_frsum(&built.raw) {
                let finding =
                    self.finding(5, Problem::CylinderGroupSummary, None, Some(u64::from(cg)));
                cg_findings.push(finding);
            }
            if !cg_findings.is_empty() && self.repair {
                self.write_cg_raw(cg, &built.raw)?;
                for finding in &cg_findings {
                    self.mark_fixed(finding);
                }
            }
            total.ndir += i64::from(built.cs.ndir);
            total.nbfree += i64::from(built.cs.nbfree);
            total.nifree += i64::from(built.cs.nifree);
            total.nffree += i64::from(built.cs.nffree);
            summaries.push(built.cs);
        }
        if total != self.fs.cstotal {
            let finding = self.finding(5, Problem::SuperblockSummary, None, None);
            if self.repair {
                self.fs.cstotal = total;
                self.fs.write_cstotal_raw();
                self.mark_fixed(&finding);
            }
        }
        if self.repair {
            ffs_vfsops::write_summaries(self.disk, &self.fs, &summaries)?;
            ffs_vfsops::write_superblock(self.disk, &self.fs)?;
        }
        Ok(())
    }

    fn build_cg(&mut self, cg: u32) -> Result<CgBuild, UfsError> {
        let ipg = self.fs.ipg as usize;
        let fpg = self.fs.fpg as usize;
        let frag = self.fs.frag as usize;
        let inode_map_len = ipg.div_ceil(8);
        let block_map_len = fpg.div_ceil(8);
        let old = self.read_cg_raw(cg)?;
        let cgbase = self.fs.cgbase(cg)?;
        let ndblk = u64::from(self.fs.fpg).min(self.fs.size.saturating_sub(cgbase)) as u32;
        let mut inode_map = vec![0u8; inode_map_len];
        let mut ndir = 0i32;
        let mut used_inodes = 0i32;
        let first = u64::from(cg) * u64::from(self.fs.ipg);
        for index in 0..self.fs.ipg {
            let ino = first + u64::from(index);
            if ino >= u64::from(self.maxino) {
                break;
            }
            let used =
                ino < u64::from(UFS_ROOTINO) || self.states[ino as usize].state != InoState::Free;
            if used {
                ffs_bitmap::set_bit(&mut inode_map, index as usize)?;
                used_inodes += 1;
                if self.states[ino as usize].state == InoState::Dir {
                    ndir += 1;
                }
            }
        }
        let mut block_map = vec![0u8; block_map_len];
        for index in 0..ndblk as usize {
            let fragno = cgbase + index as u64;
            let used = self.is_reserved(fragno)
                || self.owners.get(fragno as usize).copied().unwrap_or(0) > 0;
            if !used {
                ffs_bitmap::set_bit(&mut block_map, index)?;
            }
        }
        let mut nbfree = 0i32;
        let mut nffree = 0i32;
        let mut frsum = [0i32; 8];
        let blocks = fpg / frag;
        for block in 0..blocks {
            let map = ffs_bitmap::block_map(self.fs.frag, &block_map, block * frag)?;
            let free = map.count_ones();
            if free == self.fs.frag {
                nbfree += 1;
            } else {
                nffree += free as i32;
            }
            if free > 0 {
                ffs_bitmap::fragacct(self.fs.frag, map, &mut frsum, 1)?;
            }
        }
        let cs = Csum32 {
            ndir,
            nbfree,
            nifree: self.fs.ipg as i32 - used_inodes,
            nffree,
        };

        let mut newcg = vec![0u8; self.fs.cgsize as usize];
        write_u32_at(&mut newcg, CG_MAGIC_OFF, CG_MAGIC)?;
        if let Some(old_time) = old.raw.get(CG_OLD_TIME_OFF..CG_OLD_TIME_OFF + 4)
            && let Some(dst) = newcg.get_mut(CG_OLD_TIME_OFF..CG_OLD_TIME_OFF + 4)
        {
            dst.copy_from_slice(old_time);
        }
        write_u32_at(&mut newcg, CG_CGX_OFF, cg)?;
        write_u32_at(&mut newcg, CG_NDBLK_OFF, ndblk)?;
        for offset in [CG_ROTOR_OFF, CG_FROTOR_OFF, CG_IROTOR_OFF] {
            if let Some(value) = old.raw.get(offset..offset + 4)
                && let Some(dst) = newcg.get_mut(offset..offset + 4)
            {
                dst.copy_from_slice(value);
            }
        }
        newcg
            .get_mut(CG_CS_OFF..CG_CS_OFF + 16)
            .ok_or(UfsError::Corrupt("cylinder group counters"))?
            .copy_from_slice(&cs.to_le_bytes());
        for (index, value) in frsum.iter().enumerate() {
            write_u32_at(&mut newcg, CG_FRSUM_OFF + index * 4, *value as u32)?;
        }
        let iusedoff = if self.fs.ufs2 { 168usize } else { 174usize };
        let freeoff = iusedoff + inode_map_len;
        let mut nextfree = freeoff + block_map_len;
        let mut clustersumoff = 0usize;
        let mut clusteroff = 0usize;
        if self.fs.contigsumsize > 0 {
            clustersumoff = roundup_usize(freeoff + block_map_len - 4, 4);
            clusteroff = clustersumoff + (self.fs.contigsumsize as usize + 1) * 4;
            nextfree = clusteroff + (fpg / frag).div_ceil(8);
        }
        write_u32_at(&mut newcg, CG_IUSEDOFF_OFF, iusedoff as u32)?;
        write_u32_at(&mut newcg, CG_FREEOFF_OFF, freeoff as u32)?;
        write_u32_at(&mut newcg, CG_NEXTFREEOFF_OFF, nextfree as u32)?;
        write_u32_at(&mut newcg, CG_CLUSTERSUMOFF_OFF, clustersumoff as u32)?;
        write_u32_at(&mut newcg, CG_CLUSTEROFF_OFF, clusteroff as u32)?;
        write_u32_at(&mut newcg, CG_NCLUSTERBLKS_OFF, ndblk / self.fs.frag)?;
        if self.fs.ufs2 {
            write_u32_at(&mut newcg, CG_NIBLK_OFF, self.fs.ipg)?;
            let inited = old
                .raw
                .get(CG_INITEDIBLK_OFF..CG_INITEDIBLK_OFF + 4)
                .and_then(|bytes| bytes.try_into().ok())
                .map(u32::from_le_bytes)
                .unwrap_or(self.fs.ipg)
                .min(self.fs.ipg);
            write_u32_at(&mut newcg, CG_INITEDIBLK_OFF, inited)?;
            if let Some(old_time) = old.raw.get(CG_TIME_OFF..CG_TIME_OFF + 8)
                && let Some(dst) = newcg.get_mut(CG_TIME_OFF..CG_TIME_OFF + 8)
            {
                dst.copy_from_slice(old_time);
            }
        } else {
            write_u16_at(&mut newcg, 16, 1)?;
            write_u16_at(&mut newcg, CG_UFS1_NIBLK_OFF, self.fs.ipg as u16)?;
            write_u32_at(&mut newcg, CG_OLD_BTOTOFF_OFF, 168)?;
            write_u32_at(&mut newcg, CG_OLD_BOOFF_OFF, 172)?;
            if let Some(old_space) = old.raw.get(168..174)
                && let Some(dst) = newcg.get_mut(168..174)
            {
                dst.copy_from_slice(old_space);
            }
        }
        if let Some(dst) = newcg.get_mut(iusedoff..iusedoff + inode_map_len) {
            dst.copy_from_slice(&inode_map);
        }
        if let Some(dst) = newcg.get_mut(freeoff..freeoff + block_map_len) {
            dst.copy_from_slice(&block_map);
        }
        if self.fs.contigsumsize > 0 {
            let ncluster = ndblk as usize / frag;
            let mut clusters = vec![0u8; ncluster.div_ceil(8)];
            for block in 0..ncluster {
                let map = ffs_bitmap::block_map(self.fs.frag, &block_map, block * frag)?;
                if map == 0xff {
                    ffs_bitmap::set_bit(&mut clusters, block)?;
                }
            }
            let mut sums = vec![0i32; self.fs.contigsumsize as usize + 1];
            let mut run = 0usize;
            for block in 0..ncluster {
                if ffs_bitmap::bit_is_set(&clusters, block) {
                    run += 1;
                } else if run > 0 {
                    let slot = run.min(self.fs.contigsumsize as usize);
                    if let Some(value) = sums.get_mut(slot) {
                        *value += 1;
                    }
                    run = 0;
                }
            }
            if run > 0 {
                let slot = run.min(self.fs.contigsumsize as usize);
                if let Some(value) = sums.get_mut(slot) {
                    *value += 1;
                }
            }
            for (index, value) in sums.iter().enumerate().skip(1) {
                write_u32_at(&mut newcg, clustersumoff + index * 4, *value as u32)?;
            }
            if let Some(dst) = newcg.get_mut(clusteroff..clusteroff + clusters.len()) {
                dst.copy_from_slice(&clusters);
            }
        }
        Ok(CgBuild {
            raw: newcg,
            cs,
            inode_map,
            block_map,
        })
    }

    fn read_cg_raw(&mut self, cg: u32) -> Result<CgRaw, UfsError> {
        let offset = self
            .fs
            .cgstart(cg)?
            .checked_add(self.fs.cblkno as u64)
            .and_then(|frag| frag.checked_mul(u64::from(self.fs.fsize)))
            .ok_or(UfsError::Corrupt("cylinder group offset"))?;
        let end = offset
            .checked_add(u64::from(self.fs.cgsize))
            .ok_or(UfsError::Corrupt("cylinder group offset"))?;
        if end > self.disk.size_bytes() {
            return Err(UfsError::Corrupt("cylinder group beyond device"));
        }
        let mut raw = vec![0u8; self.fs.cgsize as usize];
        self.disk
            .read_at(offset, &mut raw)
            .map_err(|_| UfsError::Io)?;
        let iusedoff = read_u32_from(&raw, CG_IUSEDOFF_OFF)? as usize;
        let freeoff = read_u32_from(&raw, CG_FREEOFF_OFF)? as usize;
        let inode_map_len = (self.fs.ipg as usize).div_ceil(8);
        let block_map_len = (self.fs.fpg as usize).div_ceil(8);
        let inode_map = raw
            .get(iusedoff..iusedoff.saturating_add(inode_map_len))
            .map(|slice| slice.to_vec())
            .unwrap_or_default();
        let block_map = raw
            .get(freeoff..freeoff.saturating_add(block_map_len))
            .map(|slice| slice.to_vec())
            .unwrap_or_default();
        let cs = Csum32::from_le_bytes(
            raw.get(CG_CS_OFF..CG_CS_OFF + 16)
                .ok_or(UfsError::Corrupt("cylinder group counters"))?,
        )?;
        let frsum = read_frsum(&raw);
        Ok(CgRaw {
            raw,
            cs,
            inode_map,
            block_map,
            frsum,
        })
    }

    fn write_cg_raw(&mut self, cg: u32, raw: &[u8]) -> Result<(), UfsError> {
        let offset = self
            .fs
            .cgstart(cg)?
            .checked_add(self.fs.cblkno as u64)
            .and_then(|frag| frag.checked_mul(u64::from(self.fs.fsize)))
            .ok_or(UfsError::Corrupt("cylinder group offset"))?;
        self.disk.write_at(offset, raw).map_err(|_| UfsError::Io)
    }

    // --- raw directory helpers --------------------------------------------

    fn block_cover(&mut self, inode: &RawInode, lbn: u64) -> Result<u32, UfsError> {
        let bsize = u64::from(self.fs.bsize);
        if lbn < inode.size / bsize {
            return Ok(self.fs.bsize);
        }
        let offset = (inode.size % bsize) as u32;
        Ok(roundup(u64::from(offset), u64::from(self.fs.fsize)) as u32)
    }

    fn read_extent(&mut self, frag: u64, cover: u32) -> Result<Vec<u8>, UfsError> {
        if cover == 0 || cover > self.fs.bsize || !cover.is_multiple_of(self.fs.fsize) {
            return Err(UfsError::Corrupt("extent size"));
        }
        let offset = self.fs.frag_offset(frag)?;
        let end = offset
            .checked_add(u64::from(cover))
            .ok_or(UfsError::Corrupt("extent offset"))?;
        if end > self.disk.size_bytes() {
            return Err(UfsError::Corrupt("extent beyond device"));
        }
        let mut data = vec![0u8; cover as usize];
        self.disk
            .read_at(offset, &mut data)
            .map_err(|_| UfsError::Io)?;
        Ok(data)
    }

    fn write_extent(&mut self, frag: u64, data: &[u8]) -> Result<(), UfsError> {
        if data.is_empty() || data.len() > self.fs.bsize as usize {
            return Err(UfsError::Invalid);
        }
        let offset = self.fs.frag_offset(frag)?;
        self.disk.write_at(offset, data).map_err(|_| UfsError::Io)
    }

    fn read_fs_block(&mut self, frag: u64) -> Result<Vec<u8>, UfsError> {
        self.read_extent(frag, self.fs.bsize)
    }

    fn write_fs_block(&mut self, frag: u64, data: &[u8]) -> Result<(), UfsError> {
        self.write_extent(frag, data)
    }

    fn lookup_entry(&mut self, dir_ino: Ino, name: &[u8]) -> Result<Option<Ino>, UfsError> {
        let inode = self.read_inode(dir_ino)?;
        if inode.file_type() != Some(FileType::Directory) {
            return Ok(None);
        }
        let blocks = inode.size.div_ceil(u64::from(self.fs.bsize));
        for lbn in 0..blocks {
            let Some(frag) = self.inode_bmap(&inode, lbn)? else {
                continue;
            };
            let cover = self.block_cover(&inode, lbn)?;
            if cover < UFS_DIRBLKSIZ as u32 {
                continue;
            }
            let data = self.read_extent(frag, cover)?;
            let base = (lbn * u64::from(self.fs.bsize)) as usize;
            let block_start = base % self.fs.bsize as usize;
            let mut off = 0usize;
            while off < UFS_DIRBLKSIZ {
                let Ok(Some(entry)) = dir::parse(&data[block_start + off..], UFS_DIRBLKSIZ - off)
                else {
                    break;
                };
                let next = off + entry.next;
                if next > UFS_DIRBLKSIZ {
                    break;
                }
                if entry.ino != 0 && entry.name == name {
                    return Ok(Some(entry.ino));
                }
                off = next;
            }
        }
        Ok(None)
    }

    fn insert_entry(
        &mut self,
        dir_ino: Ino,
        inode: &RawInode,
        name: &[u8],
        target: Ino,
        entry_type: u8,
    ) -> Result<bool, UfsError> {
        if name.is_empty() || name.len() > FFS_MAXNAMLEN || name.contains(&0) {
            return Ok(false);
        }
        let need = dirsiz(name.len());
        let blocks = inode.size.div_ceil(u64::from(self.fs.bsize));
        for lbn in 0..blocks {
            let Some(frag) = self.inode_bmap(inode, lbn)? else {
                continue;
            };
            let cover = self.block_cover(inode, lbn)?;
            if cover < UFS_DIRBLKSIZ as u32 {
                continue;
            }
            let mut data = self.read_extent(frag, cover)?;
            let base = (lbn * u64::from(self.fs.bsize)) as usize;
            let block_start = base % self.fs.bsize as usize;
            if insert_into_block(&mut data, block_start, name, target, entry_type, need)? {
                self.write_extent(frag, &data)?;
                let _ = dir_ino;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn rewrite_entry(
        &mut self,
        dir_ino: Ino,
        inode: &RawInode,
        offset: u64,
        target: Ino,
        entry_type: u8,
    ) -> Result<bool, UfsError> {
        let bsize = u64::from(self.fs.bsize);
        let lbn = offset / bsize;
        let block_off = (offset % bsize) as usize;
        let Some(frag) = self.inode_bmap(inode, lbn)? else {
            return Ok(false);
        };
        let cover = self.block_cover(inode, lbn)?;
        if cover < (block_off + 8) as u32 {
            return Ok(false);
        }
        let mut data = self.read_extent(frag, cover)?;
        let block_start = ((lbn * bsize) % bsize) as usize + block_off;
        if data.get(block_start + 4..block_start + 6).is_none() {
            return Ok(false);
        }
        write_u32_at(&mut data, block_start, target)?;
        if let Some(slot) = data.get_mut(block_start + 6) {
            *slot = entry_type;
        }
        self.write_extent(frag, &data)?;
        let _ = dir_ino;
        Ok(true)
    }
}

struct CgBuild {
    raw: Vec<u8>,
    cs: Csum32,
    inode_map: Vec<u8>,
    block_map: Vec<u8>,
}

struct CgRaw {
    raw: Vec<u8>,
    cs: Csum32,
    inode_map: Vec<u8>,
    block_map: Vec<u8>,
    frsum: [i32; 8],
}

// --- record helpers --------------------------------------------------------

fn decode_inode(ufs2: bool, raw: &[u8]) -> Result<RawInode, UfsError> {
    let expected = if ufs2 { 256 } else { 128 };
    if raw.len() != expected {
        return Err(UfsError::Corrupt("inode size"));
    }
    let mode = read_u16_from(raw, 0)?;
    let nlink = i32::from(read_i16_from(raw, 2)?);
    let (uid, gid, size, blocks, flags, db, ib) = if ufs2 {
        (
            read_u32_from(raw, 4)?,
            read_u32_from(raw, 8)?,
            read_u64_from(raw, 16)?,
            read_u64_from(raw, 24)?,
            read_u32_from(raw, 88)?,
            read_db(raw, 112, 8)?,
            read_ib(raw, 208, 8)?,
        )
    } else {
        (
            read_u32_from(raw, 112)?,
            read_u32_from(raw, 116)?,
            read_u64_from(raw, 8)?,
            u64::from(read_u32_from(raw, 104)?),
            read_u32_from(raw, 100)?,
            read_db(raw, 40, 4)?,
            read_ib(raw, 88, 4)?,
        )
    };
    Ok(RawInode {
        mode,
        nlink,
        uid,
        gid,
        size,
        blocks,
        flags,
        db,
        ib,
    })
}

fn encode_inode(ufs2: bool, raw: &mut [u8], inode: &RawInode) -> Result<(), UfsError> {
    let expected = if ufs2 { 256 } else { 128 };
    if raw.len() != expected {
        return Err(UfsError::Corrupt("inode size"));
    }
    write_u16_at(raw, 0, inode.mode)?;
    write_u16_at(raw, 2, inode.nlink.clamp(0, i16::MAX as i32) as u16)?;
    if ufs2 {
        write_u32_at(raw, 4, inode.uid)?;
        write_u32_at(raw, 8, inode.gid)?;
        write_u64_at(raw, 16, inode.size)?;
        write_u64_at(raw, 24, inode.blocks)?;
        write_u32_at(raw, 88, inode.flags)?;
        for (index, value) in inode.db.iter().enumerate() {
            write_u64_at(raw, 112 + index * 8, *value)?;
        }
        for (index, value) in inode.ib.iter().enumerate() {
            write_u64_at(raw, 208 + index * 8, *value)?;
        }
    } else {
        write_u32_at(raw, 112, inode.uid)?;
        write_u32_at(raw, 116, inode.gid)?;
        write_u64_at(raw, 8, inode.size)?;
        write_u32_at(raw, 104, u32::try_from(inode.blocks).unwrap_or(u32::MAX))?;
        write_u32_at(raw, 100, inode.flags)?;
        for (index, value) in inode.db.iter().enumerate() {
            write_u32_at(
                raw,
                40 + index * 4,
                u32::try_from(*value).map_err(|_| UfsError::Corrupt("32-bit pointer"))?,
            )?;
        }
        for (index, value) in inode.ib.iter().enumerate() {
            write_u32_at(
                raw,
                88 + index * 4,
                u32::try_from(*value).map_err(|_| UfsError::Corrupt("32-bit pointer"))?,
            )?;
        }
    }
    Ok(())
}

fn read_db(raw: &[u8], off: usize, width: usize) -> Result<[u64; UFS_NDADDR], UfsError> {
    let mut out = [0u64; UFS_NDADDR];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = if width == 8 {
            read_u64_from(raw, off + index * 8)?
        } else {
            u64::from(read_u32_from(raw, off + index * 4)?)
        };
    }
    Ok(out)
}

fn read_ib(raw: &[u8], off: usize, width: usize) -> Result<[u64; UFS_NIADDR], UfsError> {
    let mut out = [0u64; UFS_NIADDR];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = if width == 8 {
            read_u64_from(raw, off + index * 8)?
        } else {
            u64::from(read_u32_from(raw, off + index * 4)?)
        };
    }
    Ok(out)
}

fn read_pointer<D: BlockIo>(
    disk: &mut D,
    fs: &Superblock,
    frag: u64,
    index: u32,
) -> Result<u64, UfsError> {
    if !(frag > 0 && frag < fs.size) {
        return Err(UfsError::Corrupt("indirect block outside filesystem"));
    }
    let width = fs.ptr_size();
    let offset = fs
        .frag_offset(frag)?
        .checked_add(u64::from(index) * width as u64)
        .ok_or(UfsError::Corrupt("pointer offset"))?;
    if width == 8 {
        let mut bytes = [0u8; 8];
        disk.read_at(offset, &mut bytes).map_err(|_| UfsError::Io)?;
        Ok(u64::from_le_bytes(bytes))
    } else {
        let mut bytes = [0u8; 4];
        disk.read_at(offset, &mut bytes).map_err(|_| UfsError::Io)?;
        Ok(u64::from(u32::from_le_bytes(bytes)))
    }
}

fn read_pointer_from(data: &[u8], index: u64, width: usize) -> Result<u64, UfsError> {
    let offset = usize::try_from(index)
        .ok()
        .and_then(|index| index.checked_mul(width))
        .ok_or(UfsError::Corrupt("pointer offset"))?;
    if width == 8 {
        let bytes = data
            .get(offset..offset + 8)
            .ok_or(UfsError::Corrupt("pointer outside block"))?;
        let mut value = [0u8; 8];
        value.copy_from_slice(bytes);
        Ok(u64::from_le_bytes(value))
    } else {
        let bytes = data
            .get(offset..offset + 4)
            .ok_or(UfsError::Corrupt("pointer outside block"))?;
        let mut value = [0u8; 4];
        value.copy_from_slice(bytes);
        Ok(u64::from(u32::from_le_bytes(value)))
    }
}

fn write_pointer_to(data: &mut [u8], index: u64, value: u64, width: usize) -> Result<(), UfsError> {
    let offset = usize::try_from(index)
        .ok()
        .and_then(|index| index.checked_mul(width))
        .ok_or(UfsError::Corrupt("pointer offset"))?;
    if width == 8 {
        let dst = data
            .get_mut(offset..offset + 8)
            .ok_or(UfsError::Corrupt("pointer outside block"))?;
        dst.copy_from_slice(&value.to_le_bytes());
    } else {
        let dst = data
            .get_mut(offset..offset + 4)
            .ok_or(UfsError::Corrupt("pointer outside block"))?;
        dst.copy_from_slice(
            &u32::try_from(value)
                .map_err(|_| UfsError::Corrupt("32-bit pointer"))?
                .to_le_bytes(),
        );
    }
    Ok(())
}

fn insert_into_block(
    data: &mut [u8],
    block_start: usize,
    name: &[u8],
    target: Ino,
    entry_type: u8,
    need: usize,
) -> Result<bool, UfsError> {
    let mut off = 0usize;
    while off < UFS_DIRBLKSIZ {
        let entry = dir::parse(&data[block_start + off..], UFS_DIRBLKSIZ - off)?;
        let Some(entry) = entry else {
            return Ok(false);
        };
        let next = off + entry.next;
        if next > UFS_DIRBLKSIZ {
            return Ok(false);
        }
        if entry.ino == 0 {
            if entry.next >= need {
                if entry.next >= need + 8 {
                    write_entry_at(data, block_start + off, target, entry_type, name, need)?;
                    write_entry_at(data, block_start + off + need, 0, 0, b"", entry.next - need)?;
                } else {
                    write_entry_at(
                        data,
                        block_start + off,
                        target,
                        entry_type,
                        name,
                        entry.next,
                    )?;
                }
                return Ok(true);
            }
        } else {
            let used = dirsiz(entry.name.len());
            if entry.next >= used + need {
                write_u16_at(data, block_start + off + 4, used as u16)?;
                write_entry_at(
                    data,
                    block_start + off + used,
                    target,
                    entry_type,
                    name,
                    entry.next - used,
                )?;
                return Ok(true);
            }
        }
        off = next;
    }
    Ok(false)
}

fn write_entry_at(
    data: &mut [u8],
    off: usize,
    ino: u32,
    entry_type: u8,
    name: &[u8],
    reclen: usize,
) -> Result<(), UfsError> {
    if reclen < dirsiz(name.len()) || reclen & 3 != 0 {
        return Err(UfsError::Corrupt("directory record length"));
    }
    let dst = data
        .get_mut(off..off + reclen)
        .ok_or(UfsError::Corrupt("directory record"))?;
    dst.fill(0);
    dst[0..4].copy_from_slice(&ino.to_le_bytes());
    dst[4..6].copy_from_slice(
        &u16::try_from(reclen)
            .map_err(|_| UfsError::Corrupt("directory record length"))?
            .to_le_bytes(),
    );
    dst[6] = entry_type;
    dst[7] = u8::try_from(name.len()).map_err(|_| UfsError::NameTooLong)?;
    dst[8..8 + name.len()].copy_from_slice(name);
    Ok(())
}

fn read_frsum(raw: &[u8]) -> [i32; 8] {
    let mut frsum = [0i32; 8];
    for (index, slot) in frsum.iter_mut().enumerate() {
        *slot = read_u32_from(raw, CG_FRSUM_OFF + index * 4).unwrap_or(0) as i32;
    }
    frsum
}

fn roundup(value: u64, unit: u64) -> u64 {
    if unit == 0 {
        value
    } else {
        value.div_ceil(unit) * unit
    }
}

fn roundup_usize(value: usize, unit: usize) -> usize {
    if unit == 0 {
        value
    } else {
        value.div_ceil(unit) * unit
    }
}

fn read_super_u64(raw: &[u8], off: usize) -> Result<u64, UfsError> {
    read_u64_from(raw, off)
}

fn read_u16_from(data: &[u8], off: usize) -> Result<u16, UfsError> {
    let bytes = data
        .get(off..off + 2)
        .ok_or(UfsError::Corrupt("short field"))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_i16_from(data: &[u8], off: usize) -> Result<i16, UfsError> {
    Ok(read_u16_from(data, off)? as i16)
}

fn read_u32_from(data: &[u8], off: usize) -> Result<u32, UfsError> {
    let bytes = data
        .get(off..off + 4)
        .ok_or(UfsError::Corrupt("short field"))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64_from(data: &[u8], off: usize) -> Result<u64, UfsError> {
    let bytes = data
        .get(off..off + 8)
        .ok_or(UfsError::Corrupt("short field"))?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(value))
}

fn write_u16_at(data: &mut [u8], off: usize, value: u16) -> Result<(), UfsError> {
    let dst = data
        .get_mut(off..off + 2)
        .ok_or(UfsError::Corrupt("short field"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u32_at(data: &mut [u8], off: usize, value: u32) -> Result<(), UfsError> {
    let dst = data
        .get_mut(off..off + 4)
        .ok_or(UfsError::Corrupt("short field"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u64_at(data: &mut [u8], off: usize, value: u64) -> Result<(), UfsError> {
    let dst = data
        .get_mut(off..off + 8)
        .ok_or(UfsError::Corrupt("short field"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inode_round_trip_ufs1() {
        let mut raw = [0u8; 128];
        let inode = RawInode {
            mode: 0o100644,
            nlink: 2,
            uid: 1000,
            gid: 100,
            size: 1234,
            blocks: 8,
            flags: 0x10,
            db: [42; UFS_NDADDR],
            ib: [7; UFS_NIADDR],
        };
        encode_inode(false, &mut raw, &inode).expect("encode");
        let decoded = decode_inode(false, &raw).expect("decode");
        assert_eq!(decoded.mode, inode.mode);
        assert_eq!(decoded.nlink, 2);
        assert_eq!(decoded.size, 1234);
        assert_eq!(decoded.blocks, 8);
        assert_eq!(decoded.db[0], 42);
        assert_eq!(decoded.ib[2], 7);
    }

    #[test]
    fn inode_round_trip_ufs2() {
        let mut raw = [0u8; 256];
        let inode = RawInode {
            mode: 0o040755,
            nlink: 3,
            uid: 0,
            gid: 0,
            size: 4096,
            blocks: 8,
            flags: 0,
            db: [99; UFS_NDADDR],
            ib: [5; UFS_NIADDR],
        };
        encode_inode(true, &mut raw, &inode).expect("encode");
        let decoded = decode_inode(true, &raw).expect("decode");
        assert_eq!(decoded.mode, 0o040755);
        assert_eq!(decoded.nlink, 3);
        assert_eq!(decoded.size, 4096);
        assert_eq!(decoded.db[11], 99);
        assert_eq!(decoded.ib[0], 5);
    }

    #[test]
    fn empty_inode_is_recognised() {
        assert!(decode_inode(true, &[0u8; 256]).expect("decode").is_empty());
        let mut raw = [0u8; 128];
        raw[40] = 1;
        assert!(!decode_inode(false, &raw).expect("decode").is_empty());
    }

    #[test]
    fn short_inode_is_rejected() {
        assert!(decode_inode(false, &[0u8; 64]).is_err());
        assert!(decode_inode(true, &[0u8; 64]).is_err());
    }

    #[test]
    fn record_insertion_splits_a_free_record() {
        let mut data = [0u8; UFS_DIRBLKSIZ];
        write_entry_at(&mut data, 0, 2, 4, b".", 12).expect("dot");
        write_entry_at(&mut data, 12, 2, 4, b"..", 500).expect("dotdot");
        assert!(insert_into_block(&mut data, 0, b"hello", 7, 8, dirsiz(5)).expect("insert"));
        let dotdot = dir::parse(&data[12..], 500).expect("parse").expect("entry");
        assert_eq!(dotdot.name, b"..");
        let inserted = dir::parse(&data[24..], 500 - 12)
            .expect("parse")
            .expect("entry");
        assert_eq!(inserted.ino, 7);
        assert_eq!(inserted.name, b"hello");
    }

    #[test]
    fn missing_dot_entries_are_built() {
        let dots = crate::dirwrite::dot_entries(2, 2).expect("dots");
        let dot = dir::parse(&dots, UFS_DIRBLKSIZ)
            .expect("parse")
            .expect("dot");
        assert_eq!(dot.name, b".");
        assert_eq!(dot.ino, 2);
        let dotdot = dir::parse(&dots[12..], UFS_DIRBLKSIZ - 12)
            .expect("parse")
            .expect("dotdot");
        assert_eq!(dotdot.name, b"..");
    }

    #[test]
    fn pointer_round_trip() {
        let mut data = vec![0u8; 32];
        write_pointer_to(&mut data, 2, 0x1234, 8).expect("write");
        assert_eq!(read_pointer_from(&data, 2, 8).expect("read"), 0x1234);
        write_pointer_to(&mut data, 3, 0x55, 4).expect("write");
        assert_eq!(read_pointer_from(&data, 3, 4).expect("read"), 0x55);
    }

    #[test]
    fn roundup_behaves() {
        assert_eq!(roundup(0, 512), 0);
        assert_eq!(roundup(1, 512), 512);
        assert_eq!(roundup(512, 512), 512);
        assert_eq!(roundup_usize(13, 4), 16);
    }
}
