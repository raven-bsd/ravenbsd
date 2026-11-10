// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ffs/ffs_alloc.c (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notice follows.
//
// Copyright (c) 1982, 1986, 1989, 1993
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

//! Cylinder-group allocation, fragment accounting and writable-mount bookkeeping.

use alloc_crate::format;
use alloc_crate::string::String;
use alloc_crate::vec;
use alloc_crate::vec::Vec;

use crate::blockio::BlockIo;
use crate::error::UfsError;
use crate::ffs_bitmap;
use crate::ffs_vfsops;
use crate::fs::{Csum, Csum32, Superblock, UFS_ROOTINO};
use crate::types::{FileType, Ino};
use crate::ufs::Ufs;

const CG_MAGIC: u32 = 0x0009_0255;
const CG_HDR: usize = 168;
const CG_MAGIC_OFF: usize = 4;
const CG_CGX_OFF: usize = 12;
const CG_NDBLK_OFF: usize = 20;
const CG_CS_OFF: usize = 24;
const CG_ROTOR_OFF: usize = 40;
const CG_FROTOR_OFF: usize = 44;
const CG_IROTOR_OFF: usize = 48;
const CG_FRSUM_OFF: usize = 52;
const CG_IUSEDOFF_OFF: usize = 92;
const CG_FREEOFF_OFF: usize = 96;
const CG_CLUSTERSUMOFF_OFF: usize = 104;
const CG_CLUSTEROFF_OFF: usize = 108;
const CG_NCLUSTERBLKS_OFF: usize = 112;
const CG_NIBLK_OFF: usize = 116;
const CG_INITEDIBLK_OFF: usize = 120;

const FS_OPTTIME: i32 = 0;
const FS_OPTSPACE: i32 = 1;

/// A decoded cylinder group with its raw on-disk bytes.
#[derive(Clone, Debug)]
pub(crate) struct CylinderGroup {
    pub(crate) raw: Vec<u8>,
    pub(crate) cgx: u32,
    pub(crate) ndblk: u32,
    pub(crate) cs: Csum32,
    pub(crate) rotor: u32,
    pub(crate) frotor: u32,
    pub(crate) irotor: u32,
    pub(crate) frsum: [i32; 8],
    pub(crate) iusedoff: u32,
    pub(crate) freeoff: u32,
    pub(crate) clustersumoff: u32,
    pub(crate) clusteroff: u32,
    pub(crate) nclusterblks: u32,
    pub(crate) niblk: u32,
    pub(crate) initediblk: u32,
}

impl CylinderGroup {
    pub(crate) fn read<D: BlockIo>(
        disk: &mut D,
        fs: &Superblock,
        cg: u32,
    ) -> Result<Self, UfsError> {
        if cg >= fs.ncg {
            return Err(UfsError::Invalid);
        }
        let cgtod = fs
            .cgstart(cg)?
            .checked_add(fs.cblkno as u64)
            .ok_or(UfsError::BadSuperblock("cylinder group offset"))?;
        let offset = cgtod
            .checked_mul(u64::from(fs.fsize))
            .ok_or(UfsError::Corrupt("cylinder group offset"))?;
        let size = fs.cgsize as usize;
        let end = offset
            .checked_add(size as u64)
            .ok_or(UfsError::Corrupt("cylinder group offset"))?;
        if end > disk.size_bytes() {
            return Err(UfsError::Corrupt("cylinder group beyond device"));
        }
        let mut raw = vec![0u8; size];
        disk.read_at(offset, &mut raw).map_err(|_| UfsError::Io)?;
        Self::decode(raw, cg, fs)
    }

    fn decode(raw: Vec<u8>, cg: u32, fs: &Superblock) -> Result<Self, UfsError> {
        let magic = read_u32(&raw, CG_MAGIC_OFF)?;
        if magic != CG_MAGIC {
            return Err(UfsError::Corrupt("cylinder group magic"));
        }
        let cgx = read_u32(&raw, CG_CGX_OFF)?;
        if cgx != cg {
            return Err(UfsError::Corrupt("cylinder group index"));
        }
        let ndblk = read_u32(&raw, CG_NDBLK_OFF)?;
        let cs = Csum32::from_le_bytes(read_slice(&raw, CG_CS_OFF, 16)?)?;
        if cs.ndir < 0 || cs.nbfree < 0 || cs.nifree < 0 || cs.nffree < 0 {
            return Err(UfsError::Corrupt("negative cylinder group counters"));
        }
        let mut frsum = [0i32; 8];
        for (i, slot) in frsum.iter_mut().enumerate() {
            *slot = read_u32(&raw, CG_FRSUM_OFF + i * 4)? as i32;
            if *slot < 0 {
                return Err(UfsError::Corrupt("negative fragment histogram"));
            }
        }
        let iusedoff = read_u32(&raw, CG_IUSEDOFF_OFF)?;
        let freeoff = read_u32(&raw, CG_FREEOFF_OFF)?;
        let clustersumoff = read_u32(&raw, CG_CLUSTERSUMOFF_OFF)?;
        let clusteroff = read_u32(&raw, CG_CLUSTEROFF_OFF)?;
        let nclusterblks = read_u32(&raw, CG_NCLUSTERBLKS_OFF)?;
        let niblk = read_u32(&raw, CG_NIBLK_OFF)?;
        let initediblk = read_u32(&raw, CG_INITEDIBLK_OFF)?;
        let rotor = read_u32(&raw, CG_ROTOR_OFF)?;
        let frotor = read_u32(&raw, CG_FROTOR_OFF)?;
        let irotor = read_u32(&raw, CG_IROTOR_OFF)?;

        let size = raw.len();
        let iused_end = (iusedoff as usize)
            .checked_add((fs.ipg as usize).div_ceil(8))
            .ok_or(UfsError::Corrupt("inode map offset"))?;
        let free_end = (freeoff as usize)
            .checked_add((fs.fpg as usize).div_ceil(8))
            .ok_or(UfsError::Corrupt("free map offset"))?;
        if (iusedoff as usize) < CG_HDR
            || (freeoff as usize) < CG_HDR
            || iused_end > size
            || free_end > size
        {
            return Err(UfsError::Corrupt("cylinder group map bounds"));
        }
        if fs.contigsumsize > 0 {
            let sum_end = (clustersumoff as usize)
                .checked_add((fs.contigsumsize as usize + 1) * 4)
                .ok_or(UfsError::Corrupt("cluster summary offset"))?;
            let map_end = (clusteroff as usize)
                .checked_add((nclusterblks as usize).div_ceil(8))
                .ok_or(UfsError::Corrupt("cluster map offset"))?;
            if (clustersumoff as usize) < CG_HDR
                || (clusteroff as usize) < CG_HDR
                || sum_end > size
                || map_end > size
            {
                return Err(UfsError::Corrupt("cluster map bounds"));
            }
        }
        if niblk > fs.ipg || initediblk > niblk || ndblk > fs.fpg {
            return Err(UfsError::Corrupt("inode/block count"));
        }
        Ok(Self {
            raw,
            cgx,
            ndblk,
            cs,
            rotor,
            frotor,
            irotor,
            frsum,
            iusedoff,
            freeoff,
            clustersumoff,
            clusteroff,
            nclusterblks,
            niblk,
            initediblk,
        })
    }

