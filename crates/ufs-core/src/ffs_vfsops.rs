// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ffs/ffs_vfsops.c (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notices follow.
//
// Copyright (c) 2008, 2009 The NetBSD Foundation, Inc.
// All rights reserved.
//
// This code is derived from software contributed to The NetBSD Foundation
// by Wasabi Systems, Inc, and by Andrew Doran.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
// BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
// Copyright (c) 1989, 1991, 1993, 1994
//	The Regents of the University of California.  All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! Superblock probing, mount validation and `statfs` counters.

use alloc_crate::vec;
use alloc_crate::vec::Vec;

use crate::blockio::BlockIo;
use crate::error::UfsError;
use crate::fs::{
    Csum32, MAXBSIZE, MAXFRAG, MINBSIZE, SBLOCK_FLOPPY, SBLOCK_PIGGY, SBLOCK_UFS1, SBLOCK_UFS2,
    SBLOCKSIZE, Superblock, UFS_ROOTINO,
};
use crate::types::StatFs;

/// Probe the upstream superblock locations in order and return the first valid one.
pub(crate) fn probe<D: BlockIo>(disk: &mut D) -> Result<Superblock, UfsError> {
    let device_size = disk.size_bytes();
    let mut last = UfsError::BadSuperblock("no superblock");
    for &candidate in &[SBLOCK_UFS2, SBLOCK_UFS1, SBLOCK_FLOPPY, SBLOCK_PIGGY] {
        let Some(end) = candidate.checked_add(SBLOCKSIZE) else {
            continue;
        };
        if end > device_size {
            continue;
        }
        let mut buf = vec![0u8; SBLOCKSIZE as usize];
        if disk.read_at(candidate, &mut buf).is_err() {
            last = UfsError::Io;
            continue;
        }
        match Superblock::parse(&buf, candidate) {
            Ok(sb) => match validate(&sb, device_size) {
                Ok(()) => return Ok(sb),
                Err(err @ UfsError::Unsupported(_)) => return Err(err),
                Err(err) => last = err,
            },
            Err(err @ UfsError::Unsupported(_)) => return Err(err),
            Err(err) => last = err,
        }
    }
    Err(last)
}

