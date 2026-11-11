// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ffs/ffs_balloc.c (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notice follows.
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

//! Logical-block allocation for writes: direct and indirect bmap growth.

use alloc_crate::vec;
use alloc_crate::vec::Vec;

use crate::blockio::BlockIo;
use crate::dinode::Inode;
use crate::error::UfsError;
use crate::fs::Superblock;
use crate::types::Ino;
use crate::ufs::Ufs;
use crate::ufs_bmap::{self, BmapPlan};

impl<D: BlockIo> Ufs<D> {
    /// Read a fragment-aligned extent into memory.
    pub(crate) fn read_frags(&mut self, addr: u64, size: u32) -> Result<Vec<u8>, UfsError> {
        self.validate_extent(addr, size)?;
        let offset = self.fs.frag_offset(addr)?;
        let mut data = vec![0u8; size as usize];
        self.read_disk_at(offset, &mut data)?;
        Ok(data)
    }

    /// Write a fragment-aligned extent and drop any cached block covering it.
    pub(crate) fn write_frags(&mut self, addr: u64, data: &[u8]) -> Result<(), UfsError> {
        if data.is_empty() || data.len() > self.fs.bsize as usize {
            return Err(UfsError::Invalid);
        }
        if !data.len().is_multiple_of(self.fs.fsize as usize) {
            return Err(UfsError::Invalid);
        }
        self.validate_extent(addr, data.len() as u32)?;
        let offset = self.fs.frag_offset(addr)?;
        self.disk.write_at(offset, data).map_err(|_| UfsError::Io)?;
        self.blocks.retain(|entry| entry.frag != addr);
        Ok(())
    }

    fn validate_extent(&self, addr: u64, size: u32) -> Result<(), UfsError> {
        if size == 0 || size > self.fs.bsize || !size.is_multiple_of(self.fs.fsize) {
            return Err(UfsError::Invalid);
        }
        if addr == 0 || addr >= self.fs.size {
            return Err(UfsError::Corrupt("extent outside filesystem"));
        }
        let end = addr
            .checked_add(u64::from(size / self.fs.fsize))
            .ok_or(UfsError::Corrupt("extent overflow"))?;
        if end > self.fs.size {
            return Err(UfsError::Corrupt("extent outside filesystem"));
        }
        Ok(())
    }

    /// Return the allocation size currently backing `lbn`, or zero for a hole.
    pub(crate) fn allocated_size(&mut self, ip: &Inode, lbn: u64) -> Result<u32, UfsError> {
        if self.bmap(ip, lbn)?.is_none() {
            return Ok(0);
        }
        let bsize = u64::from(self.fs.bsize);
        if lbn < ip.size / bsize {
            return Ok(self.fs.bsize);
        }
        if lbn == ip.size / bsize && !ip.size.is_multiple_of(bsize) {
            return Ok(fragroundup(&self.fs, (ip.size % bsize) as u32));
        }
        Ok(self.fs.bsize)
    }

    /// Store one inode block after encoding the changed fields.
    pub(crate) fn store_inode(&mut self, ino: Ino, inode: &Inode) -> Result<(), UfsError> {
        let (fsba, slot) = self.fs.ino_to_fsba(ino)?;
        let mut block = self.read_frags(fsba, self.fs.bsize)?;
        let offset = slot
            .checked_mul(self.fs.inode_size())
            .ok_or(UfsError::Corrupt("inode slot"))?;
        let end = offset
            .checked_add(self.fs.inode_size())
            .ok_or(UfsError::Corrupt("inode slot"))?;
        let raw = block
            .get_mut(offset..end)
            .ok_or(UfsError::Corrupt("inode slot"))?;
        crate::dinode::encode(self.fs.ufs2, raw, inode)?;
        self.write_block_at(fsba, &block)?;
        self.inodes.retain(|entry| entry.ino != ino);
        if let Some(journal) = self.journal.as_mut() {
            if inode.nlink == 0 {
                journal.note_inode(ino, u32::from(inode.mode));
            } else {
                journal.forget_inode(ino);
            }
        }
        Ok(())
    }

