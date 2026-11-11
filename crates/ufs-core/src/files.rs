// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ffs/ffs_inode.c (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01), with write-path behaviour from
// sys/ufs/ufs/ufs_readwrite.c and setattr from sys/ufs/ufs/ufs_inode.c.
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

//! File data path: write, truncate, setattr, inode sync.

use alloc_crate::vec;
use alloc_crate::vec::Vec;

use crate::blockio::BlockIo;
use crate::dinode::Inode;
use crate::error::UfsError;
use crate::types::{FileType, Ino, SetAttr, Timespec};
use crate::ufs::Ufs;

impl<D: BlockIo> Ufs<D> {
    /// Write `data` at `offset`, extending the file and allocating blocks/fragments as needed.
    ///
    /// A write past EOF leaves a sparse gap that reads as zeros; the block that holds the new
    /// last byte is allocated. On `NoSpace` the inode is updated to the bytes already written
    /// and the error is returned.
    pub fn write(&mut self, ino: Ino, offset: u64, data: &[u8]) -> Result<usize, UfsError> {
        self.ensure_writable()?;
        let mut inode = self.load_inode(ino)?;
        match inode.file_type()? {
            FileType::Regular => {}
            FileType::Symlink => return self.write_symlink(ino, &mut inode, offset, data),
            FileType::Directory => return Err(UfsError::IsDir),
            _ => return Err(UfsError::Invalid),
        }
        if data.is_empty() {
            return Ok(0);
        }
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or(UfsError::Invalid)?;
        let now = self.next_time();
        let mut result = Ok(());
        if offset > inode.size && inode.size > 0 {
            // Growing into a later block makes the old partial tail an interior block, which
            // must be a full allocation. Bytes past the old EOF stay zero.
            let bsize = u64::from(self.fs.bsize);
            let old_lbn = (inode.size - 1) / bsize;
            if old_lbn < (end - 1) / bsize {
                let old_off = inode.size - old_lbn * bsize;
                let old_alloc = fragroundup(self.fs.fsize, old_off as u32);
                if old_alloc < self.fs.bsize {
                    let old_size = inode.size;
                    result = self.ensure_block(ino, &mut inode, old_lbn, self.fs.bsize, old_size);
                }
            }
        }
        if result.is_ok() {
            result = self.write_range(ino, &mut inode, offset, data);
        }
        if result.is_ok() && inode.size < end {
            inode.size = end;
        }
        inode.mtime = Timespec { sec: now, nsec: 0 };
        inode.ctime = inode.mtime;
        let stored = self.store_inode(ino, &inode);
        if let Err(error) = result {
            let _ = stored;
            return Err(error);
        }
        stored?;
        Ok(data.len())
    }