    pub(crate) fn write<D: BlockIo>(
        &mut self,
        disk: &mut D,
        fs: &Superblock,
    ) -> Result<(), UfsError> {
        if self.raw.len() != fs.cgsize as usize {
            return Err(UfsError::Corrupt("cylinder group size"));
        }
        self.encode()?;
        let cgtod = fs
            .cgstart(self.cgx)?
            .checked_add(fs.cblkno as u64)
            .ok_or(UfsError::BadSuperblock("cylinder group offset"))?;
        let offset = cgtod
            .checked_mul(u64::from(fs.fsize))
            .ok_or(UfsError::Corrupt("cylinder group offset"))?;
        disk.write_at(offset, &self.raw).map_err(|_| UfsError::Io)
    }

    fn encode(&mut self) -> Result<(), UfsError> {
        write_u32(&mut self.raw, CG_CGX_OFF, self.cgx)?;
        write_u32(&mut self.raw, CG_NDBLK_OFF, self.ndblk)?;
        write_u32(&mut self.raw, CG_ROTOR_OFF, self.rotor)?;
        write_u32(&mut self.raw, CG_FROTOR_OFF, self.frotor)?;
        write_u32(&mut self.raw, CG_IROTOR_OFF, self.irotor)?;
        write_u32(&mut self.raw, CG_NIBLK_OFF, self.niblk)?;
        write_u32(&mut self.raw, CG_INITEDIBLK_OFF, self.initediblk)?;
        write_u32(&mut self.raw, CG_NCLUSTERBLKS_OFF, self.nclusterblks)?;
        let cs = self.cs.to_le_bytes();
        let dst = self
            .raw
            .get_mut(CG_CS_OFF..CG_CS_OFF + 16)
            .ok_or(UfsError::Corrupt("cylinder group counters"))?;
        dst.copy_from_slice(&cs);
        for (i, value) in self.frsum.iter().enumerate() {
            write_u32(&mut self.raw, CG_FRSUM_OFF + i * 4, *value as u32)?;
        }
        write_u32(&mut self.raw, CG_IUSEDOFF_OFF, self.iusedoff)?;
        write_u32(&mut self.raw, CG_FREEOFF_OFF, self.freeoff)?;
        write_u32(&mut self.raw, CG_CLUSTERSUMOFF_OFF, self.clustersumoff)?;
        write_u32(&mut self.raw, CG_CLUSTEROFF_OFF, self.clusteroff)?;
        Ok(())
    }

    fn inosused(&self, fs: &Superblock) -> Result<&[u8], UfsError> {
        let start = self.iusedoff as usize;
        let end = start
            .checked_add((fs.ipg as usize).div_ceil(8))
            .ok_or(UfsError::Corrupt("inode map"))?;
        self.raw
            .get(start..end)
            .ok_or(UfsError::Corrupt("inode map"))
    }

    fn inosused_mut(&mut self, fs: &Superblock) -> Result<&mut [u8], UfsError> {
        let start = self.iusedoff as usize;
        let end = start
            .checked_add((fs.ipg as usize).div_ceil(8))
            .ok_or(UfsError::Corrupt("inode map"))?;
        self.raw
            .get_mut(start..end)
            .ok_or(UfsError::Corrupt("inode map"))
    }

    fn blksfree(&self, fs: &Superblock) -> Result<&[u8], UfsError> {
        let start = self.freeoff as usize;
        let end = start
            .checked_add((fs.fpg as usize).div_ceil(8))
            .ok_or(UfsError::Corrupt("free map"))?;
        self.raw
            .get(start..end)
            .ok_or(UfsError::Corrupt("free map"))
    }