    /// Store an inode and patch a fast symlink target into the inode `di_db` area.
    pub(crate) fn store_fast_symlink(
        &mut self,
        ino: Ino,
        inode: &Inode,
        target: &[u8],
    ) -> Result<(), UfsError> {
        let (fsba, slot) = self.fs.ino_to_fsba(ino)?;
        let mut block = self.read_frags(fsba, self.fs.bsize)?;
        let offset = slot
            .checked_mul(self.fs.inode_size())
            .ok_or(UfsError::Corrupt("inode slot"))?;
        let end = offset
            .checked_add(self.fs.inode_size())
            .ok_or(UfsError::Corrupt("inode slot"))?;
        let raw = block
            .get_mut(offset..end)
            .ok_or(UfsError::Corrupt("inode slot"))?;
        crate::dinode::encode(self.fs.ufs2, raw, inode)?;
        let db_off = if self.fs.ufs2 { 112usize } else { 40usize };
        let dst = raw
            .get_mut(db_off..db_off + target.len())
            .ok_or(UfsError::Corrupt("fast symlink target"))?;
        dst.copy_from_slice(target);
        self.write_block_at(fsba, &block)?;
        self.inodes.retain(|entry| entry.ino != ino);
        if let Some(journal) = self.journal.as_mut() {
            journal.forget_inode(ino);
        }
        Ok(())
    }

    /// Set the data pointer for `lbn`, which must already have its indirect blocks.
    pub(crate) fn set_data_pointer(
        &mut self,
        ip: &mut Inode,
        lbn: u64,
        value: u64,
    ) -> Result<(), UfsError> {
        match ufs_bmap::plan(lbn, self.fs.nindir())? {
            BmapPlan::Direct(index) => {
                let slot = ip
                    .db
                    .get_mut(index)
                    .ok_or(UfsError::Corrupt("direct block index"))?;
                *slot = value;
                Ok(())
            }
            BmapPlan::Indirect { level, indices } => {
                let mut current = *ip
                    .ib
                    .get((level - 1) as usize)
                    .ok_or(UfsError::Corrupt("indirect level"))?;
                if current == 0 {
                    return Err(UfsError::Corrupt("missing indirect block"));
                }
                for &index in indices.iter().take(level as usize - 1) {
                    let block = self.read_frags(current, self.fs.bsize)?;
                    current = get_pointer(&block, index as usize, self.fs.ptr_size())?;
                    if current == 0 {
                        return Err(UfsError::Corrupt("missing indirect block"));
                    }
                }
                let mut block = self.read_frags(current, self.fs.bsize)?;
                let index = indices[level as usize - 1] as usize;
                set_pointer(&mut block, index, value, self.fs.ptr_size())?;
                self.write_block_at(current, &block)
            }
        }
    }