    /// Truncate or grow a regular file or symlink to `size`.
    ///
    /// Shrinking clears pointers and frees data and indirect blocks; growing creates a sparse hole.
    pub fn truncate(&mut self, ino: Ino, size: u64) -> Result<(), UfsError> {
        self.ensure_writable()?;
        let mut inode = self.load_inode(ino)?;
        match inode.file_type()? {
            FileType::Regular => {}
            FileType::Symlink => return self.truncate_symlink(ino, &mut inode, size),
            FileType::Directory => return Err(UfsError::IsDir),
            _ => return Err(UfsError::Invalid),
        }
        let now = self.next_time();
        inode.mtime = Timespec { sec: now, nsec: 0 };
        inode.ctime = inode.mtime;
        if size >= inode.size {
            if size > inode.size {
                self.grow_file(ino, &mut inode, size)?;
                inode.blocks = self.count_file_blocks(&inode, size)?;
            }
            inode.size = size;
            return self.store_inode(ino, &inode);
        }
        let old_size = inode.size;
        let bsize = u64::from(self.fs.bsize);
        let keep = if size == 0 { 0 } else { (size - 1) / bsize + 1 };
        let old_blocks = old_size.div_ceil(bsize);
        let mut data_frees: Vec<(u64, u32)> = Vec::new();
        let mut tail_free: Option<(u64, u32)> = None;
        let mut full_tops: Vec<(u64, u8, u64)> = Vec::new();
        let mut partial_tops: Vec<(u64, u8, u64)> = Vec::new();

        // Trim the tail of the block that contains the new EOF. When the new size falls inside
        // that block, fragments past the rounded-up size are returned to the free map. A last
        // block that was full (or made full by a previous grow) shrinks to a fragment.
        if size > 0 {
            let last = (size - 1) / bsize;
            let old_alloc = if last < old_size / bsize {
                self.fs.bsize
            } else {
                fragroundup(self.fs.fsize, (old_size - last * bsize) as u32)
            };
            let new_off = size - last * bsize;
            let new_alloc = fragroundup(self.fs.fsize, new_off as u32);
            if (new_alloc < old_alloc || new_off < u64::from(new_alloc))
                && let Some(addr) = self.bmap(&inode, last)?
            {
                // Zero the bytes between the new EOF and the retained fragment end so a
                // later grow cannot expose stale data.
                let mut buffer = self.read_frags(addr, new_alloc)?;
                for byte in &mut buffer[new_off as usize..] {
                    *byte = 0;
                }
                self.write_frags(addr, &buffer)?;
                if new_alloc < old_alloc {
                    tail_free = Some((
                        addr + u64::from(new_alloc / self.fs.fsize),
                        old_alloc - new_alloc,
                    ));
                }
            }
        }

        for lbn in keep..old_blocks {
            if lbn < 12 {
                let index = lbn as usize;
                if let Some(slot) = inode.db.get_mut(index)
                    && *slot != 0
                {
                    let addr = *slot;
                    *slot = 0;
                    data_frees.push((
                        addr,
                        data_block_size(self.fs.bsize, self.fs.fsize, old_size, lbn),
                    ));
                }
            }
        }
        let nindir = u64::from(self.fs.nindir);
        for level in 1..=3u8 {
            let index = (level - 1) as usize;
            let top = inode.ib[index];
            if top == 0 {
                continue;
            }
            let base = match level {
                1 => 12,
                2 => 12 + nindir,
                _ => 12 + nindir + nindir * nindir,
            };
            if base >= keep {
                inode.ib[index] = 0;
                full_tops.push((top, level, base));
            } else {
                partial_tops.push((top, level, base));
            }
        }
        inode.size = size;
        // The shortened inode (size, cleared direct pointers and dropped top pointers) is
        // written before any block is released, as classic FFS orders it.
        self.store_inode(ino, &inode)?;

        if let Some((addr, len)) = tail_free {
            self.free_frags(addr, len)?;
        }
        for (addr, len) in data_frees {
            self.free_frags(addr, len)?;
        }
        for (top, level, base) in full_tops {
            self.free_subtree(top, level, base, old_size)?;
        }

        let mut empty_tops: Vec<(u64, u8)> = Vec::new();
        for (top, level, base) in partial_tops {
            if self.trim_indirect(top, level, base, keep, old_size)? {
                inode.ib[(level - 1) as usize] = 0;
                empty_tops.push((top, level));
            }
        }
        inode.blocks = self.count_file_blocks(&inode, size)?;
        self.store_inode(ino, &inode)?;
        for (top, _) in empty_tops {
            self.free_frags(top, self.fs.bsize)?;
        }
        Ok(())
    }

