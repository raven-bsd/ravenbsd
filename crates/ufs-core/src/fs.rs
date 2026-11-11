// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ffs/fs.h (revision 36eddebcc36cb0775aa2962005b9c05419a6ff01).
// The original notice follows.
//
// Copyright (c) 1982, 1986, 1993
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

//! UFS superblock constants, decoding and cylinder-group arithmetic.

use alloc_crate::vec::Vec;

use crate::dinode::{UFS1_MAXSYMLINKLEN, UFS2_MAXSYMLINKLEN};
use crate::error::UfsError;
use crate::types::Ino;

pub(crate) const SBLOCK_FLOPPY: u64 = 0;
pub(crate) const SBLOCK_UFS1: u64 = 8192;
pub(crate) const SBLOCK_UFS2: u64 = 65536;
pub(crate) const SBLOCK_PIGGY: u64 = 262_144;
pub(crate) const SBLOCKSIZE: u64 = 8192;
pub(crate) const MINBSIZE: u32 = 4096;
pub(crate) const MAXBSIZE: u32 = 65536;
pub(crate) const MAXFRAG: u32 = 8;
pub(crate) const UFS_ROOTINO: Ino = 2;
pub(crate) const UFS_DIRBLKSIZ: usize = 512;
pub(crate) const FFS_MAXNAMLEN: usize = 255;

pub(crate) const FS_UFS1_MAGIC: u32 = 0x0001_1954;
pub(crate) const FS_UFS2_MAGIC: u32 = 0x1954_0119;
pub(crate) const FS_UFS2EA_MAGIC: u32 = 0x1901_2038;
pub(crate) const FS_UFS1_MAGIC_SWAPPED: u32 = 0x5419_0100;
pub(crate) const FS_UFS2_MAGIC_SWAPPED: u32 = 0x1901_5419;
pub(crate) const FS_UFS2EA_MAGIC_SWAPPED: u32 = 0x3820_0119;

pub(crate) const FS_44INODEFMT: i32 = 2;
pub(crate) const FS_ISCLEAN: u8 = 0x01;
pub(crate) const FS_WASCLEAN: u8 = 0x02;
pub(crate) const FS_FLAGS_UPDATED: u8 = 0x80;
pub(crate) const FS_SWAPPED: u32 = 0x8000_0000;

const O_SBLKNO: usize = 8;
const O_CBLKNO: usize = 12;
const O_IBLKNO: usize = 16;
const O_DBLKNO: usize = 20;
const O_OLD_CGOFFSET: usize = 24;
const O_OLD_CGMASK: usize = 28;
const O_OLD_SIZE: usize = 36;
const O_OLD_DSIZE: usize = 40;
const O_NCG: usize = 44;
const O_BSIZE: usize = 48;
const O_FSIZE: usize = 52;
const O_FRAG: usize = 56;
const O_MINFREE: usize = 60;
const O_BMASK: usize = 72;
const O_FMASK: usize = 76;
const O_BSHIFT: usize = 80;
const O_FSHIFT: usize = 84;
const O_MAXBPG: usize = 92;
const O_FRAGSHIFT: usize = 96;
const O_SBSIZE: usize = 104;
const O_NINDIR: usize = 116;
const O_INOPB: usize = 120;
const O_OPTIM: usize = 128;
const O_OLD_INODEFMT: usize = 1324;
const O_OLD_CSADDR: usize = 152;
const O_CSSIZE: usize = 156;
const O_CGSIZE: usize = 160;
const O_IPG: usize = 184;
const O_FPG: usize = 188;
const O_OLD_CSTOTAL: usize = 192;
const O_CSTOTAL: usize = 1008;
const O_SBLOCKLOC: usize = 1000;
const O_SIZE: usize = 1080;
const O_DSIZE: usize = 1088;
const O_CSADDR: usize = 1096;
const O_TIME: usize = 1072;
const O_AVGFILESIZE: usize = 1196;
const O_AVGFPDIR: usize = 1200;
const O_FMOD: usize = 208;
const O_CLEAN: usize = 209;
const O_OLD_FLAGS: usize = 211;
const O_FLAGS: usize = 1312;
const O_CONTIGSUMSIZE: usize = 1316;
const O_MAXSYMLINKLEN: usize = 1320;
const O_MAGIC: usize = 1372;