/// Validate the superblock fields that matter for safe read-only access.
pub(crate) fn validate(fs: &Superblock, device_size: u64) -> Result<(), UfsError> {
    if fs.sbsize < 1376 || fs.sbsize > SBLOCKSIZE as u32 {
        return Err(UfsError::BadSuperblock("bad superblock size"));
    }
    if fs.bsize < MINBSIZE || fs.bsize > MAXBSIZE || !power_of_two(fs.bsize) {
        return Err(UfsError::BadSuperblock("bad block size"));
    }
    if fs.fsize < 512 || fs.fsize > fs.bsize || !power_of_two(fs.fsize) {
        return Err(UfsError::BadSuperblock("bad fragment size"));
    }
    if !fs.bsize.is_multiple_of(fs.fsize) {
        return Err(UfsError::BadSuperblock("block/fragment ratio"));
    }
    let frag = fs.bsize / fs.fsize;
    if frag == 0 || frag > MAXFRAG || fs.frag != frag || !frag.is_power_of_two() {
        return Err(UfsError::BadSuperblock("bad fragments per block"));
    }
    if !(0..100).contains(&fs.minfree) {
        return Err(UfsError::BadSuperblock("bad minfree"));
    }
    if fs.optim != 0 && fs.optim != 1 {
        return Err(UfsError::BadSuperblock("bad optimization"));
    }
    if fs.contigsumsize < 0 || fs.contigsumsize > 64 {
        return Err(UfsError::BadSuperblock("bad cluster summary size"));
    }
    if fs.bshift != log2(fs.bsize) || fs.fshift != log2(fs.fsize) || fs.fragshift != log2(frag) {
        return Err(UfsError::BadSuperblock("bad shift fields"));
    }
    if fs.bmask != !((fs.bsize - 1) as i32) || fs.fmask != !((fs.fsize - 1) as i32) {
        return Err(UfsError::BadSuperblock("bad masks"));
    }
    if fs.ncg == 0 || fs.ipg == 0 || fs.fpg == 0 || fs.inopb == 0 || fs.nindir == 0 {
        return Err(UfsError::BadSuperblock("zero geometry field"));
    }
    if fs.maxbpg <= 0 {
        return Err(UfsError::BadSuperblock("zero maxbpg"));
    }
    if fs.inopb != fs.bsize / fs.inode_size() as u32 {
        return Err(UfsError::BadSuperblock("inodes per block"));
    }
    if fs.nindir != fs.bsize / fs.ptr_size() as u32 {
        return Err(UfsError::BadSuperblock("pointers per block"));
    }
    if fs.size == 0 || fs.dsize == 0 || fs.dsize > fs.size {
        return Err(UfsError::BadSuperblock("bad filesystem size"));
    }
    if fs.cgsize < 512 || fs.cgsize > fs.bsize {
        return Err(UfsError::BadSuperblock("bad cylinder group size"));
    }
    if fs.cssize == 0 {
        return Err(UfsError::BadSuperblock("zero summary size"));
    }
    if (fs.cssize as usize) < (fs.ncg as usize) * 16 {
        return Err(UfsError::BadSuperblock("summary array too small"));
    }
    let cg_blocks = u64::from(fs.ncg)
        .checked_mul(u64::from(fs.fpg))
        .ok_or(UfsError::BadSuperblock("cylinder group overflow"))?;
    if fs.size > cg_blocks {
        return Err(UfsError::BadSuperblock("size beyond cylinder groups"));
    }
    let device_needed = fs
        .size
        .checked_mul(u64::from(fs.fsize))
        .ok_or(UfsError::BadSuperblock("filesystem size overflow"))?;
    if device_needed > device_size {
        return Err(UfsError::BadSuperblock("filesystem larger than device"));
    }
    let sblock_end = fs
        .sblockloc
        .checked_add(SBLOCKSIZE)
        .ok_or(UfsError::BadSuperblock("superblock offset overflow"))?;
    if sblock_end > device_size {
        return Err(UfsError::BadSuperblock("superblock outside device"));
    }
    let summary_end = u64::from(fs.csaddr)
        .checked_add(u64::from(fs.cssize).div_ceil(u64::from(fs.fsize)))
        .ok_or(UfsError::BadSuperblock("summary address overflow"))?;
    if summary_end > fs.size {
        return Err(UfsError::BadSuperblock("summary outside filesystem"));
    }
    if fs.sblkno < 0
        || fs.cblkno < fs.sblkno
        || fs.iblkno < fs.cblkno
        || i64::from(fs.dblkno) < i64::from(fs.iblkno) + i64::from(frag)
    {
        return Err(UfsError::BadSuperblock("bad cylinder group offsets"));
    }
    if i64::from(fs.dblkno) >= i64::from(fs.fpg)
        || i64::from(fs.iblkno) + i64::from(frag) > i64::from(fs.fpg)
    {
        return Err(UfsError::BadSuperblock("offset beyond cylinder group"));
    }
    let last = fs.ncg - 1;
    let last_start = fs.cgstart(last)?;
    let last_data = last_start
        .checked_add(fs.dblkno as u64)
        .ok_or(UfsError::BadSuperblock("last group overflow"))?;
    if last_data > fs.size {
        return Err(UfsError::BadSuperblock("last group beyond filesystem"));
    }
    let last_inodes = last_start
        .checked_add(fs.iblkno as u64)
        .and_then(|v| v.checked_add(u64::from(frag)))
        .ok_or(UfsError::BadSuperblock("last inode group overflow"))?;
    if last_inodes > fs.size {
        return Err(UfsError::BadSuperblock(
            "last inode group beyond filesystem",
        ));
    }
    validate_csum(fs, cg_blocks)?;
    Ok(())
}