    fn blksfree_mut(&mut self, fs: &Superblock) -> Result<&mut [u8], UfsError> {
        let start = self.freeoff as usize;
        let end = start
            .checked_add((fs.fpg as usize).div_ceil(8))
            .ok_or(UfsError::Corrupt("free map"))?;
        self.raw
            .get_mut(start..end)
            .ok_or(UfsError::Corrupt("free map"))
    }

    fn inode_used(&self, fs: &Superblock, index: u32) -> Result<bool, UfsError> {
        Ok(ffs_bitmap::bit_is_set(self.inosused(fs)?, index as usize))
    }

    fn set_inode_used(&mut self, fs: &Superblock, index: u32) -> Result<(), UfsError> {
        let map = self.inosused_mut(fs)?;
        ffs_bitmap::set_bit(map, index as usize)
    }

    fn clear_inode_used(&mut self, fs: &Superblock, index: u32) -> Result<(), UfsError> {
        let map = self.inosused_mut(fs)?;
        ffs_bitmap::clear_bit(map, index as usize)
    }

    fn block_free(&self, fs: &Superblock, fragno: u32) -> Result<bool, UfsError> {
        Ok(ffs_bitmap::bit_is_set(self.blksfree(fs)?, fragno as usize))
    }

    fn set_block_free(&mut self, fs: &Superblock, fragno: u32) -> Result<(), UfsError> {
        let map = self.blksfree_mut(fs)?;
        ffs_bitmap::set_bit(map, fragno as usize)
    }

    fn clear_block_free(&mut self, fs: &Superblock, fragno: u32) -> Result<(), UfsError> {
        let map = self.blksfree_mut(fs)?;
        ffs_bitmap::clear_bit(map, fragno as usize)
    }

    fn recompute_blocks(&mut self, fs: &Superblock) -> Result<(), UfsError> {
        let frag = fs.frag;
        let blocks = (fs.fpg / frag) as usize;
        let mut nbfree = 0i32;
        let mut nffree = 0i32;
        let mut frsum = [0i32; 8];
        for block in 0..blocks {
            let bno = (block as u32) * frag;
            let map = ffs_bitmap::block_map(frag, self.blksfree(fs)?, bno as usize)?;
            let free = map.count_ones();
            if free == frag {
                nbfree += 1;
            } else {
                nffree += free as i32;
            }
            if free > 0 {
                ffs_bitmap::fragacct(frag, map, &mut frsum, 1)?;
            }
        }
        self.cs.nbfree = nbfree;
        self.cs.nffree = nffree;
        self.frsum = frsum;
        self.recompute_clusters(fs)?;
        Ok(())
    }

    fn recompute_clusters(&mut self, fs: &Superblock) -> Result<(), UfsError> {
        if fs.contigsumsize <= 0 {
            return Ok(());
        }
        let ncluster = self.nclusterblks.min(fs.fpg / fs.frag);
        let mut bits = vec![0u8; (ncluster as usize).div_ceil(8)];
        for block in 0..ncluster {
            let map = ffs_bitmap::block_map(
                fs.frag,
                self.blksfree(fs)?,
                (block as usize) * fs.frag as usize,
            )?;
            if map.count_ones() == fs.frag {
                ffs_bitmap::set_bit(&mut bits, block as usize)?;
            }
        }
        let start = self.clusteroff as usize;
        let end = start
            .checked_add(bits.len())
            .ok_or(UfsError::Corrupt("cluster map"))?;
        let dst = self
            .raw
            .get_mut(start..end)
            .ok_or(UfsError::Corrupt("cluster map"))?;
        dst.copy_from_slice(&bits);
        let mut sums = vec![0i32; fs.contigsumsize as usize + 1];
        let mut run = 0usize;
        for block in 0..ncluster as usize {
            let free = ffs_bitmap::bit_is_set(&bits, block);
            if free {
                run += 1;
            } else if run > 0 {
                let slot = run.min(fs.contigsumsize as usize);
                if let Some(value) = sums.get_mut(slot) {
                    *value += 1;
                }
                run = 0;
            }
        }
        if run > 0 {
            let slot = run.min(fs.contigsumsize as usize);
            if let Some(value) = sums.get_mut(slot) {
                *value += 1;
            }
        }
        let start = self.clustersumoff as usize;
        for (index, value) in sums.iter().enumerate().skip(1) {
            write_u32(&mut self.raw, start + index * 4, *value as u32)?;
        }
        Ok(())
    }
}

fn read_slice(buf: &[u8], off: usize, len: usize) -> Result<&[u8], UfsError> {
    buf.get(off..off.checked_add(len).ok_or(UfsError::Corrupt("offset"))?)
        .ok_or(UfsError::Corrupt("truncated cylinder group"))
}

fn read_u32(buf: &[u8], off: usize) -> Result<u32, UfsError> {
    let bytes = read_slice(buf, off, 4)?;
    let mut value = [0u8; 4];
    value.copy_from_slice(bytes);
    Ok(u32::from_le_bytes(value))
}