/// Aggregate cylinder-group summary counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Csum {
    pub(crate) ndir: i64,
    pub(crate) nbfree: i64,
    pub(crate) nifree: i64,
    pub(crate) nffree: i64,
}

/// On-disk per-cylinder-group `struct csum` (32-bit counters).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Csum32 {
    pub(crate) ndir: i32,
    pub(crate) nbfree: i32,
    pub(crate) nifree: i32,
    pub(crate) nffree: i32,
}

impl Csum32 {
    pub(crate) fn widen(self) -> Csum {
        Csum {
            ndir: i64::from(self.ndir),
            nbfree: i64::from(self.nbfree),
            nifree: i64::from(self.nifree),
            nffree: i64::from(self.nffree),
        }
    }

    pub(crate) fn from_le_bytes(bytes: &[u8]) -> Result<Self, UfsError> {
        if bytes.len() < 16 {
            return Err(UfsError::Corrupt("short csum"));
        }
        let mut ndir = [0u8; 4];
        let mut nbfree = [0u8; 4];
        let mut nifree = [0u8; 4];
        let mut nffree = [0u8; 4];
        ndir.copy_from_slice(&bytes[0..4]);
        nbfree.copy_from_slice(&bytes[4..8]);
        nifree.copy_from_slice(&bytes[8..12]);
        nffree.copy_from_slice(&bytes[12..16]);
        Ok(Self {
            ndir: i32::from_le_bytes(ndir),
            nbfree: i32::from_le_bytes(nbfree),
            nifree: i32::from_le_bytes(nifree),
            nffree: i32::from_le_bytes(nffree),
        })
    }

    pub(crate) fn to_le_bytes(self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[0..4].copy_from_slice(&self.ndir.to_le_bytes());
        out[4..8].copy_from_slice(&self.nbfree.to_le_bytes());
        out[8..12].copy_from_slice(&self.nifree.to_le_bytes());
        out[12..16].copy_from_slice(&self.nffree.to_le_bytes());
        out
    }
}

/// A decoded, structurally checked UFS superblock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Superblock {
    pub(crate) ufs2: bool,
    pub(crate) sblockloc: u64,
    pub(crate) bsize: u32,
    pub(crate) fsize: u32,
    pub(crate) frag: u32,
    pub(crate) bshift: u32,
    pub(crate) fshift: u32,
    pub(crate) fragshift: u32,
    pub(crate) bmask: i32,
    pub(crate) fmask: i32,
    pub(crate) maxbpg: i32,
    pub(crate) ncg: u32,
    pub(crate) ipg: u32,
    pub(crate) fpg: u32,
    pub(crate) inopb: u32,
    pub(crate) nindir: u32,
    pub(crate) iblkno: i32,
    pub(crate) dblkno: i32,
    pub(crate) sblkno: i32,
    pub(crate) cblkno: i32,
    pub(crate) old_cgoffset: i32,
    pub(crate) old_cgmask: i32,
    pub(crate) size: u64,
    pub(crate) dsize: u64,
    pub(crate) csaddr: u32,
    pub(crate) cssize: u32,
    pub(crate) cgsize: u32,
    pub(crate) sbsize: u32,
    pub(crate) clean: u8,
    pub(crate) cstotal: Csum,
    pub(crate) maxsymlinklen: i32,
    pub(crate) time: i64,
    pub(crate) minfree: i32,
    pub(crate) optim: i32,
    pub(crate) avgfilesize: u32,
    pub(crate) avgfpdir: u32,
    pub(crate) contigsumsize: i32,
    pub(crate) old_flags: u8,
    pub(crate) raw: Vec<u8>,
}