fn validate_csum(fs: &Superblock, cg_blocks: u64) -> Result<(), UfsError> {
    let c = fs.cstotal;
    if c.ndir < 0 || c.nbfree < 0 || c.nifree < 0 || c.nffree < 0 {
        return Err(UfsError::BadSuperblock("negative summary counter"));
    }
    let free_blocks = (c.nbfree as u64)
        .checked_mul(u64::from(fs.frag))
        .and_then(|v| v.checked_add(c.nffree as u64))
        .ok_or(UfsError::BadSuperblock("free block overflow"))?;
    if free_blocks > fs.size {
        return Err(UfsError::BadSuperblock("free blocks beyond filesystem"));
    }
    let total_inodes = u64::from(fs.ncg)
        .checked_mul(u64::from(fs.ipg))
        .ok_or(UfsError::BadSuperblock("inode count overflow"))?;
    if c.nifree as u64 > total_inodes || c.ndir as u64 > total_inodes {
        return Err(UfsError::BadSuperblock("inode counter beyond filesystem"));
    }
    let _ = cg_blocks;
    Ok(())
}

/// Write the raw superblock to its on-disk location.
pub(crate) fn write_superblock<D: BlockIo>(disk: &mut D, fs: &Superblock) -> Result<(), UfsError> {
    let len = fs.sbsize as usize;
    if len == 0 || len > fs.raw.len() {
        return Err(UfsError::BadSuperblock("bad superblock write size"));
    }
    let raw = &fs.raw[..len];
    disk.write_at(fs.sblockloc, raw).map_err(|_| UfsError::Io)?;
    // NetBSD writes a copy in every cylinder group; keep the backups current too.
    for cg in 0..fs.ncg {
        let frag = fs
            .cgstart(cg)?
            .checked_add(fs.sblkno as u64)
            .ok_or(UfsError::BadSuperblock("superblock backup offset"))?;
        let offset = frag
            .checked_mul(u64::from(fs.fsize))
            .ok_or(UfsError::BadSuperblock("superblock backup offset"))?;
        disk.write_at(offset, raw).map_err(|_| UfsError::Io)?;
    }
    Ok(())
}

/// Load the per-cylinder-group `struct csum` array from `fs_csaddr`.
pub(crate) fn load_summaries<D: BlockIo>(
    disk: &mut D,
    fs: &Superblock,
) -> Result<Vec<Csum32>, UfsError> {
    let count = fs.ncg as usize;
    let needed = count
        .checked_mul(16)
        .ok_or(UfsError::BadSuperblock("summary size overflow"))?;
    if needed > fs.cssize as usize {
        return Err(UfsError::BadSuperblock("summary array too small"));
    }
    let base = u64::from(fs.csaddr)
        .checked_mul(u64::from(fs.fsize))
        .ok_or(UfsError::BadSuperblock("summary offset overflow"))?;
    let mut out = Vec::with_capacity(count);
    for cg in 0..count {
        let offset = base
            .checked_add((cg as u64) * 16)
            .ok_or(UfsError::BadSuperblock("summary offset overflow"))?;
        let mut bytes = [0u8; 16];
        disk.read_at(offset, &mut bytes).map_err(|_| UfsError::Io)?;
        out.push(Csum32::from_le_bytes(&bytes)?);
    }
    Ok(out)
}

/// Write the per-cylinder-group `struct csum` array.
pub(crate) fn write_summaries<D: BlockIo>(
    disk: &mut D,
    fs: &Superblock,
    cs: &[Csum32],
) -> Result<(), UfsError> {
    if cs.len() != fs.ncg as usize {
        return Err(UfsError::Corrupt("summary array length"));
    }
    let base = u64::from(fs.csaddr)
        .checked_mul(u64::from(fs.fsize))
        .ok_or(UfsError::BadSuperblock("summary offset overflow"))?;
    for (cg, entry) in cs.iter().enumerate() {
        let offset = base
            .checked_add((cg as u64) * 16)
            .ok_or(UfsError::BadSuperblock("summary offset overflow"))?;
        disk.write_at(offset, &entry.to_le_bytes())
            .map_err(|_| UfsError::Io)?;
    }
    Ok(())
}

/// `statfs` counters, using the same totals as NetBSD `ffs_statvfs`.
pub(crate) fn statfs(fs: &Superblock) -> StatFs {
    let frag = u64::from(fs.frag);
    let nbfree = u64::try_from(fs.cstotal.nbfree).unwrap_or(0);
    let nffree = u64::try_from(fs.cstotal.nffree).unwrap_or(0);
    let nifree = u64::try_from(fs.cstotal.nifree).unwrap_or(0);
    let blocks_free = nbfree.saturating_mul(frag).saturating_add(nffree);
    let total_inodes = u64::from(fs.ncg).saturating_mul(u64::from(fs.ipg));
    StatFs {
        block_size: fs.bsize,
        fragment_size: fs.fsize,
        blocks: fs.dsize,
        blocks_free,
        files: total_inodes.saturating_sub(u64::from(UFS_ROOTINO)),
        files_free: nifree,
    }
}