fn write_u32(buf: &mut [u8], off: usize, value: u32) -> Result<(), UfsError> {
    let dst = buf
        .get_mut(off..off.checked_add(4).ok_or(UfsError::Corrupt("offset"))?)
        .ok_or(UfsError::Corrupt("truncated cylinder group"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn csum_total(cs: &[Csum32]) -> Csum {
    let mut total = Csum::default();
    for entry in cs {
        total.ndir += i64::from(entry.ndir);
        total.nbfree += i64::from(entry.nbfree);
        total.nifree += i64::from(entry.nifree);
        total.nffree += i64::from(entry.nffree);
    }
    total
}

fn ino_to_cg(fs: &Superblock, ino: Ino) -> u32 {
    if fs.ipg == 0 {
        return 0;
    }
    (ino / fs.ipg).min(fs.ncg.saturating_sub(1))
}

impl<D: BlockIo> Ufs<D> {
    fn ensure_writable(&self) -> Result<(), UfsError> {
        if self.read_only {
            Err(UfsError::ReadOnly)
        } else {
            Ok(())
        }
    }

    fn load_cg(&mut self, cg: u32) -> Result<CylinderGroup, UfsError> {
        CylinderGroup::read(&mut self.disk, &self.fs, cg)
    }

    fn store_cg(&mut self, cg: u32, group: &mut CylinderGroup) -> Result<(), UfsError> {
        group.write(&mut self.disk, &self.fs)?;
        if !self.dirty_cgs.contains(&cg) {
            self.dirty_cgs.push(cg);
        }
        Ok(())
    }

    fn update_group(&mut self, cg: u32, group: &mut CylinderGroup) -> Result<(), UfsError> {
        self.store_cg(cg, group)?;
        if let Some(slot) = self.cs.get_mut(cg as usize) {
            *slot = group.cs;
        } else {
            return Err(UfsError::Corrupt("summary index"));
        }
        self.fs.cstotal = csum_total(&self.cs);
        Ok(())
    }

    /// Allocate a block or fragment. `size` is a byte multiple of the fragment size and at most
    /// one block. The returned address is a fragment number.
    pub fn alloc_block(
        &mut self,
        owner: Ino,
        lbn: u64,
        pref: Option<u64>,
        size: u32,
    ) -> Result<u64, UfsError> {
        self.ensure_writable()?;
        self.validate_alloc_size(size)?;
        let _ = lbn;
        let bpref = pref.unwrap_or(0);
        let bpref = if bpref >= self.fs.size { 0 } else { bpref };
        let cg = if bpref == 0 {
            ino_to_cg(&self.fs, owner)
        } else {
            (bpref / u64::from(self.fs.fpg)) as u32
        };
        if !self.has_room(size)? {
            return Err(UfsError::NoSpace);
        }
        let mut allocator = |ufs: &mut Self, cg: u32, pref: u64| ufs.alloccg(cg, pref, size);
        let address = self.hash_alloc(cg, bpref, &mut allocator)?;
        address.ok_or(UfsError::NoSpace)
    }

    /// Extend a fragment range in place, or move it to a new allocation.
    pub fn realloc_frags(
        &mut self,
        owner: Ino,
        lbn: u64,
        old: u64,
        old_size: u32,
        new_size: u32,
    ) -> Result<u64, UfsError> {
        self.ensure_writable()?;
        self.validate_alloc_size(old_size)?;
        self.validate_alloc_size(new_size)?;
        let _ = (owner, lbn);
        if old == 0 || old >= self.fs.size {
            return Err(UfsError::Corrupt("bad reallocation address"));
        }
        if new_size <= old_size {
            if new_size < old_size {
                self.free_frags(
                    old + u64::from(new_size / self.fs.fsize),
                    old_size - new_size,
                )?;
            }
            return Ok(old);
        }
        let cg = (old / u64::from(self.fs.fpg)) as u32;
        if let Some(group) = self.fragextend(cg, old, old_size, new_size)? {
            return Ok(group);
        }
        if !self.has_room(new_size)? {
            return Err(UfsError::NoSpace);
        }
        let request = self.realloc_request(new_size);
        let mut allocator = |ufs: &mut Self, cg: u32, pref: u64| ufs.alloccg(cg, pref, request);
        let cg_pref = if old < self.fs.size { old } else { 0 };
        let new = self
            .hash_alloc(cg, cg_pref, &mut allocator)?
            .ok_or(UfsError::NoSpace)?;
        if request > new_size {
            self.free_frags(
                new + u64::from(new_size / self.fs.fsize),
                request - new_size,
            )?;
        }
        self.free_frags(old, old_size)?;
        Ok(new)
    }

    /// Free a block or fragment range.
    pub fn free_frags(&mut self, addr: u64, size: u32) -> Result<(), UfsError> {
        self.ensure_writable()?;
        self.validate_alloc_size(size)?;
        if addr == 0 || addr >= self.fs.size {
            return Err(UfsError::Corrupt("bad free address"));
        }
        let cg = (addr / u64::from(self.fs.fpg)) as u32;
        let fragno = (addr % u64::from(self.fs.fpg)) as u32;
        let mut group = self.load_cg(cg)?;
        let frags = size / self.fs.fsize;
        if fragno % self.fs.frag + frags > self.fs.frag {
            return Err(UfsError::Corrupt("free crosses block boundary"));
        }
        let map = group.blksfree(&self.fs)?;
        if size == self.fs.bsize {
            if !ffs_bitmap::is_free_block(self.fs.fragshift, map, (fragno / self.fs.frag) as usize)
            {
                return Err(UfsError::Corrupt("double free of block"));
            }
            let map = group.blksfree_mut(&self.fs)?;
            ffs_bitmap::set_block(self.fs.fragshift, map, (fragno / self.fs.frag) as usize)?;
        } else {
            for i in 0..frags {
                if group.block_free(&self.fs, fragno + i)? {
                    return Err(UfsError::Corrupt("double free"));
                }
            }
            for i in 0..frags {
                group.set_block_free(&self.fs, fragno + i)?;
            }
        }
        group.recompute_blocks(&self.fs)?;
        self.update_group(cg, &mut group)
    }

    /// Allocate an inode in the parent directory's group policy.
    pub fn alloc_inode(&mut self, parent: Ino, mode: u16) -> Result<Ino, UfsError> {
        self.ensure_writable()?;
        if self.fs.cstotal.nifree <= 0 {
            return Err(UfsError::NoSpace);
        }
        let is_dir = mode & 0o170000 == 0o040000;
        let mut ipref = if is_dir {
            self.dirpref(parent)
        } else if parent < self.fs.ncg.saturating_mul(self.fs.ipg) {
            parent
        } else {
            0
        };
        if ipref >= self.fs.ncg.saturating_mul(self.fs.ipg) {
            ipref = 0;
        }
        let cg = ino_to_cg(&self.fs, ipref);
        let mut allocator =
            |ufs: &mut Self, cg: u32, pref: u64| ufs.nodealloccg(cg, pref as u32, mode);
        let ino = self
            .hash_alloc(cg, u64::from(ipref), &mut allocator)?
            .ok_or(UfsError::NoSpace)? as Ino;
        if let Err(error) = self.write_new_inode(ino, mode) {
            let _ = self.free_inode(ino, mode);
            return Err(error);
        }
        Ok(ino)
    }

    /// Free an inode and its bitmap bit.
    pub fn free_inode(&mut self, ino: Ino, mode: u16) -> Result<(), UfsError> {
        self.ensure_writable()?;
        if ino >= self.fs.ncg.saturating_mul(self.fs.ipg) {
            return Err(UfsError::Corrupt("inode out of range"));
        }
        let cg = ino / self.fs.ipg;
        let index = ino % self.fs.ipg;
        let mut group = self.load_cg(cg)?;
        if !group.inode_used(&self.fs, index)? {
            return Err(UfsError::Corrupt("double free of inode"));
        }
        group.clear_inode_used(&self.fs, index)?;
        group.cs.nifree = group.cs.nifree.saturating_add(1);
        if mode & 0o170000 == 0o040000 {
            group.cs.ndir = group.cs.ndir.saturating_sub(1);
        }
        if index < group.irotor {
            group.irotor = index;
        }
        self.update_group(cg, &mut group)?;
        self.inodes.retain(|entry| entry.ino != ino);
        Ok(())
    }

    /// Preference for the next logical block (NetBSD `ffs_blkpref_ufs1/ufs2`).
    pub fn blkpref(&mut self, owner: Ino, lbn: u64, indx: usize, prev: &[u64]) -> u64 {
        let fs = &self.fs;
        let frag = u64::from(fs.frag);
        let previous = if indx == 0 {
            None
        } else {
            prev.get(indx - 1).copied()
        };
        if previous.is_none() || previous == Some(0) {
            if lbn < 12 + u64::from(fs.nindir) {
                let cg = ino_to_cg(fs, owner);
                return fs.cgbase(cg).unwrap_or(0) + frag;
            }
            let startcg = (ino_to_cg(fs, owner) as u64 + lbn / u64::from(fs.maxbpg()))
                % u64::from(fs.ncg.max(1));
            let avg = fs.cstotal.nbfree.max(0) as u64 / u64::from(fs.ncg.max(1));
            for pass in 0..2 {
                let (begin, end) = if pass == 0 {
                    (startcg, u64::from(fs.ncg))
                } else {
                    (0, startcg)
                };
                for cg in begin..end {
                    if self
                        .cs
                        .get(cg as usize)
                        .is_some_and(|cs| cs.nbfree.max(0) as u64 >= avg)
                    {
                        return fs.cgbase(cg as u32).unwrap_or(0) + frag;
                    }
                }
            }
            return 0;
        }
        previous.unwrap_or(0).saturating_add(frag)
    }
    /// Recompute every cylinder group's counts and summaries from the bitmaps and inode maps.
    pub fn verify_allocation(&mut self) -> Result<(), String> {
        let mut total = Csum::default();
        for cg in 0..self.fs.ncg {
            let mut group = self
                .load_cg(cg)
                .map_err(|error| format!("cg {cg}: load: {error:?}"))?;
            group
                .recompute_blocks(&self.fs)
                .map_err(|error| format!("cg {cg}: block counts: {error:?}"))?;
            let (nifree, ndir) = self
                .count_inode_state(cg)
                .map_err(|error| format!("cg {cg}: inode counts: {error:?}"))?;
            group.cs.nifree = nifree;
            group.cs.ndir = ndir;
            let expected = group.cs;
            let expected_total = expected.widen();
            let recorded = self
                .cs
                .get(cg as usize)
                .copied()
                .ok_or_else(|| format!("cg {cg}: missing summary"))?;
            if expected != recorded {
                return Err(format!(
                    "cg {cg}: in-memory summary {:?} != bitmap {:?}",
                    recorded, expected
                ));
            }
            total.ndir += expected_total.ndir;
            total.nbfree += expected_total.nbfree;
            total.nifree += expected_total.nifree;
            total.nffree += expected_total.nffree;
        }
        if total != self.fs.cstotal {
            return Err(format!(
                "global summary {:?} != per-group sum {:?}",
                self.fs.cstotal, total
            ));
        }
        Ok(())
    }

    /// Flush summaries, dirty cylinder groups and a clean superblock.
    pub fn sync(&mut self) -> Result<(), UfsError> {
        if self.read_only {
            return Ok(());
        }
        let dirty = core::mem::take(&mut self.dirty_cgs);
        for cg in dirty {
            let mut group = self.load_cg(cg)?;
            group.write(&mut self.disk, &self.fs)?;
        }
        ffs_vfsops::write_summaries(&mut self.disk, &self.fs, &self.cs)?;
        self.fs.cstotal = csum_total(&self.cs);
        self.fs.write_cstotal_raw();
        self.fs.set_time_raw(self.fs.time);
        self.fs.set_clean_raw(crate::fs::FS_ISCLEAN);
        self.fs.clear_fmod_raw();
        ffs_vfsops::write_superblock(&mut self.disk, &self.fs)?;
        self.disk.flush().map_err(|_| UfsError::Io)
    }

    /// Sync and return the block device.
    pub fn unmount(mut self) -> Result<D, UfsError> {
        self.sync()?;
        Ok(self.disk)
    }

    // --- allocator internals -------------------------------------------------

    fn validate_alloc_size(&self, size: u32) -> Result<(), UfsError> {
        if size == 0 || size > self.fs.bsize || !size.is_multiple_of(self.fs.fsize) {
            return Err(UfsError::Invalid);
        }
        if size / self.fs.fsize > self.fs.frag {
            return Err(UfsError::Invalid);
        }
        Ok(())
    }

    fn has_room(&self, size: u32) -> Result<bool, UfsError> {
        if size == self.fs.bsize && self.fs.cstotal.nbfree == 0 {
            return Ok(false);
        }
        let free = (self.fs.cstotal.nbfree.max(0) as u64)
            .checked_mul(u64::from(self.fs.frag))
            .and_then(|v| v.checked_add(self.fs.cstotal.nffree.max(0) as u64))
            .ok_or(UfsError::Corrupt("free space overflow"))?;
        let reserve = self
            .fs
            .dsize
            .checked_mul(self.fs.minfree.max(0) as u64)
            .ok_or(UfsError::Corrupt("reserve overflow"))?
            / 100;
        Ok(free > reserve)
    }

    fn hash_alloc<F>(
        &mut self,
        start_cg: u32,
        pref: u64,
        allocator: &mut F,
    ) -> Result<Option<u64>, UfsError>
    where
        F: FnMut(&mut Self, u32, u64) -> Result<Option<u64>, UfsError>,
    {
        if start_cg >= self.fs.ncg || self.fs.ncg == 0 {
            return Ok(None);
        }
        let icg = start_cg;
        if let Some(value) = allocator(self, start_cg, pref)? {
            return Ok(Some(value));
        }
        let mut cg = start_cg;
        let mut step = 1u32;
        while step < self.fs.ncg {
            cg = (cg + step) % self.fs.ncg;
            if let Some(value) = allocator(self, cg, 0)? {
                return Ok(Some(value));
            }
            step = step.saturating_mul(2);
        }
        cg = (icg + 2) % self.fs.ncg;
        for _ in 2..self.fs.ncg {
            if let Some(value) = allocator(self, cg, 0)? {
                return Ok(Some(value));
            }
            cg = (cg + 1) % self.fs.ncg;
        }
        Ok(None)
    }

    fn alloccg(&mut self, cg: u32, pref: u64, size: u32) -> Result<Option<u64>, UfsError> {
        let mut group = self.load_cg(cg)?;
        if size == self.fs.bsize && group.cs.nbfree == 0 {
            return Ok(None);
        }
        if size == self.fs.bsize {
            let bno = self.alloccgblk(&mut group, pref)?;
            let address = self.fs.cgbase(cg)? + u64::from(bno);
            self.update_group(cg, &mut group)?;
            return Ok(Some(address));
        }
        let frags = size / self.fs.fsize;
        let mut allocsiz = frags;
        while allocsiz < self.fs.frag {
            if group.frsum.get(allocsiz as usize).copied().unwrap_or(0) != 0 {
                break;
            }
            allocsiz += 1;
        }
        if allocsiz == self.fs.frag {
            if group.cs.nbfree == 0 {
                return Ok(None);
            }
            let bno = self.alloccgblk(&mut group, pref)?;
            for extra in frags..self.fs.frag {
                group.set_block_free(&self.fs, bno + extra)?;
            }
            group.recompute_blocks(&self.fs)?;
            let address = self.fs.cgbase(cg)? + u64::from(bno);
            self.update_group(cg, &mut group)?;
            return Ok(Some(address));
        }
        let start = if pref != 0 {
            (pref % u64::from(self.fs.fpg)) as u32
        } else {
            0
        };
        let map = group.blksfree(&self.fs)?;
        let bno = match ffs_bitmap::mapsearch(
            self.fs.frag,
            self.fs.fpg,
            start,
            group.frotor,
            map,
            allocsiz,
        ) {
            Ok(value) => value,
            Err(UfsError::NoSpace) => return Ok(None),
            Err(error) => return Err(error),
        };
        for i in 0..frags {
            group.clear_block_free(&self.fs, bno + i)?;
        }
        group.frotor = bno;
        group.recompute_blocks(&self.fs)?;
        let address = self.fs.cgbase(cg)? + u64::from(bno);
        self.update_group(cg, &mut group)?;
        Ok(Some(address))
    }

    fn alloccgblk(&mut self, group: &mut CylinderGroup, pref: u64) -> Result<u32, UfsError> {
        let mut bpref = if pref == 0 || (pref / u64::from(self.fs.fpg)) as u32 != group.cgx {
            group.rotor
        } else {
            let bno = (pref % u64::from(self.fs.fpg)) as u32;
            let block = bno - bno % self.fs.frag;
            if ffs_bitmap::is_block(
                self.fs.fragshift,
                group.blksfree(&self.fs)?,
                (block / self.fs.frag) as usize,
            ) {
                group.blocks_clear(&self.fs, block / self.fs.frag)?;
                group.recompute_blocks(&self.fs)?;
                return Ok(block);
            }
            block
        };
        let map = group.blksfree(&self.fs)?;
        bpref = ffs_bitmap::mapsearch(
            self.fs.frag,
            self.fs.fpg,
            bpref,
            group.rotor,
            map,
            self.fs.frag,
        )?;
        group.rotor = bpref;
        group.blocks_clear(&self.fs, bpref / self.fs.frag)?;
        group.recompute_blocks(&self.fs)?;
        Ok(bpref)
    }

    fn fragextend(
        &mut self,
        cg: u32,
        old: u64,
        old_size: u32,
        new_size: u32,
    ) -> Result<Option<u64>, UfsError> {
        let frags_old = old_size / self.fs.fsize;
        let frags_new = new_size / self.fs.fsize;
        let bbase = (old % u64::from(self.fs.frag)) as u32;
        if bbase + frags_new > self.fs.frag {
            return Ok(None);
        }
        let mut group = self.load_cg(cg)?;
        let extra = (new_size - old_size) / self.fs.fsize;
        if group.cs.nffree < extra as i32 {
            return Ok(None);
        }
        let fragno = (old % u64::from(self.fs.fpg)) as u32;
        for i in frags_old..frags_new {
            if !group.block_free(&self.fs, fragno + i)? {
                return Ok(None);
            }
        }
        for i in frags_old..frags_new {
            group.clear_block_free(&self.fs, fragno + i)?;
        }
        group.recompute_blocks(&self.fs)?;
        self.update_group(cg, &mut group)?;
        Ok(Some(old))
    }

    fn realloc_request(&mut self, new_size: u32) -> u32 {
        let nffree = self.fs.cstotal.nffree.max(0) as u64;
        let dsize = self.fs.dsize;
        let minfree = self.fs.minfree.max(0) as u64;
        match self.fs.optim {
            FS_OPTSPACE => {
                if self.fs.minfree < 5 || nffree > dsize * minfree / 200 {
                    new_size
                } else {
                    self.fs.bsize
                }
            }
            FS_OPTTIME => {
                if nffree < dsize * minfree.saturating_sub(2) / 100 {
                    self.fs.bsize
                } else {
                    new_size
                }
            }
            _ => new_size,
        }
    }

    fn dirpref(&mut self, parent: Ino) -> u32 {
        let fs = &self.fs;
        if fs.ncg == 0 || fs.ipg == 0 {
            return 0;
        }
        let avgifree = fs.cstotal.nifree / i64::from(fs.ncg);
        let avgbfree = fs.cstotal.nbfree / i64::from(fs.ncg);
        let avgndir = fs.cstotal.ndir / i64::from(fs.ncg);
        let prefcg = ino_to_cg(fs, parent);
        if parent == UFS_ROOTINO {
            let mut mincg = prefcg;
            let mut minndir = fs.ipg;
            for cg in prefcg..fs.ncg {
                if let Some(cs) = self.cs.get(cg as usize)
                    && i64::from(cs.ndir) < i64::from(minndir)
                    && i64::from(cs.nifree) >= avgifree
                    && i64::from(cs.nbfree) >= avgbfree
                {
                    mincg = cg;
                    minndir = cs.ndir as u32;
                }
            }
            for cg in 0..prefcg {
                if let Some(cs) = self.cs.get(cg as usize)
                    && i64::from(cs.ndir) < i64::from(minndir)
                    && i64::from(cs.nifree) >= avgifree
                    && i64::from(cs.nbfree) >= avgbfree
                {
                    mincg = cg;
                    minndir = cs.ndir as u32;
                }
            }
            return mincg * fs.ipg;
        }
        let maxndir = avgndir.max(0) as u32 + fs.ipg / 16;
        let maxndir = maxndir.min(fs.ipg);
        let minifree = (avgifree - avgifree / 4).max(1);
        let minbfree = (avgbfree - avgbfree / 4).max(1);
        for cg in prefcg..fs.ncg {
            if self.cg_has_dir_room(cg, maxndir, minifree, minbfree) {
                return cg * fs.ipg;
            }
        }
        for cg in 0..prefcg {
            if self.cg_has_dir_room(cg, maxndir, minifree, minbfree) {
                return cg * fs.ipg;
            }
        }
        prefcg * fs.ipg
    }

    fn cg_has_dir_room(&self, cg: u32, maxndir: u32, minifree: i64, minbfree: i64) -> bool {
        self.cs.get(cg as usize).is_some_and(|cs| {
            cs.ndir as u32 <= maxndir
                && i64::from(cs.nifree) >= minifree
                && i64::from(cs.nbfree) >= minbfree
        })
    }

    fn nodealloccg(&mut self, cg: u32, ipref: u32, mode: u16) -> Result<Option<u64>, UfsError> {
        let mut group = self.load_cg(cg)?;
        if group.cs.nifree == 0 {
            return Ok(None);
        }
        let max_inodes = if self.fs.ufs2 && group.niblk != 0 {
            group.niblk.min(self.fs.ipg)
        } else {
            self.fs.ipg
        };
        let mut limit = if self.fs.ufs2 {
            group.initediblk.min(max_inodes)
        } else {
            max_inodes
        };
        let ipref = ipref % self.fs.ipg;
        let index = loop {
            if self.fs.ufs2 && (ipref >= limit || limit == 0) {
                while limit <= ipref && limit < max_inodes {
                    self.init_inode_block(cg, &mut group, limit)?;
                    limit = group.initediblk.min(max_inodes);
                }
            }
            let mut found = None;
            if ipref < limit && !group.inode_used(&self.fs, ipref)? {
                found = Some(ipref);
            } else {
                let start = group.irotor.min(limit.saturating_sub(1));
                for pass in 0..2 {
                    let (begin, end) = if pass == 0 {
                        (start, limit)
                    } else {
                        (0, start)
                    };
                    for candidate in begin..end {
                        if !group.inode_used(&self.fs, candidate)? {
                            found = Some(candidate);
                            break;
                        }
                    }
                    if found.is_some() {
                        break;
                    }
                }
            }
            if let Some(candidate) = found {
                break candidate;
            }
            if self.fs.ufs2 && limit < max_inodes {
                self.init_inode_block(cg, &mut group, limit)?;
                limit = group.initediblk.min(max_inodes);
                continue;
            }
            return Ok(None);
        };
        group.set_inode_used(&self.fs, index)?;
        group.irotor = index;
        group.cs.nifree = group.cs.nifree.saturating_sub(1);
        if mode & 0o170000 == 0o040000 {
            group.cs.ndir = group.cs.ndir.saturating_add(1);
        }
        self.update_group(cg, &mut group)?;
        Ok(Some(
            u64::from(cg) * u64::from(self.fs.ipg) + u64::from(index),
        ))
    }

    fn init_inode_block(
        &mut self,
        cg: u32,
        group: &mut CylinderGroup,
        start: u32,
    ) -> Result<(), UfsError> {
        let ino = cg
            .checked_mul(self.fs.ipg)
            .and_then(|v| v.checked_add(start))
            .ok_or(UfsError::Corrupt("inode initialization address"))?;
        let (fsba, _) = self.fs.ino_to_fsba(ino)?;
        let mut block = vec![0u8; self.fs.bsize as usize];
        let inode_size = self.fs.inode_size();
        let inopb = self.fs.inopb as usize;
        for index in 0..inopb {
            let off = index * inode_size;
            let generation = ino.wrapping_add(index as u32).wrapping_mul(2_654_435_761) | 1;
            let gen_off = if self.fs.ufs2 { 80 } else { 108 };
            let dst = block
                .get_mut(off + gen_off..off + gen_off + 4)
                .ok_or(UfsError::Corrupt("inode generation offset"))?;
            dst.copy_from_slice(&generation.to_le_bytes());
        }
        self.write_block_at(fsba, &block)?;
        group.initediblk = group.initediblk.saturating_add(self.fs.inopb);
        Ok(())
    }

    fn write_new_inode(&mut self, ino: Ino, mode: u16) -> Result<(), UfsError> {
        let (fsba, slot) = self.fs.ino_to_fsba(ino)?;
        let mut block = vec![0u8; self.fs.bsize as usize];
        self.disk
            .read_at(self.fs.frag_offset(fsba)?, &mut block)
            .map_err(|_| UfsError::Io)?;
        let offset = slot
            .checked_mul(self.fs.inode_size())
            .ok_or(UfsError::Corrupt("inode slot"))?;
        let len = self.fs.inode_size();
        let raw = block
            .get_mut(offset..offset + len)
            .ok_or(UfsError::Corrupt("inode slot"))?;
        raw.fill(0);
        raw[0..2].copy_from_slice(&mode.to_le_bytes());
        let generation = (ino.wrapping_mul(2_654_435_761) | 1).to_le_bytes();
        let gen_off = if self.fs.ufs2 { 80 } else { 108 };
        raw[gen_off..gen_off + 4].copy_from_slice(&generation);
        self.write_block_at(fsba, &block)?;
        self.inodes.retain(|entry| entry.ino != ino);
        Ok(())
    }

    fn write_block_at(&mut self, frag: u64, data: &[u8]) -> Result<(), UfsError> {
        let offset = self.fs.frag_offset(frag)?;
        if data.len() != self.fs.bsize as usize {
            return Err(UfsError::Invalid);
        }
        self.disk.write_at(offset, data).map_err(|_| UfsError::Io)?;
        self.blocks.retain(|entry| entry.frag != frag);
        Ok(())
    }

    fn count_inode_state(&mut self, cg: u32) -> Result<(i32, i32), UfsError> {
        let group = self.load_cg(cg)?;
        let mut used = 0i32;
        let mut dirs = 0i32;
        for index in 0..self.fs.ipg {
            if group.inode_used(&self.fs, index)? {
                used += 1;
                let ino = cg * self.fs.ipg + index;
                if let Ok(inode) = self.load_inode(ino)
                    && inode.file_type()? == FileType::Directory
                {
                    dirs += 1;
                }
            }
        }
        Ok((self.fs.ipg as i32 - used, dirs))
    }
}

impl CylinderGroup {
    fn blocks_clear(&mut self, fs: &Superblock, block: u32) -> Result<(), UfsError> {
        let map = self.blksfree_mut(fs)?;
        ffs_bitmap::clear_block(fs.fragshift, map, block as usize)
    }
}

impl Superblock {
    pub(crate) fn maxbpg(&self) -> u32 {
        self.maxbpg.max(1) as u32
    }
}
