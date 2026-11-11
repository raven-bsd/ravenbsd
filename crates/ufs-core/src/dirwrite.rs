// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ufs/ufs_lookup.c (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notice follows.
//
// Copyright (c) 1989, 1993
//	The Regents of the University of California.  All rights reserved.
// (c) UNIX System Laboratories, Inc.
// All or some portions of this file are derived from material licensed
// to the University of California by American Telephone and Telegraph
// Co. or Unix System Laboratories, Inc. and are reproduced herein with
// the permission of UNIX System Laboratories, Inc.
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

//! Writable directory entry machinery: lookup, insertion, removal and rewrite.

use alloc_crate::vec::Vec;

use crate::blockio::BlockIo;
use crate::dinode::Inode;
use crate::dir::{self, DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG, DT_SOCK, RawEntry};
use crate::error::UfsError;
use crate::fs::{FFS_MAXNAMLEN, UFS_DIRBLKSIZ};
use crate::types::{FileType, Ino};
use crate::ufs::Ufs;

/// NetBSD `UFS_WINO`: the inode number used for whiteout entries.
pub(crate) const UFS_WINO: u32 = 1;

/// One located live directory entry and the record before it in the same 512-byte block.
#[derive(Clone, Debug)]
pub(crate) struct LocatedEntry {
    pub(crate) offset: u64,
    pub(crate) prev: Option<u64>,
    pub(crate) block: u64,
    pub(crate) entry: RawEntry,
}

/// NetBSD `UFS_DIRSIZ`: the minimum record length for `namlen` bytes.
pub(crate) fn dirsiz(namlen: usize) -> usize {
    8 + ((namlen + 1 + 3) & !3)
}

/// Validate a user-supplied name for lookup or removal.
pub(crate) fn validate_name(name: &[u8]) -> Result<(), UfsError> {
    if name.is_empty() {
        return Err(UfsError::Invalid);
    }
    if name.len() > FFS_MAXNAMLEN {
        return Err(UfsError::NameTooLong);
    }
    if name.contains(&0) || name.contains(&b'/') {
        return Err(UfsError::Invalid);
    }
    Ok(())
}

/// Validate a name for a new directory entry, rejecting `.` and `..`.
pub(crate) fn validate_new_name(name: &[u8]) -> Result<(), UfsError> {
    validate_name(name)?;
    if name == b"." || name == b".." {
        return Err(UfsError::Invalid);
    }
    Ok(())
}

/// Map an inode type to the on-disk `d_type`.
pub(crate) fn dtype(kind: FileType) -> u8 {
    match kind {
        FileType::Fifo => DT_FIFO,
        FileType::CharDevice => DT_CHR,
        FileType::Directory => DT_DIR,
        FileType::BlockDevice => DT_BLK,
        FileType::Regular => DT_REG,
        FileType::Symlink => DT_LNK,
        FileType::Socket => DT_SOCK,
    }
}

fn fragroundup(fsize: u32, value: u32) -> Result<u32, UfsError> {
    let mask = fsize - 1;
    value
        .checked_add(mask)
        .map(|rounded| rounded & !mask)
        .ok_or(UfsError::Corrupt("fragment roundup"))
}

fn write_entry(
    buf: &mut [u8],
    off: usize,
    ino: u32,
    entry_type: u8,
    name: &[u8],
    reclen: usize,
) -> Result<(), UfsError> {
    if reclen & 0x3 != 0 || reclen < dirsiz(name.len()) {
        return Err(UfsError::Corrupt("directory record length"));
    }
    let end = off
        .checked_add(reclen)
        .ok_or(UfsError::Corrupt("directory record offset"))?;
    let dst = buf
        .get_mut(off..end)
        .ok_or(UfsError::Corrupt("directory record outside block"))?;
    dst.fill(0);
    dst[0..4].copy_from_slice(&ino.to_le_bytes());
    let reclen16 =
        u16::try_from(reclen).map_err(|_| UfsError::Corrupt("directory record length"))?;
    dst[4..6].copy_from_slice(&reclen16.to_le_bytes());
    dst[6] = entry_type;
    dst[7] = u8::try_from(name.len()).map_err(|_| UfsError::NameTooLong)?;
    dst[8..8 + name.len()].copy_from_slice(name);
    Ok(())
}