    /// Allocate the data block for `lbn` (and any missing indirect blocks), returning its fragment.
    pub(crate) fn bmap_alloc(
        &mut self,
        ino: Ino,
        ip: &mut Inode,
        lbn: u64,
        data_size: u32,
    ) -> Result<u64, UfsError> {
        let plan = ufs_bmap::plan(lbn, self.fs.nindir())?;
        match plan {
            BmapPlan::Direct(index) => {
                let existing = *ip
                    .db
                    .get(index)
                    .ok_or(UfsError::Corrupt("direct block index"))?;
                if existing != 0 {
                    return Ok(existing);
                }
                let pref = self.data_preference(ino, lbn);
                let addr = self.alloc_block(ino, lbn, pref, data_size)?;
                ip.db[index] = addr;
                ip.blocks = ip
                    .blocks
                    .checked_add(u64::from(data_size / 512))
                    .ok_or(UfsError::Corrupt("block count"))?;
                Ok(addr)
            }
            BmapPlan::Indirect { level, indices } => {
                let root_index = (level - 1) as usize;
                let mut current = *ip
                    .ib
                    .get(root_index)
                    .ok_or(UfsError::Corrupt("indirect level"))?;
                if current == 0 {
                    current = self.alloc_block(ino, lbn, None, self.fs.bsize)?;
                    let zero = vec![0u8; self.fs.bsize as usize];
                    self.write_meta_frags(current, &zero)?;
                    ip.ib[root_index] = current;
                    ip.blocks = ip
                        .blocks
                        .checked_add(u64::from(self.fs.bsize / 512))
                        .ok_or(UfsError::Corrupt("block count"))?;
                }
                for (depth, index) in indices.iter().enumerate() {
                    let mut block = self.read_frags(current, self.fs.bsize)?;
                    let index = *index as usize;
                    let existing = get_pointer(&block, index, self.fs.ptr_size())?;
                    if depth + 1 == level as usize {
                        if existing != 0 {
                            return Ok(existing);
                        }
                        let pref = self.data_preference(ino, lbn);
                        let addr = self.alloc_block(ino, lbn, pref, data_size)?;
                        ip.blocks = ip
                            .blocks
                            .checked_add(u64::from(data_size / 512))
                            .ok_or(UfsError::Corrupt("block count"))?;
                        set_pointer(&mut block, index, addr, self.fs.ptr_size())?;
                        self.write_block_at(current, &block)?;
                        return Ok(addr);
                    }
                    let child = if existing != 0 {
                        existing
                    } else {
                        let child = self.alloc_block(ino, lbn, None, self.fs.bsize)?;
                        let zero = vec![0u8; self.fs.bsize as usize];
                        self.write_meta_frags(child, &zero)?;
                        ip.blocks = ip
                            .blocks
                            .checked_add(u64::from(self.fs.bsize / 512))
                            .ok_or(UfsError::Corrupt("block count"))?;
                        set_pointer(&mut block, index, child, self.fs.ptr_size())?;
                        self.write_block_at(current, &block)?;
                        child
                    };
                    current = child;
                }
                Err(UfsError::Corrupt("indirect depth"))
            }
        }
    }

    fn data_preference(&mut self, ino: Ino, lbn: u64) -> Option<u64> {
        let pref = self.blkpref(ino, lbn, 0, &[]);
        if pref == 0 { None } else { Some(pref) }
    }
}

fn fragroundup(fs: &Superblock, value: u32) -> u32 {
    let mask = fs.fsize - 1;
    (value + mask) & !mask
}

fn get_pointer(block: &[u8], index: usize, width: usize) -> Result<u64, UfsError> {
    let offset = index
        .checked_mul(width)
        .ok_or(UfsError::Corrupt("pointer offset"))?;
    let bytes = block
        .get(offset..offset + width)
        .ok_or(UfsError::Corrupt("pointer outside block"))?;
    if width == 8 {
        let mut value = [0u8; 8];
        value.copy_from_slice(bytes);
        Ok(u64::from_le_bytes(value))
    } else {
        let mut value = [0u8; 4];
        value.copy_from_slice(bytes);
        Ok(u64::from(u32::from_le_bytes(value)))
    }
}

fn set_pointer(block: &mut [u8], index: usize, value: u64, width: usize) -> Result<(), UfsError> {
    let offset = index
        .checked_mul(width)
        .ok_or(UfsError::Corrupt("pointer offset"))?;
    let bytes = block
        .get_mut(offset..offset + width)
        .ok_or(UfsError::Corrupt("pointer outside block"))?;
    if width == 8 {
        bytes.copy_from_slice(&value.to_le_bytes());
    } else {
        let value = u32::try_from(value).map_err(|_| UfsError::Corrupt("32-bit block pointer"))?;
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}