impl Superblock {
    /// Decode a superblock read from `probe` bytes into the device.
    pub(crate) fn parse(buf: &[u8], probe: u64) -> Result<Self, UfsError> {
        let magic = read_u32(buf, O_MAGIC)?;
        let ufs2 = match magic {
            FS_UFS1_MAGIC => false,
            FS_UFS2_MAGIC => true,
            FS_UFS1_MAGIC_SWAPPED | FS_UFS2_MAGIC_SWAPPED => {
                return Err(UfsError::Unsupported("big-endian UFS"));
            }
            FS_UFS2EA_MAGIC | FS_UFS2EA_MAGIC_SWAPPED => {
                return Err(UfsError::Unsupported("UFS2 extended attributes"));
            }
            _ => return Err(UfsError::BadSuperblock("unknown magic")),
        };

        let fsize = read_u32(buf, O_FSIZE)?;
        let frag = read_u32(buf, O_FRAG)?;
        let old_flags = read_u8(buf, O_OLD_FLAGS)?;
        let old_inodefmt = read_i32(buf, O_OLD_INODEFMT)?;
        let old = !ufs2 && (old_flags & FS_FLAGS_UPDATED) == 0;

        let mut sblockloc = read_u64(buf, O_SBLOCKLOC)?;
        let mut size = read_u64(buf, O_SIZE)?;
        let mut dsize = read_u64(buf, O_DSIZE)?;
        let mut csaddr = read_u32(buf, O_CSADDR)?;
        let mut flags = read_u32(buf, O_FLAGS)?;
        let mut cstotal = read_csum_total(buf, O_CSTOTAL)?;

        if old {
            if old_inodefmt < FS_44INODEFMT {
                return Err(UfsError::Unsupported("UFS1 4.2BSD inode format"));
            }
            sblockloc = probe;
            size = read_u32(buf, O_OLD_SIZE)? as u64;
            dsize = read_u32(buf, O_OLD_DSIZE)? as u64;
            csaddr = read_u32(buf, O_OLD_CSADDR)?;
            flags = old_flags as u32;
            cstotal = read_csum_old(buf, O_OLD_CSTOTAL)?;
        }

        if sblockloc != probe {
            return Err(UfsError::BadSuperblock("superblock location mismatch"));
        }
        if flags & FS_SWAPPED != 0 {
            return Err(UfsError::Unsupported("big-endian UFS"));
        }

        let maxsymlinklen = read_i32(buf, O_MAXSYMLINKLEN)?;
        let max_allowed = if ufs2 {
            UFS2_MAXSYMLINKLEN as i32
        } else {
            UFS1_MAXSYMLINKLEN as i32
        };
        if maxsymlinklen < 0 || maxsymlinklen > max_allowed {
            return Err(UfsError::BadSuperblock("bad maxsymlinklen"));
        }

        let sb = Self {
            ufs2,
            sblockloc,
            bsize: read_u32(buf, O_BSIZE)?,
            fsize,
            frag,
            bshift: read_u32(buf, O_BSHIFT)?,
            fshift: read_u32(buf, O_FSHIFT)?,
            fragshift: read_u32(buf, O_FRAGSHIFT)?,
            bmask: read_i32(buf, O_BMASK)?,
            fmask: read_i32(buf, O_FMASK)?,
            maxbpg: read_i32(buf, O_MAXBPG)?,
            ncg: read_u32(buf, O_NCG)?,
            ipg: read_u32(buf, O_IPG)?,
            fpg: read_u32(buf, O_FPG)?,
            inopb: read_u32(buf, O_INOPB)?,
            nindir: read_u32(buf, O_NINDIR)?,
            iblkno: read_i32(buf, O_IBLKNO)?,
            dblkno: read_i32(buf, O_DBLKNO)?,
            sblkno: read_i32(buf, O_SBLKNO)?,
            cblkno: read_i32(buf, O_CBLKNO)?,
            old_cgoffset: read_i32(buf, O_OLD_CGOFFSET)?,
            old_cgmask: read_i32(buf, O_OLD_CGMASK)?,
            size,
            dsize,
            csaddr,
            cssize: read_u32(buf, O_CSSIZE)?,
            cgsize: read_u32(buf, O_CGSIZE)?,
            sbsize: read_u32(buf, O_SBSIZE)?,
            clean: read_u8(buf, O_CLEAN)?,
            cstotal,
            maxsymlinklen,
            time: read_i64(buf, O_TIME)?,
            minfree: read_i32(buf, O_MINFREE)?,
            optim: read_i32(buf, O_OPTIM)?,
            avgfilesize: read_u32(buf, O_AVGFILESIZE)?,
            avgfpdir: read_u32(buf, O_AVGFPDIR)?,
            contigsumsize: read_i32(buf, O_CONTIGSUMSIZE)?,
            old_flags,
            raw: buf.to_vec(),
        };
        Ok(sb)
    }

    /// Size of one inode in bytes.
    pub(crate) fn inode_size(&self) -> usize {
        if self.ufs2 { 256 } else { 128 }
    }