    /// Change inode attributes; a `size` change is performed through `truncate`.
    pub fn setattr(&mut self, ino: Ino, attr: SetAttr) -> Result<(), UfsError> {
        self.ensure_writable()?;
        if let Some(size) = attr.size {
            self.truncate(ino, size)?;
        }
        let mut inode = self.load_inode(ino)?;
        if let Some(mode) = attr.mode {
            inode.mode = (inode.mode & 0o170000) | (mode & 0o7777);
        }
        if let Some(uid) = attr.uid {
            inode.uid = uid;
        }
        if let Some(gid) = attr.gid {
            inode.gid = gid;
        }
        if let Some(atime) = attr.atime {
            inode.atime = atime;
        }
        if let Some(mtime) = attr.mtime {
            inode.mtime = mtime;
        }
        let now = self.next_time();
        inode.ctime = Timespec { sec: now, nsec: 0 };
        self.store_inode(ino, &inode)
    }

    /// Write the current inode metadata to disk.
    pub fn sync_inode(&mut self, ino: Ino) -> Result<(), UfsError> {
        if self.read_only {
            return Ok(());
        }
        let inode = self.load_inode(ino)?;
        self.store_inode(ino, &inode)
    }

    /// Flush the inode and the underlying device.
    pub fn fsync(&mut self, ino: Ino) -> Result<(), UfsError> {
        self.sync_inode(ino)?;
        self.disk.flush().map_err(|_| UfsError::Io)
    }

    fn write_range(
        &mut self,
        ino: Ino,
        inode: &mut Inode,
        offset: u64,
        data: &[u8],
    ) -> Result<(), UfsError> {
        let bsize = u64::from(self.fs.bsize);
        let old_size = inode.size;
        let final_size = inode.size.max(
            offset
                .checked_add(data.len() as u64)
                .ok_or(UfsError::Invalid)?,
        );
        let mut done = 0usize;
        while done < data.len() {
            let pos = offset.checked_add(done as u64).ok_or(UfsError::Invalid)?;
            let lbn = pos / bsize;
            let block_off = (pos % bsize) as u32;
            let chunk = (data.len() - done).min(self.fs.bsize as usize - block_off as usize);
            if chunk == 0 {
                break;
            }
            let existing = self.allocated_size(inode, lbn)?;
            // Every block before the final one is a full allocation; the block that holds the
            // last byte of the file is allocated to cover the final size, so overwriting a
            // partial tail does not grow it.
            let last_lbn = (final_size - 1) / bsize;
            let desired = if lbn < last_lbn {
                self.fs.bsize
            } else {
                fragroundup(self.fs.fsize, (final_size - lbn * bsize) as u32)
            };
            let (addr, mut buffer) = if existing == 0 {
                let addr = self.bmap_alloc(ino, inode, lbn, desired)?;
                (addr, vec![0u8; desired as usize])
            } else {
                let addr = self
                    .bmap(inode, lbn)?
                    .ok_or(UfsError::Corrupt("missing data block"))?;
                if desired > existing {
                    self.move_block_grow(ino, inode, lbn, addr, (existing, desired), old_size)?
                } else {
                    (addr, self.read_frags(addr, existing)?)
                }
            };
            if old_size < offset {
                let block_start = lbn * bsize;
                let from = old_size.max(block_start);
                let to = offset.min(block_start + bsize);
                if from < to {
                    let start = (from - block_start) as usize;
                    let end = (to - block_start) as usize;
                    let gap = buffer
                        .get_mut(start..end)
                        .ok_or(UfsError::Corrupt("gap outside block"))?;
                    gap.fill(0);
                }
            }
            let src = data.get(done..done + chunk).ok_or(UfsError::Invalid)?;
            let dst = buffer
                .get_mut(block_off as usize..block_off as usize + chunk)
                .ok_or(UfsError::Corrupt("block offset"))?;
            dst.copy_from_slice(src);
            self.write_frags(addr, &buffer)?;
            inode.size = inode.size.max(pos + chunk as u64);
            done += chunk;
        }
        Ok(())
    }

