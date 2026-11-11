// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Geometry and layout behaviour follows NetBSD usr.sbin/makefs/ffs/mkfs.c
// (revision 36eddebcc36cb0775aa2962005b9c05419a6ff01), which is the same
// implementation used for the B63 golden images. The 4-clause newfs files are
// not copied; only the on-disk definitions in sys/ufs/ffs/fs.h are used.

//! Create an empty UFS1/UFS2 filesystem and mount it for population.

use alloc_crate::vec;
use alloc_crate::vec::Vec;

use crate::blockio::BlockIo;
use crate::dinode::{Inode, encode};
use crate::error::UfsError;
use crate::ffs_bitmap;
use crate::fs::{
    Csum32, FS_44INODEFMT, FS_FLAGS_UPDATED, FS_ISCLEAN, FS_UFS1_MAGIC, FS_UFS2_MAGIC, SBLOCK_UFS1,
    UFS_DIRBLKSIZ,
};
use crate::types::{Ino, MountOptions, Timespec};
use crate::ufs::Ufs;

const FS_OPTTIME: i32 = 0;

/// Options for [`format`].
#[derive(Clone, Copy, Debug)]
pub struct MkfsOptions<'a> {
    /// Image size in bytes; zero uses the device size. Must be a multiple of `block_size`.
    pub size: u64,
    /// UFS version: 1 or 2.
    pub version: u8,
    /// Block size in bytes (4096..65536, power of two).
    pub block_size: u32,
    /// Fragment size in bytes (512..block size, power of two).
    pub fragment_size: u32,
    /// Expected bytes per inode.
    pub bytes_per_inode: u32,
    /// Reserved free-space percentage.
    pub minfree: i32,
    /// Volume label, at most 31 bytes.
    pub label: &'a str,
    /// Filesystem timestamp used for all generated metadata.
    pub timestamp: i64,
}