/// Build the initial `.` and `..` contents of a new directory.
pub(crate) fn dot_entries(dir: Ino, parent: Ino) -> Result<[u8; UFS_DIRBLKSIZ], UfsError> {
    let mut buf = [0u8; UFS_DIRBLKSIZ];
    write_entry(&mut buf, 0, dir, DT_DIR, b".", 12)?;
    write_entry(&mut buf, 12, parent, DT_DIR, b"..", UFS_DIRBLKSIZ - 12)?;
    Ok(buf)
}

fn read_reclen(buf: &[u8], off: usize) -> Result<usize, UfsError> {
    let bytes = buf
        .get(off + 4..off + 6)
        .ok_or(UfsError::Corrupt("directory record length"))?;
    Ok(usize::from(u16::from_le_bytes([bytes[0], bytes[1]])))
}

fn write_reclen(buf: &mut [u8], off: usize, reclen: usize) -> Result<(), UfsError> {
    let value = u16::try_from(reclen).map_err(|_| UfsError::Corrupt("directory record length"))?;
    let bytes = buf
        .get_mut(off + 4..off + 6)
        .ok_or(UfsError::Corrupt("directory record length"))?;
    bytes.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

/// Try to insert one entry in a 512-byte directory block, compacting if needed.
fn insert_in_block(
    buf: &mut [u8; UFS_DIRBLKSIZ],
    name: &[u8],
    target: Ino,
    entry_type: u8,
    need: usize,
) -> Result<bool, UfsError> {
    let mut off = 0usize;
    let mut used: Vec<(u32, u8, Vec<u8>)> = Vec::new();
    while off < UFS_DIRBLKSIZ {
        let entry = dir::parse(&buf[off..], UFS_DIRBLKSIZ - off)?
            .ok_or(UfsError::Corrupt("truncated directory block"))?;
        let next = off
            .checked_add(entry.next)
            .ok_or(UfsError::Corrupt("directory offset"))?;
        if next > UFS_DIRBLKSIZ {
            return Err(UfsError::Corrupt("directory entry crosses block"));
        }
        if entry.ino == 0 {
            if entry.next >= need {
                if entry.next >= need + 8 {
                    write_entry(buf, off, target, entry_type, name, need)?;
                    write_entry(buf, off + need, 0, 0, b"", entry.next - need)?;
                } else {
                    write_entry(buf, off, target, entry_type, name, entry.next)?;
                }
                return Ok(true);
            }
        } else {
            let used_len = dirsiz(entry.name.len());
            if entry.next >= used_len + need {
                write_reclen(buf, off, used_len)?;
                write_entry(
                    buf,
                    off + used_len,
                    target,
                    entry_type,
                    name,
                    entry.next - used_len,
                )?;
                return Ok(true);
            }
            used.push((entry.ino, entry.dtype, entry.name));
        }
        off = next;
    }

    let total: usize = used.iter().map(|(_, _, name)| dirsiz(name.len())).sum();
    if total > UFS_DIRBLKSIZ {
        return Err(UfsError::Corrupt("directory block overflow"));
    }
    let free = UFS_DIRBLKSIZ - total;
    if free < need {
        return Ok(false);
    }
    let mut repacked = [0u8; UFS_DIRBLKSIZ];
    let mut pos = 0usize;
    for (ino, entry_type, name) in &used {
        let used_len = dirsiz(name.len());
        write_entry(&mut repacked, pos, *ino, *entry_type, name, used_len)?;
        pos += used_len;
    }
    write_entry(&mut repacked, pos, target, entry_type, name, free)?;
    buf.copy_from_slice(&repacked);
    Ok(true)
}

impl<D: BlockIo> Ufs<D> {
    /// Find a live entry by name, including `.` and `..`; whiteouts are invisible.
    pub(crate) fn lookup_entry(
        &mut self,
        dir: Ino,
        name: &[u8],
    ) -> Result<Option<LocatedEntry>, UfsError> {
        let ip = self.load_inode(dir)?;
        if ip.file_type()? != FileType::Directory {
            return Err(UfsError::NotDir);
        }
        if !ip.size.is_multiple_of(UFS_DIRBLKSIZ as u64) {
            return Err(UfsError::Corrupt("directory size"));
        }
        let mut block = 0u64;
        while block < ip.size {
            let block_end = block + UFS_DIRBLKSIZ as u64;
            let mut off = block;
            let mut prev = None;
            while off < block_end {
                let Some(entry) = self.read_dir_entry(&ip, off)? else {
                    break;
                };
                let next = off
                    .checked_add(entry.next as u64)
                    .ok_or(UfsError::Corrupt("directory offset"))?;
                if next > block_end {
                    return Err(UfsError::Corrupt("directory entry crosses block"));
                }
                if entry.ino != 0 && entry.ino != UFS_WINO && entry.name.as_slice() == name {
                    return Ok(Some(LocatedEntry {
                        offset: off,
                        prev,
                        block,
                        entry,
                    }));
                }
                prev = Some(off);
                off = next;
            }
            block = block_end;
        }
        Ok(None)
    }

    fn dir_block(&mut self, ip: &Inode, block: u64) -> Result<[u8; UFS_DIRBLKSIZ], UfsError> {
        let mut buf = [0u8; UFS_DIRBLKSIZ];
        let got = self.read_inode_data(ip, block, &mut buf)?;
        if got != UFS_DIRBLKSIZ {
            return Err(UfsError::Corrupt("short directory block"));
        }
        Ok(buf)
    }

    /// Write one 512-byte directory block, allocating or extending its fragment as needed.
    pub(crate) fn write_dir_block(
        &mut self,
        ino: Ino,
        ip: &mut Inode,
        offset: u64,
        buf: &[u8; UFS_DIRBLKSIZ],
    ) -> Result<(), UfsError> {
        if !offset.is_multiple_of(UFS_DIRBLKSIZ as u64) {
            return Err(UfsError::Invalid);
        }
        let bsize = u64::from(self.fs.bsize);
        let lbn = offset / bsize;
        let block_off = (offset % bsize) as u32;
        let cover = fragroundup(
            self.fs.fsize,
            block_off
                .checked_add(UFS_DIRBLKSIZ as u32)
                .ok_or(UfsError::Corrupt("directory block offset"))?,
        )?;
        let existing = self.allocated_size(ip, lbn)?;
        if existing < cover {
            self.ensure_block(ino, ip, lbn, cover, ip.size)?;
        }
        let addr = self
            .bmap(ip, lbn)?
            .ok_or(UfsError::Corrupt("missing directory block"))?;
        let mut data = self.read_frags(addr, cover)?;
        let start = block_off as usize;
        let end = start + UFS_DIRBLKSIZ;
        let dst = data
            .get_mut(start..end)
            .ok_or(UfsError::Corrupt("directory block offset"))?;
        dst.copy_from_slice(buf);
        self.write_meta_frags(addr, &data)
    }

    /// Update a directory's modification times and write the inode.
    pub(crate) fn touch_inode(&mut self, ino: Ino, ip: &mut Inode) -> Result<(), UfsError> {
        let now = self.next_time();
        ip.mtime = crate::types::Timespec { sec: now, nsec: 0 };
        ip.ctime = ip.mtime;
        self.store_inode(ino, ip)
    }

    /// Insert `name` in `dir`, growing the directory by one 512-byte block if needed.
    pub(crate) fn dir_insert(
        &mut self,
        dir: Ino,
        dp: &mut Inode,
        name: &[u8],
        target: Ino,
        entry_type: u8,
    ) -> Result<(), UfsError> {
        validate_new_name(name)?;
        if self.lookup_entry(dir, name)?.is_some() {
            return Err(UfsError::Exists);
        }
        let need = dirsiz(name.len());
        let mut block = 0u64;
        while block < dp.size {
            let mut buf = self.dir_block(dp, block)?;
            if insert_in_block(&mut buf, name, target, entry_type, need)? {
                self.write_dir_block(dir, dp, block, &buf)?;
                self.touch_inode(dir, dp)?;
                return Ok(());
            }
            block += UFS_DIRBLKSIZ as u64;
        }
        if !dp.size.is_multiple_of(UFS_DIRBLKSIZ as u64) {
            return Err(UfsError::Corrupt("directory size"));
        }
        let offset = dp.size;
        let mut buf = [0u8; UFS_DIRBLKSIZ];
        write_entry(&mut buf, 0, target, entry_type, name, UFS_DIRBLKSIZ)?;
        self.write_dir_block(dir, dp, offset, &buf)?;
        dp.size = offset + UFS_DIRBLKSIZ as u64;
        self.touch_inode(dir, dp)
    }

    /// Remove a located directory entry, merging its space into the previous record.
    pub(crate) fn dir_remove_entry(
        &mut self,
        dir: Ino,
        dp: &mut Inode,
        located: &LocatedEntry,
    ) -> Result<(), UfsError> {
        let mut buf = self.dir_block(dp, located.block)?;
        let off = usize::try_from(located.offset - located.block)
            .map_err(|_| UfsError::Corrupt("directory offset"))?;
        if let Some(prev) = located.prev {
            let prev_off = usize::try_from(prev - located.block)
                .map_err(|_| UfsError::Corrupt("directory offset"))?;
            let prev_len = read_reclen(&buf, prev_off)?;
            let merged = prev_len
                .checked_add(located.entry.next)
                .ok_or(UfsError::Corrupt("directory record length"))?;
            if merged > UFS_DIRBLKSIZ {
                return Err(UfsError::Corrupt("directory record crosses block"));
            }
            write_reclen(&mut buf, prev_off, merged)?;
        } else {
            let ino = buf
                .get_mut(off..off + 4)
                .ok_or(UfsError::Corrupt("directory offset"))?;
            ino.copy_from_slice(&0u32.to_le_bytes());
        }
        self.write_dir_block(dir, dp, located.block, &buf)?;
        self.touch_inode(dir, dp)
    }

    /// Rewrite a directory entry to reference another inode.
    pub(crate) fn dir_rewrite_entry(
        &mut self,
        dir: Ino,
        dp: &mut Inode,
        offset: u64,
        target: Ino,
        entry_type: u8,
    ) -> Result<(), UfsError> {
        let block = offset - offset % UFS_DIRBLKSIZ as u64;
        let mut buf = self.dir_block(dp, block)?;
        let off =
            usize::try_from(offset - block).map_err(|_| UfsError::Corrupt("directory offset"))?;
        let reclen = read_reclen(&buf, off)?;
        if reclen == 0 || off + reclen > UFS_DIRBLKSIZ {
            return Err(UfsError::Corrupt("directory record length"));
        }
        let slot = buf
            .get_mut(off..off + 4)
            .ok_or(UfsError::Corrupt("directory offset"))?;
        slot.copy_from_slice(&target.to_le_bytes());
        let kind = buf
            .get_mut(off + 6)
            .ok_or(UfsError::Corrupt("directory offset"))?;
        *kind = entry_type;
        self.write_dir_block(dir, dp, block, &buf)?;
        self.touch_inode(dir, dp)
    }

    /// NetBSD `ufs_dirempty`: true when only the correct `.` and `..` entries remain.
    pub(crate) fn dir_is_empty(
        &mut self,
        dir: Ino,
        ip: &Inode,
        parent: Ino,
    ) -> Result<bool, UfsError> {
        if !ip.size.is_multiple_of(UFS_DIRBLKSIZ as u64) {
            return Err(UfsError::Corrupt("directory size"));
        }
        let mut dot = false;
        let mut dotdot = false;
        let mut block = 0u64;
        while block < ip.size {
            let buf = self.dir_block(ip, block)?;
            let mut off = 0usize;
            while off < UFS_DIRBLKSIZ {
                let entry = dir::parse(&buf[off..], UFS_DIRBLKSIZ - off)?
                    .ok_or(UfsError::Corrupt("truncated directory block"))?;
                let next = off
                    .checked_add(entry.next)
                    .ok_or(UfsError::Corrupt("directory offset"))?;
                if next > UFS_DIRBLKSIZ {
                    return Err(UfsError::Corrupt("directory entry crosses block"));
                }
                if entry.ino != 0 && entry.ino != UFS_WINO {
                    match entry.name.as_slice() {
                        b"." => {
                            if entry.ino != dir {
                                return Ok(false);
                            }
                            dot = true;
                        }
                        b".." => {
                            if entry.ino != parent {
                                return Ok(false);
                            }
                            dotdot = true;
                        }
                        _ => return Ok(false),
                    }
                }
                off = next;
            }
            block += UFS_DIRBLKSIZ as u64;
        }
        if !dot || !dotdot {
            return Err(UfsError::Corrupt("directory missing dot entries"));
        }
        Ok(true)
    }
}