    fn write_symlink(
        &mut self,
        ino: Ino,
        inode: &mut Inode,
        offset: u64,
        data: &[u8],
    ) -> Result<usize, UfsError> {
        if offset != 0 {
            return Err(UfsError::Invalid);
        }
        if data.len() > self.fs.bsize as usize {
            return Err(UfsError::Invalid);
        }
        let max = self.fs.maxsymlinklen.max(0) as usize;
        let now = self.next_time();
        if data.len() < max {
            if !inode.is_fast_symlink(self.fs.maxsymlinklen)
                && let Some(addr) = self.bmap(inode, 0)?
            {
                let size = self.allocated_size(inode, 0)?;
                self.free_frags(addr, size)?;
            }
            inode.db = [0; 12];
            inode.ib = [0; 3];
            inode.size = data.len() as u64;
            inode.blocks = 0;
            inode.mtime = Timespec { sec: now, nsec: 0 };
            inode.ctime = inode.mtime;
            self.store_fast_symlink(ino, inode, data)?;
            return Ok(data.len());
        }
        let desired = fragroundup(self.fs.fsize, data.len() as u32);
        if !inode.is_fast_symlink(self.fs.maxsymlinklen)
            && let Some(addr) = self.bmap(inode, 0)?
        {
            let old = self.allocated_size(inode, 0)?;
            self.free_frags(addr, old)?;
            inode.blocks = inode.blocks.saturating_sub(u64::from(old / 512));
        }
        inode.db = [0; 12];
        inode.ib = [0; 3];
        inode.blocks = 0;
        let addr = self.bmap_alloc(ino, inode, 0, desired)?;
        let mut buffer = vec![0u8; desired as usize];
        buffer[..data.len()].copy_from_slice(data);
        self.write_frags(addr, &buffer)?;
        inode.size = data.len() as u64;
        inode.mtime = Timespec { sec: now, nsec: 0 };
        inode.ctime = inode.mtime;
        self.store_inode(ino, inode)?;
        Ok(data.len())
    }

    fn truncate_symlink(&mut self, ino: Ino, inode: &mut Inode, size: u64) -> Result<(), UfsError> {
        if size != 0 {
            return Err(UfsError::Invalid);
        }
        if !inode.is_fast_symlink(self.fs.maxsymlinklen)
            && let Some(addr) = self.bmap(inode, 0)?
        {
            let old = self.allocated_size(inode, 0)?;
            self.free_frags(addr, old)?;
            inode.blocks = inode.blocks.saturating_sub(u64::from(old / 512));
        }
        inode.db = [0; 12];
        inode.ib = [0; 3];
        inode.size = 0;
        let now = self.next_time();
        inode.mtime = Timespec { sec: now, nsec: 0 };
        inode.ctime = inode.mtime;
        self.store_inode(ino, inode)
    }

    /// Grow `lbn` by copying its contents into a new allocation, zero-filling the new space.
    ///
    /// The new block is written and the inode pointer is made durable before the old
    /// allocation is returned to the free map.
    fn move_block_grow(
        &mut self,
        ino: Ino,
        inode: &mut Inode,
        lbn: u64,
        addr: u64,
        sizes: (u32, u32),
        zero_from: u64,
    ) -> Result<(u64, Vec<u8>), UfsError> {
        let (existing, desired) = sizes;
        let new_addr = self.alloc_block(ino, lbn, None, desired)?;
        let old = self.read_frags(addr, existing)?;
        let keep = zero_from.saturating_sub(lbn * u64::from(self.fs.bsize)) as usize;
        let keep = keep.min(old.len());
        let mut buffer = vec![0u8; desired as usize];
        buffer[..keep].copy_from_slice(&old[..keep]);
        self.write_frags(new_addr, &buffer)?;
        self.set_data_pointer(inode, lbn, new_addr)?;
        inode.blocks = inode
            .blocks
            .saturating_add(u64::from((desired - existing) / 512));
        self.store_inode(ino, inode)?;
        self.free_frags(addr, existing)?;
        Ok((new_addr, buffer))
    }