fn power_of_two(value: u32) -> bool {
    value != 0 && value & (value - 1) == 0
}

fn log2(value: u32) -> u32 {
    u32::BITS - 1 - value.leading_zeros()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::{
        FS_ISCLEAN, SBLOCK_UFS1, set_i32, set_i64, set_u8, set_u32, set_u64, test_superblock,
    };

    const DEVICE: u64 = 16 * 1024 * 1024;

    fn parsed(ufs2: bool) -> Superblock {
        Superblock::parse(&test_superblock(ufs2), SBLOCK_UFS1).expect("parse")
    }

    fn valid(ufs2: bool) -> Superblock {
        let sb = parsed(ufs2);
        validate(&sb, DEVICE).expect("valid superblock");
        sb
    }

    #[test]
    fn valid_ufs1() {
        let sb = valid(false);
        assert!(!sb.ufs2);
        assert_eq!(sb.bsize, 4096);
    }

    #[test]
    fn valid_ufs2() {
        let sb = valid(true);
        assert!(sb.ufs2);
        assert_eq!(sb.inopb, 16);
    }

    #[test]
    fn bad_superblock_size() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, 104, 100);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn bad_block_size() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, 48, 4000);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn bad_fragment_size() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, 52, 600);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn bad_fragments_per_block() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, 56, 7);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn bad_shift_fields() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, 80, 11);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn bad_masks() {
        let mut buf = test_superblock(false);
        set_i32(&mut buf, 72, 0);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn zero_geometry_fields_rejected() {
        for (off, value) in [(44usize, 0u32), (184, 0), (188, 0), (120, 0), (116, 0)] {
            let mut buf = test_superblock(false);
            set_u32(&mut buf, off, value);
            let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
            assert!(matches!(
                validate(&sb, DEVICE),
                Err(UfsError::BadSuperblock(_))
            ));
        }
    }

    #[test]
    fn zero_maxbpg_rejected() {
        let mut buf = test_superblock(false);
        set_i32(&mut buf, 92, 0);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn inconsistent_inopb_rejected() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, 120, 31);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn inconsistent_nindir_rejected() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, 116, 1000);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn zero_size_rejected() {
        let mut buf = test_superblock(false);
        set_u64(&mut buf, 1080, 0);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn dsize_beyond_size_rejected() {
        let mut buf = test_superblock(false);
        set_u64(&mut buf, 1088, 33000);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn bad_cg_size() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, 160, 500);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn zero_summary_size() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, 156, 0);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn summary_outside_filesystem() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, 1096, 32768);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn bad_cylinder_group_offsets() {
        let mut buf = test_superblock(false);
        set_i32(&mut buf, 20, 56);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn negative_summary_counter() {
        let mut buf = test_superblock(false);
        set_i64(&mut buf, 1008 + 8, -1);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn free_blocks_beyond_filesystem() {
        let mut buf = test_superblock(false);
        set_i64(&mut buf, 1008 + 8, 5000);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn inode_counter_beyond_filesystem() {
        let mut buf = test_superblock(false);
        set_i64(&mut buf, 1008 + 16, 100);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(matches!(
            validate(&sb, DEVICE),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn clean_flag_is_reported() {
        let mut buf = test_superblock(false);
        set_u8(&mut buf, 209, 0);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(!sb.is_clean());
        set_u8(&mut buf, 209, FS_ISCLEAN);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert!(sb.is_clean());
    }

    #[test]
    fn statfs_matches_summary() {
        let sb = valid(false);
        let stats = statfs(&sb);
        assert_eq!(stats.block_size, 4096);
        assert_eq!(stats.fragment_size, 512);
        assert_eq!(stats.blocks, 32000);
        assert_eq!(stats.blocks_free, 800);
        assert_eq!(stats.files, 30);
        assert_eq!(stats.files_free, 10);
    }
}