    /// Size of an on-disk block pointer in bytes.
    pub(crate) fn ptr_size(&self) -> usize {
        if self.ufs2 { 8 } else { 4 }
    }

    /// Number of pointers per indirect block.
    pub(crate) fn nindir(&self) -> u64 {
        u64::from(self.nindir)
    }

    /// Whether the filesystem is marked clean (`FS_ISCLEAN` or `FS_WASCLEAN`).
    pub(crate) fn is_clean(&self) -> bool {
        self.clean & (FS_ISCLEAN | FS_WASCLEAN) != 0
    }

    /// Base fragment of cylinder group `cg`.
    pub(crate) fn cgbase(&self, cg: u32) -> Result<u64, UfsError> {
        (u64::from(self.fpg))
            .checked_mul(u64::from(cg))
            .ok_or(UfsError::BadSuperblock("cylinder group overflow"))
    }

    /// First fragment of cylinder group `cg` (the UFS1 rotation offset included).
    pub(crate) fn cgstart(&self, cg: u32) -> Result<u64, UfsError> {
        let base = self.cgbase(cg)?;
        if self.ufs2 {
            return Ok(base);
        }
        let mask = self.old_cgmask as u32;
        let term = u64::from(cg & !mask);
        let offset = (self.old_cgoffset as i64)
            .checked_mul(term as i64)
            .ok_or(UfsError::BadSuperblock("cylinder offset overflow"))?;
        if offset < 0 {
            return Err(UfsError::BadSuperblock("negative cylinder offset"));
        }
        base.checked_add(offset as u64)
            .ok_or(UfsError::BadSuperblock("cylinder group overflow"))
    }

    /// First inode-bearing fragment of cylinder group `cg`.
    pub(crate) fn cgimin(&self, cg: u32) -> Result<u64, UfsError> {
        self.cgstart(cg)?
            .checked_add(self.iblkno as u64)
            .ok_or(UfsError::BadSuperblock("inode block overflow"))
    }

    /// Fragment address of the block holding `ino`, and the inode slot in that block.
    pub(crate) fn ino_to_fsba(&self, ino: Ino) -> Result<(u64, usize), UfsError> {
        let cg = u64::from(ino) / u64::from(self.ipg);
        if cg >= u64::from(self.ncg) {
            return Err(UfsError::Invalid);
        }
        let inum = u64::from(ino) % u64::from(self.ipg);
        let blk = inum / u64::from(self.inopb);
        let fsbo = (inum % u64::from(self.inopb)) as usize;
        let frags = blk
            .checked_mul(u64::from(self.frag))
            .ok_or(UfsError::BadSuperblock("inode block overflow"))?;
        let fsba = self
            .cgimin(cg as u32)?
            .checked_add(frags)
            .ok_or(UfsError::BadSuperblock("inode address overflow"))?;
        Ok((fsba, fsbo))
    }

    /// Byte offset of a fragment address.
    pub(crate) fn frag_offset(&self, frag: u64) -> Result<u64, UfsError> {
        frag.checked_mul(u64::from(self.fsize))
            .ok_or(UfsError::Corrupt("fragment address overflow"))
    }

    /// Update the in-memory clean flag and its superblock byte.
    pub(crate) fn set_clean_raw(&mut self, clean: u8) {
        self.clean = clean;
        if let Some(byte) = self.raw.get_mut(O_CLEAN) {
            *byte = clean;
        }
    }

    /// Update the in-memory timestamp and its superblock bytes.
    pub(crate) fn set_time_raw(&mut self, time: i64) {
        self.time = time;
        if let Some(bytes) = self.raw.get_mut(O_TIME..O_TIME + 8) {
            bytes.copy_from_slice(&time.to_le_bytes());
        }
    }

    /// Clear the on-disk `fs_fmod` byte; the in-memory value is not stored.
    pub(crate) fn clear_fmod_raw(&mut self) {
        if let Some(byte) = self.raw.get_mut(O_FMOD) {
            *byte = 0;
        }
    }