impl<'a> MkfsOptions<'a> {
    /// A small UFS2 preset used by tests and the CLI when options are omitted.
    pub fn default_ufs2(size: u64, timestamp: i64) -> Self {
        Self {
            size,
            version: 2,
            block_size: 4096,
            fragment_size: 512,
            bytes_per_inode: 4096,
            minfree: 5,
            label: "",
            timestamp,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Geometry {
    ufs2: bool,
    bsize: u32,
    fsize: u32,
    frag: u32,
    bshift: u32,
    fshift: u32,
    fragshift: u32,
    nindir: u32,
    inopb: u32,
    ipg: u32,
    fpg: u32,
    ncg: u32,
    sblkno: i32,
    cblkno: i32,
    iblkno: i32,
    dblkno: i32,
    cgsize: u32,
    cssize: u32,
    sbsize: u32,
    csaddr: u64,
    size: u64,
    dsize: u64,
    maxcontig: u32,
    contigsumsize: u32,
    maxbpg: i32,
    maxfilesize: u64,
    minfree: i32,
    timestamp: i64,
}

fn log2(value: u32) -> u32 {
    u32::BITS - 1 - value.leading_zeros()
}

fn roundup(value: u64, unit: u64) -> u64 {
    if unit == 0 {
        value
    } else {
        value.div_ceil(unit) * unit
    }
}

fn div_ceil(value: u64, unit: u64) -> u64 {
    if unit == 0 { 0 } else { value.div_ceil(unit) }
}

fn cg_size(ipg: u32, fpg: u32, contigsumsize: u32, old_cpg: u32, frag: u32) -> u64 {
    let mut size = 168;
    size += 4;
    size += u64::from(old_cpg) * 4;
    size += u64::from(old_cpg) * 2;
    size += u64::from(ipg).div_ceil(8);
    size += u64::from(fpg).div_ceil(8);
    if contigsumsize > 0 {
        size += u64::from(contigsumsize) * 4;
        size += u64::from(fpg / frag).div_ceil(8);
    }
    size
}

fn validate_options(opts: &MkfsOptions<'_>, device_size: u64) -> Result<Geometry, UfsError> {
    if opts.version != 1 && opts.version != 2 {
        return Err(UfsError::Invalid);
    }
    let bsize = opts.block_size;
    let fsize = opts.fragment_size;
    if !(4096..=65536).contains(&bsize) || !bsize.is_power_of_two() {
        return Err(UfsError::Invalid);
    }
    if !(512..=bsize).contains(&fsize) || !fsize.is_power_of_two() {
        return Err(UfsError::Invalid);
    }
    let frag = bsize / fsize;
    if frag == 0 || frag > 8 || !frag.is_power_of_two() {
        return Err(UfsError::Invalid);
    }
    if !(0..100).contains(&opts.minfree) {
        return Err(UfsError::Invalid);
    }
    if opts.bytes_per_inode < fsize {
        return Err(UfsError::Invalid);
    }
    if opts.label.len() > 31 || opts.label.as_bytes().contains(&0) {
        return Err(UfsError::Invalid);
    }
    let size = if opts.size == 0 {
        device_size
    } else {
        opts.size
    };
    if size == 0 || size > device_size || size % u64::from(bsize) != 0 {
        return Err(UfsError::Invalid);
    }
    let ufs2 = opts.version == 2;
    let inode_size = if ufs2 { 256 } else { 128 };
    let ptr_size = if ufs2 { 8 } else { 4 };
    let inopb = bsize / inode_size;
    let nindir = bsize / ptr_size;
    let maxcontig = (65536 / bsize).clamp(1, 16);
    let contigsumsize = if maxcontig > 1 { maxcontig } else { 0 };
    let sblkno = roundup(
        div_ceil(SBLOCK_UFS1 + 8192, u64::from(fsize)),
        u64::from(frag),
    );
    let cblkno = sblkno + roundup(div_ceil(8192, u64::from(fsize)), u64::from(frag));
    let iblkno = cblkno + u64::from(frag);
    let size_frags = size / u64::from(fsize);
    if size_frags < iblkno + 3 * u64::from(frag) {
        return Err(UfsError::Invalid);
    }
    if !ufs2 && size_frags >= (1u64 << 31) {
        return Err(UfsError::Invalid);
    }
    let old_cpg = if ufs2 { 0 } else { 1 };
    let inopf = inopb >> log2(frag);
    if inopf == 0 {
        return Err(UfsError::Invalid);
    }

    let mut density = opts.bytes_per_inode;
    let mut fragsperinode;
    let mut ipg;
    let mut fpg;
    loop {
        fragsperinode = (u64::from(density) >> log2(fsize)).max(1);
        let mut minfpg = fragsperinode * u64::from(inopb);
        if minfpg > size_frags {
            minfpg = size_frags;
        }
        ipg = u64::from(inopb);
        fpg = roundup(iblkno + ipg / u64::from(inopf), u64::from(frag));
        if fpg < minfpg {
            fpg = minfpg;
        }
        ipg = roundup(div_ceil(fpg, fragsperinode), u64::from(inopb));
        fpg = roundup(iblkno + ipg / u64::from(inopf), u64::from(frag));
        if fpg < minfpg {
            fpg = minfpg;
        }
        ipg = roundup(div_ceil(fpg, fragsperinode), u64::from(inopb));
        if cg_size(ipg as u32, fpg as u32, contigsumsize, old_cpg, frag) < u64::from(bsize) {
            break;
        }
        if density <= fsize {
            return Err(UfsError::Invalid);
        }
        density -= fsize;
    }

    let maxblkspercg = size_frags.saturating_sub(1);
    while fpg < maxblkspercg {
        ipg = roundup(div_ceil(fpg, fragsperinode), u64::from(inopb));
        if size_frags / fpg < 1 {
            break;
        }
        let size_needed = cg_size(ipg as u32, fpg as u32, contigsumsize, old_cpg, frag);
        if size_needed < u64::from(bsize) {
            fpg += u64::from(frag);
            continue;
        }
        if size_needed == u64::from(bsize) {
            break;
        }
        fpg = fpg.saturating_sub(u64::from(frag));
        ipg = roundup(div_ceil(fpg, fragsperinode), u64::from(inopb));
        break;
    }

    loop {
        let lastminfpg = roundup(iblkno + ipg / u64::from(inopf), u64::from(frag));
        if size_frags < lastminfpg {
            return Err(UfsError::Invalid);
        }
        let rem = size_frags % fpg;
        if rem == 0 || rem >= lastminfpg {
            break;
        }
        if fpg <= u64::from(frag) {
            return Err(UfsError::Invalid);
        }
        fpg -= u64::from(frag);
        ipg = roundup(div_ceil(fpg, fragsperinode), u64::from(inopb));
    }

    let ncg = div_ceil(size_frags, fpg);
    if ncg == 0 || ncg > u64::from(u32::MAX) {
        return Err(UfsError::Invalid);
    }
    let dblkno = iblkno + ipg / u64::from(inopf);
    let cgsize = roundup(
        cg_size(ipg as u32, fpg as u32, contigsumsize, old_cpg, frag),
        u64::from(fsize),
    );
    if cgsize > u64::from(bsize) || cgsize < 512 {
        return Err(UfsError::BadSuperblock("mkfs: cylinder group too big"));
    }
    let csaddr = dblkno;
    let cssize = roundup(ncg * 16, u64::from(fsize));
    let sbsize = roundup(1376, u64::from(fsize)).min(8192);
    let csfrags = cssize / u64::from(fsize);
    let dsize = size_frags
        .checked_sub(sblkno)
        .and_then(|value| value.checked_sub(ncg * (dblkno - sblkno)))
        .and_then(|value| value.checked_sub(csfrags))
        .ok_or(UfsError::Invalid)?;
    if dsize == 0 {
        return Err(UfsError::Invalid);
    }
    let mut maxfilesize = u64::from(bsize)
        .checked_mul(12)
        .and_then(|value| value.checked_sub(1))
        .ok_or(UfsError::Invalid)?;
    let mut size_per_block = u64::from(bsize);
    for _ in 0..3 {
        size_per_block = size_per_block
            .checked_mul(u64::from(nindir))
            .ok_or(UfsError::Invalid)?;
        maxfilesize = maxfilesize
            .checked_add(size_per_block)
            .ok_or(UfsError::Invalid)?;
    }

    Ok(Geometry {
        ufs2,
        bsize,
        fsize,
        frag,
        bshift: log2(bsize),
        fshift: log2(fsize),
        fragshift: log2(frag),
        nindir,
        inopb,
        ipg: ipg as u32,
        fpg: fpg as u32,
        ncg: ncg as u32,
        sblkno: i32::try_from(sblkno).map_err(|_| UfsError::Invalid)?,
        cblkno: i32::try_from(cblkno).map_err(|_| UfsError::Invalid)?,
        iblkno: i32::try_from(iblkno).map_err(|_| UfsError::Invalid)?,
        dblkno: i32::try_from(dblkno).map_err(|_| UfsError::Invalid)?,
        cgsize: cgsize as u32,
        cssize: cssize as u32,
        sbsize: sbsize as u32,
        csaddr,
        size: size_frags,
        dsize,
        maxcontig,
        contigsumsize,
        maxbpg: i32::try_from(bsize / 4).map_err(|_| UfsError::Invalid)?,
        maxfilesize,
        minfree: opts.minfree,
        timestamp: opts.timestamp,
    })
}

fn write_u32(buf: &mut [u8], off: usize, value: u32) {
    if let Some(dst) = buf.get_mut(off..off + 4) {
        dst.copy_from_slice(&value.to_le_bytes());
    }
}

fn write_i32(buf: &mut [u8], off: usize, value: i32) {
    write_u32(buf, off, value as u32);
}

fn write_u64(buf: &mut [u8], off: usize, value: u64) {
    if let Some(dst) = buf.get_mut(off..off + 8) {
        dst.copy_from_slice(&value.to_le_bytes());
    }
}

fn write_i64(buf: &mut [u8], off: usize, value: i64) {
    write_u64(buf, off, value as u64);
}

fn set_bit(map: &mut [u8], bit: usize) -> Result<(), UfsError> {
    ffs_bitmap::set_bit(map, bit)
}

fn clear_bit(map: &mut [u8], bit: usize) -> Result<(), UfsError> {
    ffs_bitmap::clear_bit(map, bit)
}

fn is_set(map: &[u8], bit: usize) -> bool {
    ffs_bitmap::bit_is_set(map, bit)
}

struct CgBuild {
    raw: Vec<u8>,
    cs: Csum32,
}

fn build_cg(geom: &Geometry, cg: u32, root_block: Option<u32>) -> Result<CgBuild, UfsError> {
    let cgbase = u64::from(geom.fpg) * u64::from(cg);
    if cgbase >= geom.size {
        return Err(UfsError::Invalid);
    }
    let ndblk = u64::from(geom.fpg).min(geom.size - cgbase) as u32;
    let iused_len = (geom.ipg as usize).div_ceil(8);
    let free_len = (geom.fpg as usize).div_ceil(8);
    let mut iused = vec![0u8; iused_len];
    if cg == 0 {
        set_bit(&mut iused, 0)?;
        set_bit(&mut iused, 1)?;
        set_bit(&mut iused, 2)?;
    }
    let mut free = vec![0u8; free_len];
    if cg > 0 {
        let mut frag = 0u32;
        while frag + geom.frag <= geom.sblkno as u32 {
            for index in 0..geom.frag {
                set_bit(&mut free, (frag + index) as usize)?;
            }
            frag += geom.frag;
        }
    }
    let csfrags = if cg == 0 { geom.cssize / geom.fsize } else { 0 };
    let mut frag = (geom.dblkno as u32).saturating_add(csfrags);
    let misalign = frag % geom.frag;
    if misalign != 0 {
        let limit = frag + (geom.frag - misalign);
        while frag < limit && frag < ndblk {
            set_bit(&mut free, frag as usize)?;
            frag += 1;
        }
    }
    while frag + geom.frag <= ndblk {
        for index in 0..geom.frag {
            set_bit(&mut free, (frag + index) as usize)?;
        }
        frag += geom.frag;
    }
    while frag < ndblk {
        set_bit(&mut free, frag as usize)?;
        frag += 1;
    }
    if let Some(root) = root_block {
        let first = root as usize;
        // The root directory is one 512-byte directory block; it occupies one fragment
        // regardless of the fragment size.
        clear_bit(&mut free, first)?;
    }

    let mut nbfree = 0i32;
    let mut nffree = 0i32;
    let mut frsum = [0i32; 8];
    let blocks = geom.fpg / geom.frag;
    for block in 0..blocks {
        let map = ffs_bitmap::block_map(geom.frag, &free, (block * geom.frag) as usize)?;
        let free_count = map.count_ones();
        if free_count == geom.frag {
            nbfree += 1;
        } else {
            nffree += free_count as i32;
        }
        if free_count > 0 {
            ffs_bitmap::fragacct(geom.frag, map, &mut frsum, 1)?;
        }
    }
    let used_inodes = if cg == 0 { 3 } else { 0 };
    let cs = Csum32 {
        ndir: if cg == 0 { 1 } else { 0 },
        nbfree,
        nifree: geom.ipg as i32 - used_inodes,
        nffree,
    };

    let iusedoff = if geom.ufs2 { 168u32 } else { 174u32 };
    let freeoff = iusedoff + u32::try_from(iused_len).map_err(|_| UfsError::Invalid)?;
    let clustersumoff = if geom.contigsumsize > 0 {
        roundup(u64::from(freeoff) + free_len as u64 - 4, 4) as u32
    } else {
        0
    };
    let clusteroff = if geom.contigsumsize > 0 {
        clustersumoff + (geom.contigsumsize + 1) * 4
    } else {
        0
    };
    let nextfreeoff = if geom.contigsumsize > 0 {
        clusteroff + (geom.fpg / geom.frag).div_ceil(8)
    } else {
        freeoff + free_len as u32
    };
    if nextfreeoff > geom.cgsize {
        return Err(UfsError::BadSuperblock("mkfs: cylinder group overflow"));
    }
    let mut raw = vec![0u8; geom.cgsize as usize];
    write_u32(&mut raw, 4, 0x0009_0255);
    write_i32(
        &mut raw,
        8,
        if geom.ufs2 { 0 } else { geom.timestamp as i32 },
    );
    write_u32(&mut raw, 12, cg);
    if geom.ufs2 {
        write_u32(&mut raw, 136, geom.timestamp as u32);
    } else {
        write_i32(&mut raw, 16, 1);
        write_i32(&mut raw, 18, geom.ipg as i32);
    }
    write_u32(&mut raw, 20, ndblk);
    if let Some(slot) = raw.get_mut(24..40) {
        slot.copy_from_slice(&cs.to_le_bytes());
    }
    for (index, value) in frsum.iter().enumerate() {
        write_i32(&mut raw, 52 + index * 4, *value);
    }
    write_i32(&mut raw, 84, if geom.ufs2 { 0 } else { 168 });
    write_i32(&mut raw, 88, if geom.ufs2 { 0 } else { 172 });
    write_u32(&mut raw, 92, iusedoff);
    write_u32(&mut raw, 96, freeoff);
    write_u32(&mut raw, 100, nextfreeoff);
    write_u32(&mut raw, 104, clustersumoff);
    write_u32(&mut raw, 108, clusteroff);
    write_u32(&mut raw, 112, ndblk / geom.frag);
    let inited = if geom.ufs2 {
        geom.ipg.min(2 * geom.inopb)
    } else {
        0
    };
    write_u32(&mut raw, 116, if geom.ufs2 { geom.ipg } else { 0 });
    write_u32(&mut raw, 120, inited);
    if let Some(dst) = raw.get_mut(iusedoff as usize..iusedoff as usize + iused_len) {
        dst.copy_from_slice(&iused);
    }
    if let Some(dst) = raw.get_mut(freeoff as usize..freeoff as usize + free_len) {
        dst.copy_from_slice(&free);
    }
    if geom.contigsumsize > 0 {
        let ncluster = (ndblk / geom.frag) as usize;
        let mut clusters = vec![0u8; ncluster.div_ceil(8)];
        for block in 0..ncluster {
            let map = ffs_bitmap::block_map(geom.frag, &free, block * geom.frag as usize)?;
            if map.count_ones() == geom.frag {
                set_bit(&mut clusters, block)?;
            }
        }
        let mut sums = vec![0i32; geom.contigsumsize as usize + 1];
        let mut run = 0usize;
        for block in 0..ncluster {
            if is_set(&clusters, block) {
                run += 1;
            } else if run > 0 {
                let slot = run.min(geom.contigsumsize as usize);
                if let Some(value) = sums.get_mut(slot) {
                    *value += 1;
                }
                run = 0;
            }
        }
        if run > 0 {
            let slot = run.min(geom.contigsumsize as usize);
            if let Some(value) = sums.get_mut(slot) {
                *value += 1;
            }
        }
        for (index, value) in sums.iter().enumerate().skip(1) {
            write_i32(&mut raw, clustersumoff as usize + index * 4, *value);
        }
        if let Some(dst) = raw.get_mut(clusteroff as usize..clusteroff as usize + clusters.len()) {
            dst.copy_from_slice(&clusters);
        }
    }
    Ok(CgBuild { raw, cs })
}

fn write_superblock(geom: &Geometry, opts: &MkfsOptions<'_>, cs: &[Csum32]) -> Vec<u8> {
    let mut raw = vec![0u8; 8192];
    write_i32(&mut raw, 8, geom.sblkno);
    write_i32(&mut raw, 12, geom.cblkno);
    write_i32(&mut raw, 16, geom.iblkno);
    write_i32(&mut raw, 20, geom.dblkno);
    if !geom.ufs2 {
        write_i32(&mut raw, 32, geom.timestamp as i32);
        write_i32(&mut raw, 36, geom.size as i32);
        write_i32(&mut raw, 40, geom.dsize as i32);
        write_i32(&mut raw, 24, 0);
        write_i32(&mut raw, 28, -1);
    }
    write_u32(&mut raw, 44, geom.ncg);
    write_u32(&mut raw, 48, geom.bsize);
    write_u32(&mut raw, 52, geom.fsize);
    write_u32(&mut raw, 56, geom.frag);
    write_i32(&mut raw, 60, geom.minfree);
    write_i32(&mut raw, 64, 0);
    write_i32(&mut raw, 68, if geom.ufs2 { 0 } else { 60 });
    write_i32(&mut raw, 72, !((geom.bsize - 1) as i32));
    write_i32(&mut raw, 76, !((geom.fsize - 1) as i32));
    write_u32(&mut raw, 80, geom.bshift);
    write_u32(&mut raw, 84, geom.fshift);
    write_u32(&mut raw, 88, geom.maxcontig);
    write_i32(&mut raw, 92, geom.maxbpg);
    write_u32(&mut raw, 96, geom.fragshift);
    write_u32(&mut raw, 100, log2(geom.fsize / 512));
    write_u32(&mut raw, 104, geom.sbsize);
    write_u32(&mut raw, 116, geom.nindir);
    write_u32(&mut raw, 120, geom.inopb);
    if !geom.ufs2 {
        write_i32(&mut raw, 124, (geom.fsize / 512) as i32);
    }
    write_i32(&mut raw, 128, FS_OPTTIME);
    if !geom.ufs2 {
        write_i32(&mut raw, 132, 0);
        write_i32(&mut raw, 136, 1);
        write_i32(&mut raw, 140, 0);
        write_i32(&mut raw, 144, geom.timestamp as i32);
        write_i32(&mut raw, 148, 0x5a17_0001);
        write_i32(&mut raw, 152, geom.csaddr as i32);
        let spc = geom.fpg as i32 * (geom.fsize / 512) as i32;
        write_i32(&mut raw, 132, spc);
        write_i32(&mut raw, 168, spc);
        write_i32(&mut raw, 172, spc);
        write_i32(&mut raw, 176, geom.ncg as i32);
        write_i32(&mut raw, 180, 1);
    } else {
        write_u32(&mut raw, 144, geom.timestamp as u32);
        write_u32(&mut raw, 148, 0x5a17_0001);
    }
    write_u32(&mut raw, 156, geom.cssize);
    write_u32(&mut raw, 160, geom.cgsize);
    if !geom.ufs2 {
        write_i32(&mut raw, 168, geom.fpg as i32 * (geom.fsize / 512) as i32);
        write_i32(&mut raw, 172, geom.fpg as i32 * (geom.fsize / 512) as i32);
        write_i32(&mut raw, 176, geom.ncg as i32);
        write_i32(&mut raw, 180, 1);
    }
    write_u32(&mut raw, 184, geom.ipg);
    write_i32(&mut raw, 188, geom.fpg as i32);
    if let Some(slot) = raw.get_mut(192..208) {
        slot[0..4].copy_from_slice(&0i32.to_le_bytes());
        slot[4..8].copy_from_slice(&0i32.to_le_bytes());
        slot[8..12].copy_from_slice(&0i32.to_le_bytes());
        slot[12..16].copy_from_slice(&0i32.to_le_bytes());
    }
    raw[208] = 0;
    raw[209] = FS_ISCLEAN;
    raw[210] = 0;
    raw[211] = FS_FLAGS_UPDATED;
    if let Some(dst) = raw.get_mut(680..712) {
        let label = opts.label.as_bytes();
        dst[..label.len()].copy_from_slice(label);
    }
    write_u32(&mut raw, 860, geom.bsize);
    write_u64(&mut raw, 1000, SBLOCK_UFS1);
    let total = cs.iter().fold((0i64, 0i64, 0i64, 0i64), |acc, entry| {
        (
            acc.0 + i64::from(entry.ndir),
            acc.1 + i64::from(entry.nbfree),
            acc.2 + i64::from(entry.nifree),
            acc.3 + i64::from(entry.nffree),
        )
    });
    write_i64(&mut raw, 1008, total.0);
    write_i64(&mut raw, 1016, total.1);
    write_i64(&mut raw, 1024, total.2);
    write_i64(&mut raw, 1032, total.3);
    if !geom.ufs2 {
        write_i32(&mut raw, 192, total.0 as i32);
        write_i32(&mut raw, 196, total.1 as i32);
        write_i32(&mut raw, 200, total.2 as i32);
        write_i32(&mut raw, 204, total.3 as i32);
    }
    write_i64(&mut raw, 1072, geom.timestamp);
    write_u64(&mut raw, 1080, geom.size);
    write_u64(&mut raw, 1088, geom.dsize);
    write_u64(&mut raw, 1096, geom.csaddr);
    write_u32(&mut raw, 1196, 16384);
    write_u32(&mut raw, 1200, 64);
    write_u32(&mut raw, 1312, 0);
    write_i32(&mut raw, 1316, geom.contigsumsize as i32);
    write_i32(&mut raw, 1320, if geom.ufs2 { 120 } else { 60 });
    write_i32(&mut raw, 1324, FS_44INODEFMT);
    write_u64(&mut raw, 1328, geom.maxfilesize);
    write_u64(&mut raw, 1336, !(u64::from(geom.bsize) - 1));
    write_u64(&mut raw, 1344, !(u64::from(geom.fsize) - 1));
    write_i32(&mut raw, 1352, 0);
    write_i32(&mut raw, 1356, if geom.ufs2 { 0 } else { 1 });
    write_i32(&mut raw, 1360, if geom.ufs2 { 0 } else { 1 });
    write_u32(
        &mut raw,
        1372,
        if geom.ufs2 {
            FS_UFS2_MAGIC
        } else {
            FS_UFS1_MAGIC
        },
    );
    raw
}

fn write_zeroes<D: BlockIo>(disk: &mut D, size: u64) -> Result<(), UfsError> {
    let chunk = 1024 * 1024;
    let zero = vec![0u8; chunk];
    let mut offset = 0u64;
    while offset < size {
        let len =
            usize::try_from((size - offset).min(chunk as u64)).map_err(|_| UfsError::Invalid)?;
        disk.write_at(offset, &zero[..len])
            .map_err(|_| UfsError::Io)?;
        offset += len as u64;
    }
    Ok(())
}

fn dot_block(root: Ino) -> Vec<u8> {
    let mut block = vec![0u8; 512];
    let entries: [(Ino, &[u8], u16); 2] = [(root, b".", 12), (root, b"..", 500)];
    let mut off = 0usize;
    for (ino, name, reclen) in entries {
        block[off..off + 4].copy_from_slice(&ino.to_le_bytes());
        block[off + 4..off + 6].copy_from_slice(&reclen.to_le_bytes());
        block[off + 6] = 4;
        block[off + 7] = name.len() as u8;
        block[off + 8..off + 8 + name.len()].copy_from_slice(name);
        off += reclen as usize;
    }
    block
}

/// Format `disk` and create `/lost+found`, returning a writable mount.
pub fn format<D: BlockIo>(mut disk: D, opts: &MkfsOptions<'_>) -> Result<Ufs<D>, UfsError> {
    let device_size = disk.size_bytes();
    let geom = validate_options(opts, device_size)?;
    write_zeroes(&mut disk, geom.size * u64::from(geom.fsize))?;

    let mut cg0 = build_cg(&geom, 0, None)?;
    let blocks = geom.fpg / geom.frag;
    let mut root_block = None;
    for block in 0..blocks {
        let map = ffs_bitmap::block_map(
            geom.frag,
            {
                let freeoff = if geom.ufs2 { 168 } else { 174 };
                let iused_len = (geom.ipg as usize).div_ceil(8);
                let start = (freeoff + iused_len as u32) as usize;
                &cg0.raw[start..start + (geom.fpg as usize).div_ceil(8)]
            },
            block as usize * geom.frag as usize,
        )?;
        if map == 0xff {
            root_block = Some(block * geom.frag);
            break;
        }
    }
    let root_block = root_block.ok_or(UfsError::NoSpace)?;
    cg0 = build_cg(&geom, 0, Some(root_block))?;
    let mut cgs = vec![cg0];
    for cg in 1..geom.ncg {
        cgs.push(build_cg(&geom, cg, None)?);
    }
    let summaries: Vec<Csum32> = cgs.iter().map(|cg| cg.cs).collect();
    let mut superblock = write_superblock(&geom, opts, &summaries);

    for cg in &cgs {
        let cgno = superblock_cg(&cg.raw)?;
        let base = u64::from(geom.fpg) * u64::from(cgno);
        let off = (base + geom.cblkno as u64)
            .checked_mul(u64::from(geom.fsize))
            .ok_or(UfsError::Invalid)?;
        disk.write_at(off, &cg.raw).map_err(|_| UfsError::Io)?;
    }
    let summary_off = geom
        .csaddr
        .checked_mul(u64::from(geom.fsize))
        .ok_or(UfsError::Invalid)?;
    let mut summary_bytes = vec![0u8; geom.cssize as usize];
    for (index, entry) in summaries.iter().enumerate() {
        summary_bytes[index * 16..index * 16 + 16].copy_from_slice(&entry.to_le_bytes());
    }
    disk.write_at(summary_off, &summary_bytes)
        .map_err(|_| UfsError::Io)?;

    let inode_size = if geom.ufs2 { 256 } else { 128 };
    let root_inode = Inode {
        raw: Vec::new(),
        mode: 0o040755,
        nlink: 2,
        uid: 0,
        gid: 0,
        size: UFS_DIRBLKSIZ as u64,
        blocks: u64::from(geom.fsize / 512),
        atime: Timespec {
            sec: geom.timestamp,
            nsec: 0,
        },
        mtime: Timespec {
            sec: geom.timestamp,
            nsec: 0,
        },
        ctime: Timespec {
            sec: geom.timestamp,
            nsec: 0,
        },
        flags: 0,
        r#gen: 0x5a17_0001,
        rdev: 0,
        db: [0; 12],
        ib: [0; 3],
    };
    let mut root_inode = root_inode;
    root_inode.db[0] = u64::from(root_block);
    let mut inode_block = vec![0u8; geom.bsize as usize];
    let slot = 2usize.checked_mul(inode_size).ok_or(UfsError::Invalid)?;
    encode(
        geom.ufs2,
        inode_block
            .get_mut(slot..slot + inode_size)
            .ok_or(UfsError::Invalid)?,
        &root_inode,
    )?;
    let inode_off = (geom.iblkno as u64)
        .checked_mul(u64::from(geom.fsize))
        .ok_or(UfsError::Invalid)?;
    disk.write_at(inode_off, &inode_block)
        .map_err(|_| UfsError::Io)?;

    let root_off = u64::from(root_block)
        .checked_mul(u64::from(geom.fsize))
        .ok_or(UfsError::Invalid)?;
    let root_len = ((UFS_DIRBLKSIZ as u32).div_ceil(geom.fsize) * geom.fsize) as usize;
    let mut root_data = vec![0u8; root_len];
    let dots = dot_block(crate::fs::UFS_ROOTINO);
    root_data[..dots.len()].copy_from_slice(&dots);
    disk.write_at(root_off, &root_data)
        .map_err(|_| UfsError::Io)?;

    for cg in 0..geom.ncg {
        let base = u64::from(geom.fpg) * u64::from(cg);
        let off = (base + geom.sblkno as u64)
            .checked_mul(u64::from(geom.fsize))
            .ok_or(UfsError::Invalid)?;
        disk.write_at(off, &superblock).map_err(|_| UfsError::Io)?;
    }
    disk.write_at(SBLOCK_UFS1, &superblock)
        .map_err(|_| UfsError::Io)?;
    let _ = &mut superblock;

    let mut ufs = Ufs::mount(disk, MountOptions { read_only: false })?;
    ufs.mkdir(crate::fs::UFS_ROOTINO, b"lost+found", 0o700, 0, 0)?;
    ufs.sync()?;
    Ok(ufs)
}

fn superblock_cg(raw: &[u8]) -> Result<u32, UfsError> {
    raw.get(12..16)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .ok_or(UfsError::Corrupt("cylinder group index"))
}