    /// Extend `lbn` so it owns at least `desired` bytes, zero-filling the new space.
    fn ensure_block(
        &mut self,
        ino: Ino,
        inode: &mut Inode,
        lbn: u64,
        desired: u32,
        zero_from: u64,
    ) -> Result<(), UfsError> {
        let existing = self.allocated_size(inode, lbn)?;
        if existing >= desired {
            return Ok(());
        }
        if let Some(addr) = self.bmap(inode, lbn)? {
            self.move_block_grow(ino, inode, lbn, addr, (existing, desired), zero_from)?;
        } else {
            let addr = self.bmap_alloc(ino, inode, lbn, desired)?;
            let zero = vec![0u8; desired as usize];
            self.write_frags(addr, &zero)?;
        }
        Ok(())
    }

    /// Allocate the last block for a grown file and keep the old tail block full.
    fn grow_file(&mut self, ino: Ino, inode: &mut Inode, size: u64) -> Result<(), UfsError> {
        if size == 0 {
            return Ok(());
        }
        let bsize = u64::from(self.fs.bsize);
        let new_lbn = (size - 1) / bsize;
        let new_off = size - new_lbn * bsize;
        let new_alloc = fragroundup(self.fs.fsize, new_off as u32);
        if inode.size > 0 {
            let old_size = inode.size;
            let old_lbn = (old_size - 1) / bsize;
            if old_lbn < new_lbn {
                let old_off = old_size - old_lbn * bsize;
                let old_alloc = fragroundup(self.fs.fsize, old_off as u32);
                if old_alloc < self.fs.bsize {
                    self.ensure_block(ino, inode, old_lbn, self.fs.bsize, old_size)?;
                }
                self.ensure_block(ino, inode, new_lbn, new_alloc, new_lbn * bsize)
            } else {
                self.ensure_block(ino, inode, new_lbn, new_alloc, old_size)
            }
        } else {
            self.ensure_block(ino, inode, new_lbn, new_alloc, new_lbn * bsize)
        }
    }

    fn trim_indirect(
        &mut self,
        block_addr: u64,
        level: u8,
        base_lbn: u64,
        keep: u64,
        old_size: u64,
    ) -> Result<bool, UfsError> {
        let mut block = self.read_frags(block_addr, self.fs.bsize)?;
        let nindir = self.fs.nindir() as usize;
        let span = nindir.pow(u32::from(level) - 1);
        let mut empty = true;
        let mut data_frees: Vec<(u64, u32)> = Vec::new();
        let mut child_frees: Vec<(u64, u8, u64)> = Vec::new();
        let mut changed = false;
        for index in 0..nindir {
            let child_lbn = base_lbn
                .checked_add((index as u64) * span as u64)
                .ok_or(UfsError::Corrupt("indirect lbn"))?;
            let ptr = get_ptr(&block, index, self.fs.ptr_size())?;
            if ptr == 0 {
                continue;
            }
            if child_lbn >= keep {
                set_ptr(&mut block, index, 0, self.fs.ptr_size())?;
                changed = true;
                if level == 1 {
                    data_frees.push((
                        ptr,
                        data_block_size(self.fs.bsize, self.fs.fsize, old_size, child_lbn),
                    ));
                } else {
                    child_frees.push((ptr, level - 1, child_lbn));
                }
            } else if level > 1 {
                let child_empty = self.trim_indirect(ptr, level - 1, child_lbn, keep, old_size)?;
                if child_empty {
                    set_ptr(&mut block, index, 0, self.fs.ptr_size())?;
                    changed = true;
                    child_frees.push((ptr, level - 1, child_lbn));
                } else {
                    empty = false;
                }
            } else {
                empty = false;
            }
        }
        if changed {
            self.write_block_at(block_addr, &block)?;
        }
        for (addr, len) in data_frees {
            self.free_frags(addr, len)?;
        }
        for (addr, level, base) in child_frees {
            self.free_subtree(addr, level, base, old_size)?;
        }
        Ok(empty)
    }