    /// Encode the aggregate summary counters into the raw superblock.
    pub(crate) fn write_cstotal_raw(&mut self) {
        if let Some(bytes) = self.raw.get_mut(O_CSTOTAL..O_CSTOTAL + 32) {
            bytes[0..8].copy_from_slice(&self.cstotal.ndir.to_le_bytes());
            bytes[8..16].copy_from_slice(&self.cstotal.nbfree.to_le_bytes());
            bytes[16..24].copy_from_slice(&self.cstotal.nifree.to_le_bytes());
            bytes[24..32].copy_from_slice(&self.cstotal.nffree.to_le_bytes());
        }
        if !self.ufs2
            && let Some(bytes) = self.raw.get_mut(O_OLD_CSTOTAL..O_OLD_CSTOTAL + 16)
        {
            bytes[0..4].copy_from_slice(&(self.cstotal.ndir as i32).to_le_bytes());
            bytes[4..8].copy_from_slice(&(self.cstotal.nbfree as i32).to_le_bytes());
            bytes[8..12].copy_from_slice(&(self.cstotal.nifree as i32).to_le_bytes());
            bytes[12..16].copy_from_slice(&(self.cstotal.nffree as i32).to_le_bytes());
        }
    }
}

fn read_exact(buf: &[u8], off: usize, len: usize) -> Result<&[u8], UfsError> {
    buf.get(
        off..off
            .checked_add(len)
            .ok_or(UfsError::BadSuperblock("offset overflow"))?,
    )
    .ok_or(UfsError::BadSuperblock("truncated superblock"))
}

fn read_u8(buf: &[u8], off: usize) -> Result<u8, UfsError> {
    let bytes = read_exact(buf, off, 1)?;
    Ok(bytes[0])
}

fn read_u32(buf: &[u8], off: usize) -> Result<u32, UfsError> {
    let bytes = read_exact(buf, off, 4)?;
    let mut value = [0u8; 4];
    value.copy_from_slice(bytes);
    Ok(u32::from_le_bytes(value))
}

fn read_i32(buf: &[u8], off: usize) -> Result<i32, UfsError> {
    let bytes = read_exact(buf, off, 4)?;
    let mut value = [0u8; 4];
    value.copy_from_slice(bytes);
    Ok(i32::from_le_bytes(value))
}

fn read_u64(buf: &[u8], off: usize) -> Result<u64, UfsError> {
    let bytes = read_exact(buf, off, 8)?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(value))
}

fn read_csum_old(buf: &[u8], off: usize) -> Result<Csum, UfsError> {
    Ok(Csum {
        ndir: i64::from(read_i32(buf, off)?),
        nbfree: i64::from(read_i32(buf, off + 4)?),
        nifree: i64::from(read_i32(buf, off + 8)?),
        nffree: i64::from(read_i32(buf, off + 12)?),
    })
}

fn read_csum_total(buf: &[u8], off: usize) -> Result<Csum, UfsError> {
    Ok(Csum {
        ndir: read_i64(buf, off)?,
        nbfree: read_i64(buf, off + 8)?,
        nifree: read_i64(buf, off + 16)?,
        nffree: read_i64(buf, off + 24)?,
    })
}

fn read_i64(buf: &[u8], off: usize) -> Result<i64, UfsError> {
    let bytes = read_exact(buf, off, 8)?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Ok(i64::from_le_bytes(value))
}

/// Set a little-endian `u32` in a test buffer.
#[cfg(test)]
pub(crate) fn set_u32(buf: &mut [u8], off: usize, value: u32) {
    buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

/// Set a little-endian `i32` in a test buffer.
#[cfg(test)]
pub(crate) fn set_i32(buf: &mut [u8], off: usize, value: i32) {
    buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

/// Set a little-endian `u64` in a test buffer.
#[cfg(test)]
pub(crate) fn set_u64(buf: &mut [u8], off: usize, value: u64) {
    buf[off..off + 8].copy_from_slice(&value.to_le_bytes());
}

/// Set a little-endian `i64` in a test buffer.
#[cfg(test)]
pub(crate) fn set_i64(buf: &mut [u8], off: usize, value: i64) {
    buf[off..off + 8].copy_from_slice(&value.to_le_bytes());
}

/// Set a byte in a test buffer.
#[cfg(test)]
pub(crate) fn set_u8(buf: &mut [u8], off: usize, value: u8) {
    buf[off] = value;
}

/// Build a structurally valid synthetic superblock for the validation unit tests.
#[cfg(test)]
pub(crate) fn test_superblock(ufs2: bool) -> [u8; SBLOCKSIZE as usize] {
    let mut buf = [0u8; SBLOCKSIZE as usize];
    set_u32(
        &mut buf,
        O_MAGIC,
        if ufs2 { FS_UFS2_MAGIC } else { FS_UFS1_MAGIC },
    );
    set_u64(&mut buf, O_SBLOCKLOC, SBLOCK_UFS1);
    set_u32(&mut buf, O_BSIZE, 4096);
    set_u32(&mut buf, O_FSIZE, 512);
    set_u32(&mut buf, O_FRAG, 8);
    set_u32(&mut buf, O_BSHIFT, 12);
    set_u32(&mut buf, O_FSHIFT, 9);
    set_u32(&mut buf, O_FRAGSHIFT, 3);
    set_i32(&mut buf, O_BMASK, !(4096 - 1));
    set_i32(&mut buf, O_FMASK, !(512 - 1));
    set_i32(&mut buf, O_MAXBPG, 16);
    set_u32(&mut buf, O_NCG, 1);
    set_u32(&mut buf, O_IPG, 32);
    set_u32(&mut buf, O_FPG, 32768);
    set_u32(&mut buf, O_INOPB, if ufs2 { 16 } else { 32 });
    set_u32(&mut buf, O_NINDIR, if ufs2 { 512 } else { 1024 });
    set_i32(&mut buf, O_IBLKNO, 56);
    set_i32(&mut buf, O_DBLKNO, 64);
    set_i32(&mut buf, O_SBLKNO, 32);
    set_i32(&mut buf, O_CBLKNO, 40);
    set_i32(&mut buf, O_OLD_CGOFFSET, 0);
    set_i32(&mut buf, O_OLD_CGMASK, -1);
    set_u64(&mut buf, O_SIZE, 32768);
    set_u64(&mut buf, O_DSIZE, 32000);
    set_u32(&mut buf, O_CSADDR, 400);
    set_u32(&mut buf, O_CSSIZE, 512);
    set_u32(&mut buf, O_CGSIZE, 4096);
    set_u32(&mut buf, O_SBSIZE, 1376);
    set_u8(&mut buf, O_CLEAN, FS_ISCLEAN);
    set_u8(&mut buf, O_OLD_FLAGS, FS_FLAGS_UPDATED);
    set_u32(&mut buf, O_FLAGS, 0);
    set_i32(&mut buf, O_OLD_INODEFMT, FS_44INODEFMT);
    set_i32(&mut buf, O_MAXSYMLINKLEN, if ufs2 { 120 } else { 60 });
    set_i64(&mut buf, O_CSTOTAL, 1);
    set_i64(&mut buf, O_CSTOTAL + 8, 100);
    set_i64(&mut buf, O_CSTOTAL + 16, 10);
    set_i64(&mut buf, O_CSTOTAL + 24, 0);
    set_i32(&mut buf, O_OLD_CSTOTAL, 1);
    set_i32(&mut buf, O_OLD_CSTOTAL + 4, 100);
    set_i32(&mut buf, O_OLD_CSTOTAL + 8, 10);
    set_i32(&mut buf, O_OLD_CSTOTAL + 12, 0);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICE: u64 = 16 * 1024 * 1024;

    #[test]
    fn parses_ufs1() {
        let buf = test_superblock(false);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse ufs1");
        assert!(!sb.ufs2);
        assert_eq!(sb.bsize, 4096);
        assert_eq!(sb.fsize, 512);
        assert_eq!(sb.frag, 8);
        assert_eq!(sb.inopb, 32);
        assert_eq!(sb.nindir, 1024);
        assert_eq!(sb.size, 32768);
        assert_eq!(sb.cstotal.nbfree, 100);
        assert!(sb.is_clean());
    }

    #[test]
    fn parses_ufs2() {
        let buf = test_superblock(true);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse ufs2");
        assert!(sb.ufs2);
        assert_eq!(sb.inopb, 16);
        assert_eq!(sb.nindir, 512);
        assert_eq!(sb.maxsymlinklen, 120);
    }

    #[test]
    fn swapped_magic_is_unsupported() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, O_MAGIC, FS_UFS1_MAGIC_SWAPPED);
        assert_eq!(
            Superblock::parse(&buf, SBLOCK_UFS1),
            Err(UfsError::Unsupported("big-endian UFS"))
        );
    }

    #[test]
    fn extended_attribute_magic_is_unsupported() {
        let mut buf = test_superblock(true);
        set_u32(&mut buf, O_MAGIC, FS_UFS2EA_MAGIC);
        assert_eq!(
            Superblock::parse(&buf, SBLOCK_UFS1),
            Err(UfsError::Unsupported("UFS2 extended attributes"))
        );
    }

    #[test]
    fn unknown_magic_is_bad_superblock() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, O_MAGIC, 0xdead_beef);
        assert_eq!(
            Superblock::parse(&buf, SBLOCK_UFS1),
            Err(UfsError::BadSuperblock("unknown magic"))
        );
    }

    #[test]
    fn location_mismatch_is_rejected() {
        let mut buf = test_superblock(false);
        set_u64(&mut buf, O_SBLOCKLOC, SBLOCK_UFS2);
        assert_eq!(
            Superblock::parse(&buf, SBLOCK_UFS1),
            Err(UfsError::BadSuperblock("superblock location mismatch"))
        );
    }

    #[test]
    fn bad_maxsymlinklen_is_rejected() {
        let mut buf = test_superblock(true);
        set_i32(&mut buf, O_MAXSYMLINKLEN, 121);
        assert_eq!(
            Superblock::parse(&buf, SBLOCK_UFS1),
            Err(UfsError::BadSuperblock("bad maxsymlinklen"))
        );
    }

    #[test]
    fn swapped_flag_is_unsupported() {
        let mut buf = test_superblock(false);
        set_u32(&mut buf, O_FLAGS, FS_SWAPPED);
        assert_eq!(
            Superblock::parse(&buf, SBLOCK_UFS1),
            Err(UfsError::Unsupported("big-endian UFS"))
        );
    }

    #[test]
    fn old_ufs1_inode_format_is_unsupported() {
        let mut buf = test_superblock(false);
        set_u8(&mut buf, O_OLD_FLAGS, 0);
        set_i32(&mut buf, O_OLD_INODEFMT, -1);
        set_u32(&mut buf, O_OLD_SIZE, 32768);
        set_u32(&mut buf, O_OLD_DSIZE, 32000);
        assert_eq!(
            Superblock::parse(&buf, SBLOCK_UFS1),
            Err(UfsError::Unsupported("UFS1 4.2BSD inode format"))
        );
    }

    #[test]
    fn truncated_superblock_is_rejected() {
        let buf = [0u8; 128];
        assert!(matches!(
            Superblock::parse(&buf, SBLOCK_UFS1),
            Err(UfsError::BadSuperblock(_))
        ));
    }

    #[test]
    fn cylinder_group_arithmetic() {
        let mut buf = test_superblock(true);
        set_u32(&mut buf, O_NCG, 2);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert_eq!(sb.cgstart(0).expect("cg0"), 0);
        assert_eq!(sb.cgstart(1).expect("cg1"), 32768);
        assert_eq!(sb.cgimin(0).expect("imin0"), 56);
        let (fsba, slot) = sb.ino_to_fsba(2).expect("root inode");
        assert_eq!(fsba, 56);
        assert_eq!(slot, 2);
        let (fsba, slot) = sb.ino_to_fsba(32 + 16 + 1).expect("second group");
        assert_eq!(fsba, 32768 + 56 + 8);
        assert_eq!(slot, 1);
    }

    #[test]
    fn fragment_offsets() {
        let buf = test_superblock(false);
        let sb = Superblock::parse(&buf, SBLOCK_UFS1).expect("parse");
        assert_eq!(sb.frag_offset(0).expect("zero"), 0);
        assert_eq!(sb.frag_offset(3).expect("three"), 3 * 512);
    }

    #[test]
    fn inode_and_pointer_sizes() {
        let ufs1 = Superblock::parse(&test_superblock(false), SBLOCK_UFS1).expect("ufs1");
        let ufs2 = Superblock::parse(&test_superblock(true), SBLOCK_UFS1).expect("ufs2");
        assert_eq!(ufs1.inode_size(), 128);
        assert_eq!(ufs2.inode_size(), 256);
        assert_eq!(ufs1.ptr_size(), 4);
        assert_eq!(ufs2.ptr_size(), 8);
        let _ = DEVICE;
    }
}