    fn free_subtree(
        &mut self,
        block_addr: u64,
        level: u8,
        base_lbn: u64,
        old_size: u64,
    ) -> Result<(), UfsError> {
        let block = self.read_frags(block_addr, self.fs.bsize)?;
        let nindir = self.fs.nindir() as usize;
        let span = nindir.pow(u32::from(level) - 1);
        for index in 0..nindir {
            let ptr = get_ptr(&block, index, self.fs.ptr_size())?;
            if ptr == 0 {
                continue;
            }
            let child_lbn = base_lbn
                .checked_add((index as u64) * span as u64)
                .ok_or(UfsError::Corrupt("indirect lbn"))?;
            if level == 1 {
                let len = data_block_size(self.fs.bsize, self.fs.fsize, old_size, child_lbn);
                self.free_frags(ptr, len)?;
            } else {
                self.free_subtree(ptr, level - 1, child_lbn, old_size)?;
            }
        }
        self.free_frags(block_addr, self.fs.bsize)
    }

    fn count_file_blocks(&mut self, inode: &Inode, size: u64) -> Result<u64, UfsError> {
        let bsize = self.fs.bsize;
        let fsize = self.fs.fsize;
        let mut total = 0u64;
        for lbn in 0..12u64 {
            if inode.db[lbn as usize] != 0 {
                total =
                    total.saturating_add(u64::from(data_block_size(bsize, fsize, size, lbn) / 512));
            }
        }
        let nindir = u64::from(self.fs.nindir);
        for level in 1..=3u8 {
            let top = inode.ib[(level - 1) as usize];
            if top != 0 {
                let base = match level {
                    1 => 12,
                    2 => 12 + nindir,
                    _ => 12 + nindir + nindir * nindir,
                };
                total = total.saturating_add(self.count_indirect(top, level, base, size)?);
            }
        }
        Ok(total)
    }

    fn count_indirect(
        &mut self,
        block_addr: u64,
        level: u8,
        base_lbn: u64,
        size: u64,
    ) -> Result<u64, UfsError> {
        let block = self.read_frags(block_addr, self.fs.bsize)?;
        let nindir = self.fs.nindir() as usize;
        let span = nindir.pow(u32::from(level) - 1);
        let mut total = u64::from(self.fs.bsize / 512);
        for index in 0..nindir {
            let ptr = get_ptr(&block, index, self.fs.ptr_size())?;
            if ptr == 0 {
                continue;
            }
            let child_lbn = base_lbn
                .checked_add((index as u64) * span as u64)
                .ok_or(UfsError::Corrupt("indirect lbn"))?;
            if level == 1 {
                total = total.saturating_add(u64::from(
                    data_block_size(self.fs.bsize, self.fs.fsize, size, child_lbn) / 512,
                ));
            } else {
                total =
                    total.saturating_add(self.count_indirect(ptr, level - 1, child_lbn, size)?);
            }
        }
        Ok(total)
    }

    fn next_time(&mut self) -> i64 {
        let now = self.fs.time.saturating_add(1);
        self.fs.set_time_raw(now);
        now
    }
}

fn fragroundup(fsize: u32, value: u32) -> u32 {
    let mask = fsize - 1;
    (value + mask) & !mask
}

fn data_block_size(bsize: u32, fsize: u32, old_size: u64, lbn: u64) -> u32 {
    let block = u64::from(bsize);
    if lbn < old_size / block {
        bsize
    } else if lbn == old_size / block && !old_size.is_multiple_of(block) {
        fragroundup(fsize, (old_size % block) as u32)
    } else {
        bsize
    }
}

fn get_ptr(block: &[u8], index: usize, width: usize) -> Result<u64, UfsError> {
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

fn set_ptr(block: &mut [u8], index: usize, value: u64, width: usize) -> Result<(), UfsError> {
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
