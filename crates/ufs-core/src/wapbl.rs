// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// On-disk format ported from NetBSD sys/sys/wapbl_replay.h and
// sys/kern/vfs_wapbl.c (revision 36eddebcc36cb0775aa2962005b9c05419a6ff01).
// The original notices follow.
//
// Copyright (c) 2003, 2006, 2008, 2009 The NetBSD Foundation, Inc.
// All rights reserved.
//
// This code is derived from software contributed to The NetBSD Foundation
// by Wasabi Systems, Inc.
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

//! WAPBL: write-ahead physical block logging for UFS.
//!
//! The on-disk layout matches NetBSD: a log area starts with two 512-byte
//! commit headers, followed by a circular record region.  Records are a
//! 512-byte header plus (for `BLOCKS`) the block data.  A commit header stores
//! the generation, head and tail offsets; replay applies the newest copy of
//! every logged block, then the log is reset.
//!
//! NetBSD currently writes zero in `wc_checksum` and does not verify it.  This
//! implementation uses the same field for an FNV-1a checksum of the record
//! header; a zero checksum is accepted for NetBSD logs and a non-zero one is
//! verified.  A record that does not parse (a torn tail) ends replay: the
//! records before it are still applied.

use alloc_crate::collections::BTreeMap;
use alloc_crate::vec;
use alloc_crate::vec::Vec;

use crate::blockio::BlockIo;
use crate::error::{IoError, UfsError};
use crate::fs::{
    Superblock, UFS_WAPBL_JOURNALLOC_END_PARTITION, UFS_WAPBL_JOURNALLOC_IN_FILESYSTEM,
};
use crate::types::Ino;

const DEV_BSIZE: u64 = 512;
const LOG_BLOCK: usize = 512;
const CIRC_OFFSET: u64 = 2 * DEV_BSIZE;
const WAPBL_WC_HEADER: u32 = 0x5741_424c;
const WAPBL_WC_INODES: u32 = 1;
const WAPBL_WC_REVOCATIONS: u32 = 2;
const WAPBL_WC_BLOCKS: u32 = 3;
const HEADER_FIXED: usize = 80;
const BLOCK_ENTRIES: usize = (LOG_BLOCK - 16) / 16;
const INODE_ENTRIES: usize = (LOG_BLOCK - 16) / 8;

/// A parsed commit header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct JournalHeader {
    pub(crate) checksum: u32,
    pub(crate) generation: u32,
    pub(crate) head: u64,
    pub(crate) tail: u64,
    pub(crate) circ_off: u64,
    pub(crate) circ_size: u64,
    pub(crate) log_bshift: u32,
    pub(crate) fs_bshift: u32,
}

/// One WAPBL log area.
#[derive(Clone, Debug)]
pub(crate) struct Journal {
    pub(crate) start: u64,
    pub(crate) block_size: u32,
    pub(crate) circ_off: u64,
    pub(crate) circ_size: u64,
    pub(crate) head: u64,
    pub(crate) tail: u64,
    pub(crate) generation: u32,
    pub(crate) active: bool,
    pending: BTreeMap<u64, Vec<u8>>,
    inodes: BTreeMap<Ino, u32>,
}

impl Journal {
    /// Locate the journal described by the superblock.
    pub(crate) fn from_super(fs: &Superblock, device_size: u64) -> Result<Option<Self>, UfsError> {
        if !fs.has_journal() {
            return Ok(None);
        }
        if fs.journal_location != UFS_WAPBL_JOURNALLOC_END_PARTITION
            && fs.journal_location != UFS_WAPBL_JOURNALLOC_IN_FILESYSTEM
        {
            return Err(UfsError::Unsupported("unknown WAPBL journal location"));
        }
        let block_size = u32::try_from(fs.journals[2])
            .map_err(|_| UfsError::BadSuperblock("WAPBL block size"))?;
        if block_size < DEV_BSIZE as u32
            || !block_size.is_power_of_two()
            || !block_size.is_multiple_of(DEV_BSIZE as u32)
        {
            return Err(UfsError::BadSuperblock("bad WAPBL block size"));
        }
        let start = fs.journals[0]
            .checked_mul(u64::from(block_size))
            .ok_or(UfsError::BadSuperblock("WAPBL start"))?;
        let count = fs.journals[1];
        let size = count
            .checked_mul(u64::from(block_size))
            .ok_or(UfsError::BadSuperblock("WAPBL size"))?;
        if count == 0 || size < 2 * DEV_BSIZE || start.checked_add(size).is_none() {
            return Err(UfsError::BadSuperblock("bad WAPBL range"));
        }
        if start + size > device_size {
            return Err(UfsError::BadSuperblock("WAPBL beyond device"));
        }
        let circ_off = CIRC_OFFSET;
        let circ_size = (size - circ_off) & !(DEV_BSIZE - 1);
        if circ_size == 0 {
            return Err(UfsError::BadSuperblock("empty WAPBL journal"));
        }
        Ok(Some(Self {
            start,
            block_size,
            circ_off,
            circ_size,
            head: 0,
            tail: 0,
            generation: 0,
            active: false,
            pending: BTreeMap::new(),
            inodes: BTreeMap::new(),
        }))
    }

    /// Whether the log contains committed records.
    pub(crate) fn is_dirty<D: BlockIo>(&self, disk: &mut D) -> Result<bool, UfsError> {
        match self.read_header(disk)? {
            Some(header) => Ok(header.head != 0 || header.tail != 0),
            None => Ok(true),
        }
    }

    /// Load the active commit header and adopt its queue pointers.
    pub(crate) fn open<D: BlockIo>(&mut self, disk: &mut D) -> Result<(), UfsError> {
        let header = self
            .read_header(disk)?
            .ok_or(UfsError::Corrupt("invalid WAPBL header"))?;
        self.validate_header(&header)?;
        self.head = header.head;
        self.tail = header.tail;
        self.generation = header.generation;
        self.circ_off = header.circ_off;
        self.circ_size = header.circ_size;
        Ok(())
    }

    /// Make the journal active for the mount.  An empty log gets a fresh header.
    pub(crate) fn start<D: BlockIo>(&mut self, disk: &mut D) -> Result<(), UfsError> {
        if self.read_header(disk)?.is_none() {
            self.head = 0;
            self.tail = 0;
            self.generation = 0;
            let header = self.make_header(0, 0);
            self.write_header(disk, &header)?;
            disk.flush().map_err(|_| UfsError::Io)?;
        } else {
            self.open(disk)?;
        }
        self.active = true;
        Ok(())
    }

    /// Stage a metadata block.  Returns `false` when the caller must write it
    /// directly because no journal is active.
    pub(crate) fn stage(&mut self, offset: u64, data: &[u8]) -> bool {
        if !self.active {
            return false;
        }
        self.pending.insert(offset, data.to_vec());
        true
    }

    /// Whether another `len` bytes fit without committing first.
    pub(crate) fn has_room(&self, len: usize) -> bool {
        let pending: usize = self.pending.values().map(Vec::len).sum();
        pending.saturating_add(len).saturating_add(LOG_BLOCK * 4) < (self.circ_size / 2) as usize
    }

    /// Room for a whole `sync` transaction; it is not split mid-way.
    pub(crate) fn has_room_full(&self, len: usize) -> bool {
        let pending: usize = self.pending.values().map(Vec::len).sum();
        pending.saturating_add(len).saturating_add(LOG_BLOCK * 4) < self.circ_size as usize
    }

    pub(crate) fn note_inode(&mut self, ino: Ino, mode: u32) {
        if self.active {
            self.inodes.insert(ino, mode);
        }
    }

    pub(crate) fn forget_inode(&mut self, ino: Ino) {
        self.inodes.remove(&ino);
    }

    /// Read a metadata block, preferring a staged copy.
    pub(crate) fn read<D: BlockIo>(
        &self,
        disk: &mut D,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), IoError> {
        disk.read_at(offset, buf)?;
        let end = offset.checked_add(buf.len() as u64).ok_or(IoError)?;
        if let Some((&start, data)) = self.pending.range(..=offset).next_back() {
            copy_overlap(start, data, offset, buf);
        }
        for (&start, data) in self.pending.range(offset.saturating_add(1)..end) {
            copy_overlap(start, data, offset, buf);
        }
        Ok(())
    }

    /// Commit all staged metadata, checkpoint it and reset the log.
    pub(crate) fn commit<D: BlockIo>(&mut self, disk: &mut D) -> Result<(), UfsError> {
        if !self.active {
            return Ok(());
        }
        if self.pending.is_empty() && self.inodes.is_empty() {
            return Ok(());
        }
        if self.read_header(disk)?.is_none() {
            self.head = 0;
            self.tail = 0;
            self.generation = 0;
            let header = self.make_header(0, 0);
            self.write_header(disk, &header)?;
        }
        let mut off = if self.head == 0 {
            self.circ_off
        } else {
            self.head
        };
        self.write_block_records(disk, &mut off)?;
        self.write_inode_records(disk, &mut off)?;

        // The log records are only valid after the commit header is on disk.
        disk.flush().map_err(|_| UfsError::Io)?;
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.generation = 1;
        }
        let commit = self.make_header(off, self.tail);
        self.write_header(disk, &commit)?;
        disk.flush().map_err(|_| UfsError::Io)?;

        // Checkpoint every staged metadata block.
        for (offset, data) in core::mem::take(&mut self.pending) {
            disk.write_at(offset, &data).map_err(|_| UfsError::Io)?;
        }
        disk.flush().map_err(|_| UfsError::Io)?;

        // All logged blocks are now in place; mark the log empty.
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.generation = 1;
        }
        let empty = self.make_header(0, 0);
        self.write_header(disk, &empty)?;
        disk.flush().map_err(|_| UfsError::Io)?;
        self.head = 0;
        self.tail = 0;
        self.inodes.clear();
        Ok(())
    }

    /// Replay the log and reset it.  Returns the inode list for cleanup.
    pub(crate) fn replay<D: BlockIo>(
        &mut self,
        disk: &mut D,
        fs_size_bytes: u64,
    ) -> Result<Vec<(Ino, u32)>, UfsError> {
        let Some(header) = self.read_header(disk)? else {
            // A fresh, never-written log area.
            return Ok(Vec::new());
        };
        if let Err(error) = self.validate_header(&header) {
            if header.head == 0 && header.tail == 0 {
                return Ok(Vec::new());
            }
            return Err(error);
        }
        self.circ_off = header.circ_off;
        self.circ_size = header.circ_size;
        self.generation = header.generation;
        let mut blocks: BTreeMap<u64, (u64, usize)> = BTreeMap::new();
        let mut inodes: BTreeMap<Ino, u32> = BTreeMap::new();
        let mut off = if header.tail == 0 {
            header.circ_off
        } else {
            header.tail
        };
        let end = header.head;
        if end != 0 && off == end {
            return self.finish_replay(disk, fs_size_bytes, blocks, inodes);
        }
        while off != end {
            let mut raw = vec![0u8; LOG_BLOCK];
            self.circ_read(disk, off, &mut raw)?;
            let record_type = read_u32(&raw, 0)?;
            let record_len = read_i32(&raw, 4)?;
            if record_len < LOG_BLOCK as i32 || !(record_len as usize).is_multiple_of(LOG_BLOCK) {
                break;
            }
            let record_len = record_len as usize;
            let record_end = circ_advance(off, record_len as u64, self.circ_off, self.circ_size)?;
            match record_type {
                WAPBL_WC_BLOCKS => {
                    let count = read_i32(&raw, 8)?;
                    if count < 0 || count as usize > BLOCK_ENTRIES {
                        break;
                    }
                    let mut data_off =
                        circ_advance(off, LOG_BLOCK as u64, self.circ_off, self.circ_size)?;
                    for index in 0..count as usize {
                        let base = 16 + index * 16;
                        let daddr = read_u64(&raw, base)?;
                        let dlen = read_i32(&raw, base + 12)?;
                        if dlen < 0 || !(dlen as usize).is_multiple_of(512) {
                            return Err(UfsError::Corrupt("WAPBL block length"));
                        }
                        let dlen = dlen as usize;
                        let fs_offset = daddr
                            .checked_mul(DEV_BSIZE)
                            .ok_or(UfsError::Corrupt("WAPBL block address"))?;
                        if fs_offset + dlen as u64 > fs_size_bytes {
                            return Err(UfsError::Corrupt("WAPBL block outside filesystem"));
                        }
                        blocks.insert(fs_offset, (data_off, dlen));
                        data_off =
                            circ_advance(data_off, dlen as u64, self.circ_off, self.circ_size)?;
                    }
                    if data_off != record_end {
                        break;
                    }
                    off = record_end;
                }
                WAPBL_WC_REVOCATIONS => {
                    let count = read_i32(&raw, 8)?;
                    if count < 0 || count as usize > BLOCK_ENTRIES {
                        break;
                    }
                    for index in 0..count as usize {
                        let base = 16 + index * 16;
                        let daddr = read_u64(&raw, base)?;
                        let dlen = read_i32(&raw, base + 12)?;
                        if dlen < 0 {
                            break;
                        }
                        let mut at = daddr.saturating_mul(DEV_BSIZE);
                        let mut left = dlen as u64;
                        while left >= DEV_BSIZE {
                            blocks.remove(&at);
                            at = at.saturating_add(DEV_BSIZE);
                            left -= DEV_BSIZE;
                        }
                    }
                    off = record_end;
                }
                WAPBL_WC_INODES => {
                    let count = read_i32(&raw, 8)?;
                    let clear = read_i32(&raw, 12)?;
                    if count < 0 || count as usize > INODE_ENTRIES {
                        break;
                    }
                    if clear != 0 {
                        inodes.clear();
                    }
                    for index in 0..count as usize {
                        let base = 16 + index * 8;
                        let ino = read_u32(&raw, base)?;
                        let mode = read_u32(&raw, base + 4)?;
                        if ino >= 2 {
                            inodes.insert(ino, mode);
                        }
                    }
                    off = record_end;
                }
                _ => break,
            }
        }
        self.finish_replay(disk, fs_size_bytes, blocks, inodes)
    }

    fn finish_replay<D: BlockIo>(
        &mut self,
        disk: &mut D,
        _fs_size_bytes: u64,
        blocks: BTreeMap<u64, (u64, usize)>,
        inodes: BTreeMap<Ino, u32>,
    ) -> Result<Vec<(Ino, u32)>, UfsError> {
        for (offset, (log_off, len)) in blocks {
            let mut data = vec![0u8; len];
            self.circ_read(disk, log_off, &mut data)?;
            disk.write_at(offset, &data).map_err(|_| UfsError::Io)?;
        }
        disk.flush().map_err(|_| UfsError::Io)?;
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.generation = 1;
        }
        let empty = self.make_header(0, 0);
        self.write_header(disk, &empty)?;
        disk.flush().map_err(|_| UfsError::Io)?;
        self.head = 0;
        self.tail = 0;
        Ok(inodes.into_iter().collect())
    }

    // --- records -----------------------------------------------------------

    fn write_block_records<D: BlockIo>(
        &mut self,
        disk: &mut D,
        off: &mut u64,
    ) -> Result<(), UfsError> {
        let blocks: Vec<(u64, Vec<u8>)> = self
            .pending
            .iter()
            .map(|(offset, data)| (*offset, data.clone()))
            .collect();
        let max_block = blocks.iter().map(|(_, data)| data.len()).max().unwrap_or(0);
        let limit = (self.circ_size / 2) as usize;
        if max_block.saturating_add(LOG_BLOCK) > limit {
            return Err(UfsError::Unsupported(
                "WAPBL log too small for metadata block",
            ));
        }
        let per_record = ((limit - LOG_BLOCK) / max_block.max(1)).clamp(1, BLOCK_ENTRIES);
        for chunk in blocks.chunks(per_record) {
            let mut header = vec![0u8; LOG_BLOCK];
            write_u32(&mut header, 0, WAPBL_WC_BLOCKS)?;
            let total: usize = LOG_BLOCK + chunk.iter().map(|(_, data)| data.len()).sum::<usize>();
            write_i32(
                &mut header,
                4,
                i32::try_from(total).map_err(|_| UfsError::Invalid)?,
            )?;
            write_i32(&mut header, 8, chunk.len() as i32)?;
            for (index, (offset, data)) in chunk.iter().enumerate() {
                let base = 16 + index * 16;
                write_u64(&mut header, base, offset / DEV_BSIZE)?;
                write_i32(
                    &mut header,
                    base + 12,
                    i32::try_from(data.len()).map_err(|_| UfsError::Invalid)?,
                )?;
            }
            self.circ_write(disk, *off, &header)?;
            *off = circ_advance(*off, LOG_BLOCK as u64, self.circ_off, self.circ_size)?;
            for (_, data) in chunk {
                self.circ_write(disk, *off, data)?;
                *off = circ_advance(*off, data.len() as u64, self.circ_off, self.circ_size)?;
            }
        }
        Ok(())
    }

    fn write_inode_records<D: BlockIo>(
        &mut self,
        disk: &mut D,
        off: &mut u64,
    ) -> Result<(), UfsError> {
        if self.inodes.is_empty() {
            return Ok(());
        }
        let inodes: Vec<(Ino, u32)> = self
            .inodes
            .iter()
            .map(|(ino, mode)| (*ino, *mode))
            .collect();
        for (chunk_index, chunk) in inodes.chunks(INODE_ENTRIES).enumerate() {
            let mut header = vec![0u8; LOG_BLOCK];
            write_u32(&mut header, 0, WAPBL_WC_INODES)?;
            write_i32(&mut header, 4, LOG_BLOCK as i32)?;
            write_i32(&mut header, 8, chunk.len() as i32)?;
            write_i32(&mut header, 12, i32::from(chunk_index == 0))?;
            for (index, (ino, mode)) in chunk.iter().enumerate() {
                let base = 16 + index * 8;
                write_u32(&mut header, base, *ino)?;
                write_u32(&mut header, base + 4, *mode)?;
            }
            self.circ_write(disk, *off, &header)?;
            *off = circ_advance(*off, LOG_BLOCK as u64, self.circ_off, self.circ_size)?;
        }
        Ok(())
    }

    fn circ_write<D: BlockIo>(
        &self,
        disk: &mut D,
        mut off: u64,
        data: &[u8],
    ) -> Result<(), UfsError> {
        if off < self.circ_off {
            off = self.circ_off;
        }
        let end = self.circ_off + self.circ_size;
        let first = (end - off) as usize;
        if data.len() > first {
            disk.write_at(self.start + off, &data[..first])
                .map_err(|_| UfsError::Io)?;
            disk.write_at(self.start + self.circ_off, &data[first..])
                .map_err(|_| UfsError::Io)?;
        } else {
            disk.write_at(self.start + off, data)
                .map_err(|_| UfsError::Io)?;
        }
        Ok(())
    }

    fn circ_read<D: BlockIo>(
        &self,
        disk: &mut D,
        mut off: u64,
        data: &mut [u8],
    ) -> Result<(), UfsError> {
        if off < self.circ_off {
            off = self.circ_off;
        }
        let end = self.circ_off + self.circ_size;
        let first = (end - off) as usize;
        if data.len() > first {
            disk.read_at(self.start + off, &mut data[..first])
                .map_err(|_| UfsError::Io)?;
            disk.read_at(self.start + self.circ_off, &mut data[first..])
                .map_err(|_| UfsError::Io)?;
        } else {
            disk.read_at(self.start + off, data)
                .map_err(|_| UfsError::Io)?;
        }
        Ok(())
    }

    fn read_header<D: BlockIo>(&self, disk: &mut D) -> Result<Option<JournalHeader>, UfsError> {
        let mut first = vec![0u8; LOG_BLOCK];
        let mut second = vec![0u8; LOG_BLOCK];
        if disk.read_at(self.start, &mut first).is_err()
            || disk.read_at(self.start + DEV_BSIZE, &mut second).is_err()
        {
            return Ok(None);
        }
        let a = parse_header(&first, self.block_size)?;
        let b = parse_header(&second, self.block_size)?;
        Ok(match (a, b) {
            (Some(a), Some(b)) => {
                if b.generation > a.generation {
                    Some(b)
                } else {
                    Some(a)
                }
            }
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        })
    }

    fn validate_header(&self, header: &JournalHeader) -> Result<(), UfsError> {
        if header.log_bshift != self.block_size.trailing_zeros()
            || header.circ_off != CIRC_OFFSET
            || header.circ_size == 0
            || header.circ_off + header.circ_size > self.circ_off + self.circ_size
        {
            return Err(UfsError::Corrupt("bad WAPBL geometry"));
        }
        if header.head != 0
            && (header.head < header.circ_off || header.head >= header.circ_off + header.circ_size)
        {
            return Err(UfsError::Corrupt("bad WAPBL head"));
        }
        if header.tail != 0
            && (header.tail < header.circ_off || header.tail >= header.circ_off + header.circ_size)
        {
            return Err(UfsError::Corrupt("bad WAPBL tail"));
        }
        Ok(())
    }

    fn make_header(&self, head: u64, tail: u64) -> JournalHeader {
        JournalHeader {
            checksum: 0,
            generation: self.generation,
            head,
            tail,
            circ_off: self.circ_off,
            circ_size: self.circ_size,
            log_bshift: self.block_size.trailing_zeros(),
            fs_bshift: DEV_BSIZE.trailing_zeros(),
        }
    }

    fn write_header<D: BlockIo>(
        &self,
        disk: &mut D,
        header: &JournalHeader,
    ) -> Result<(), UfsError> {
        let mut raw = vec![0u8; LOG_BLOCK];
        encode_header(&mut raw, header)?;
        let slot = u64::from(header.generation % 2) * DEV_BSIZE;
        disk.write_at(self.start + slot, &raw)
            .map_err(|_| UfsError::Io)
    }
}

/// Write an empty WAPBL log header at `start` for a freshly formatted image.
pub(crate) fn format_log<D: BlockIo>(
    disk: &mut D,
    start: u64,
    size: u64,
    block_size: u32,
) -> Result<(), UfsError> {
    if size < 2 * DEV_BSIZE || block_size < DEV_BSIZE as u32 {
        return Err(UfsError::Invalid);
    }
    let circ_off = CIRC_OFFSET;
    let circ_size = (size - circ_off) & !(DEV_BSIZE - 1);
    let mut journal = Journal {
        start,
        block_size,
        circ_off,
        circ_size,
        head: 0,
        tail: 0,
        generation: 0,
        active: false,
        pending: BTreeMap::new(),
        inodes: BTreeMap::new(),
    };
    let header = journal.make_header(0, 0);
    journal.write_header(disk, &header)?;
    journal.generation = 1;
    let header = journal.make_header(0, 0);
    journal.write_header(disk, &header)?;
    disk.flush().map_err(|_| UfsError::Io)
}

// --- record encoding -------------------------------------------------------

fn parse_header(raw: &[u8], block_size: u32) -> Result<Option<JournalHeader>, UfsError> {
    if read_u32(raw, 0)? != WAPBL_WC_HEADER {
        return Ok(None);
    }
    let len = read_i32(raw, 4)?;
    if len != block_size as i32 {
        return Ok(None);
    }
    let checksum = read_u32(raw, 8)?;
    if checksum != 0 && checksum != header_checksum(raw) {
        return Ok(None);
    }
    let header = JournalHeader {
        checksum,
        generation: read_u32(raw, 12)?,
        head: u64::try_from(read_i64(raw, 48)?).map_err(|_| UfsError::Corrupt("WAPBL head"))?,
        tail: u64::try_from(read_i64(raw, 56)?).map_err(|_| UfsError::Corrupt("WAPBL tail"))?,
        circ_off: u64::try_from(read_i64(raw, 64)?)
            .map_err(|_| UfsError::Corrupt("WAPBL circ offset"))?,
        circ_size: u64::try_from(read_i64(raw, 72)?)
            .map_err(|_| UfsError::Corrupt("WAPBL circ size"))?,
        log_bshift: read_u32(raw, 40)?,
        fs_bshift: read_u32(raw, 44)?,
    };
    Ok(Some(header))
}

fn encode_header(raw: &mut [u8], header: &JournalHeader) -> Result<(), UfsError> {
    write_u32(raw, 0, WAPBL_WC_HEADER)?;
    write_i32(raw, 4, LOG_BLOCK as i32)?;
    write_u32(raw, 8, 0)?;
    write_u32(raw, 12, header.generation)?;
    write_u64(raw, 24, 0)?;
    write_u32(raw, 32, 0)?;
    write_u32(raw, 36, 1)?;
    write_u32(raw, 40, header.log_bshift)?;
    write_u32(raw, 44, header.fs_bshift)?;
    write_i64(
        raw,
        48,
        i64::try_from(header.head).map_err(|_| UfsError::Invalid)?,
    )?;
    write_i64(
        raw,
        56,
        i64::try_from(header.tail).map_err(|_| UfsError::Invalid)?,
    )?;
    write_i64(
        raw,
        64,
        i64::try_from(header.circ_off).map_err(|_| UfsError::Invalid)?,
    )?;
    write_i64(
        raw,
        72,
        i64::try_from(header.circ_size).map_err(|_| UfsError::Invalid)?,
    )?;
    let checksum = header_checksum(raw);
    write_u32(raw, 8, checksum)?;
    Ok(())
}

fn header_checksum(raw: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for (index, byte) in raw.iter().take(HEADER_FIXED.min(raw.len())).enumerate() {
        if (8..12).contains(&index) {
            continue;
        }
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn copy_overlap(start: u64, data: &[u8], offset: u64, buf: &mut [u8]) {
    let src_end = start + data.len() as u64;
    let dst_end = offset + buf.len() as u64;
    let from = start.max(offset);
    let to = src_end.min(dst_end);
    if from >= to {
        return;
    }
    let src = (from - start) as usize;
    let dst = (from - offset) as usize;
    let len = (to - from) as usize;
    if let (Some(src), Some(dst)) = (data.get(src..src + len), buf.get_mut(dst..dst + len)) {
        dst.copy_from_slice(src);
    }
}

fn circ_advance(off: u64, delta: u64, circ_off: u64, circ_size: u64) -> Result<u64, UfsError> {
    if delta > circ_size {
        return Err(UfsError::Corrupt("WAPBL record too long"));
    }
    let base = if off < circ_off { circ_off } else { off };
    let end = circ_off + circ_size;
    let mut next = base
        .checked_add(delta)
        .ok_or(UfsError::Corrupt("WAPBL offset"))?;
    if next >= end {
        next -= circ_size;
    }
    Ok(next)
}

fn read_u32(data: &[u8], off: usize) -> Result<u32, UfsError> {
    let bytes = data
        .get(off..off + 4)
        .ok_or(UfsError::Corrupt("short WAPBL field"))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_i32(data: &[u8], off: usize) -> Result<i32, UfsError> {
    Ok(read_u32(data, off)? as i32)
}

fn read_u64(data: &[u8], off: usize) -> Result<u64, UfsError> {
    let bytes = data
        .get(off..off + 8)
        .ok_or(UfsError::Corrupt("short WAPBL field"))?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(value))
}

fn read_i64(data: &[u8], off: usize) -> Result<i64, UfsError> {
    Ok(read_u64(data, off)? as i64)
}

fn write_u32(data: &mut [u8], off: usize, value: u32) -> Result<(), UfsError> {
    let dst = data
        .get_mut(off..off + 4)
        .ok_or(UfsError::Corrupt("short WAPBL field"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_i32(data: &mut [u8], off: usize, value: i32) -> Result<(), UfsError> {
    write_u32(data, off, value as u32)
}

fn write_u64(data: &mut [u8], off: usize, value: u64) -> Result<(), UfsError> {
    let dst = data
        .get_mut(off..off + 8)
        .ok_or(UfsError::Corrupt("short WAPBL field"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_i64(data: &mut [u8], off: usize, value: i64) -> Result<(), UfsError> {
    write_u64(data, off, value as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockio::BlockIo;

    struct MemDisk(Vec<u8>);

    impl BlockIo for MemDisk {
        fn size_bytes(&self) -> u64 {
            self.0.len() as u64
        }

        fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
            let start = usize::try_from(offset).map_err(|_| IoError)?;
            let end = start.checked_add(buf.len()).ok_or(IoError)?;
            let src = self.0.get(start..end).ok_or(IoError)?;
            buf.copy_from_slice(src);
            Ok(())
        }

        fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
            let start = usize::try_from(offset).map_err(|_| IoError)?;
            let end = start.checked_add(buf.len()).ok_or(IoError)?;
            let dst = self.0.get_mut(start..end).ok_or(IoError)?;
            dst.copy_from_slice(buf);
            Ok(())
        }

        fn flush(&mut self) -> Result<(), IoError> {
            Ok(())
        }
    }

    fn test_journal(start: u64, size: u64) -> Journal {
        Journal {
            start,
            block_size: 512,
            circ_off: CIRC_OFFSET,
            circ_size: (size - CIRC_OFFSET) & !(DEV_BSIZE - 1),
            head: 0,
            tail: 0,
            generation: 0,
            active: true,
            pending: BTreeMap::new(),
            inodes: BTreeMap::new(),
        }
    }

    fn committed_log(disk: &mut MemDisk, journal: &mut Journal, blocks: &[(u64, u8)]) -> u64 {
        for (offset, value) in blocks {
            journal.stage(*offset, &vec![*value; 512]);
        }
        let mut off = journal.circ_off;
        journal
            .write_block_records(disk, &mut off)
            .expect("records");
        journal.generation = 1;
        let header = journal.make_header(off, 0);
        journal.write_header(disk, &header).expect("header");
        off
    }

    #[test]
    fn header_round_trip() {
        let mut raw = vec![0u8; LOG_BLOCK];
        let header = JournalHeader {
            checksum: 0,
            generation: 7,
            head: 4096,
            tail: 1024,
            circ_off: CIRC_OFFSET,
            circ_size: 1 << 20,
            log_bshift: 9,
            fs_bshift: 9,
        };
        encode_header(&mut raw, &header).expect("encode");
        let parsed = parse_header(&raw, LOG_BLOCK as u32)
            .expect("parse")
            .expect("header");
        assert_eq!(parsed.generation, 7);
        assert_eq!(parsed.head, 4096);
        assert_eq!(parsed.tail, 1024);
        assert_eq!(parsed.circ_off, CIRC_OFFSET);
        assert_eq!(parsed.circ_size, 1 << 20);
    }

    #[test]
    fn checksum_rejects_damage() {
        let mut raw = vec![0u8; LOG_BLOCK];
        let header = JournalHeader {
            checksum: 0,
            generation: 1,
            head: 0,
            tail: 0,
            circ_off: CIRC_OFFSET,
            circ_size: 4096,
            log_bshift: 9,
            fs_bshift: 9,
        };
        encode_header(&mut raw, &header).expect("encode");
        raw[12] ^= 0x40;
        assert!(
            parse_header(&raw, LOG_BLOCK as u32)
                .expect("parse")
                .is_none()
        );
    }

    #[test]
    fn zero_checksum_is_netbsd_compatible() {
        let mut raw = vec![0u8; LOG_BLOCK];
        write_u32(&mut raw, 0, WAPBL_WC_HEADER).expect("type");
        write_i32(&mut raw, 4, LOG_BLOCK as i32).expect("len");
        write_u32(&mut raw, 12, 3).expect("generation");
        write_u32(&mut raw, 40, 9).expect("log shift");
        write_u32(&mut raw, 44, 9).expect("fs shift");
        write_i64(&mut raw, 64, CIRC_OFFSET as i64).expect("circ");
        write_i64(&mut raw, 72, 4096).expect("size");
        assert!(
            parse_header(&raw, LOG_BLOCK as u32)
                .expect("parse")
                .is_some()
        );
    }

    #[test]
    fn circular_advance_wraps() {
        assert_eq!(circ_advance(1024, 512, 1024, 4096).expect("advance"), 1536);
        assert_eq!(circ_advance(4608, 512, 1024, 4096).expect("advance"), 1024);
        assert_eq!(circ_advance(1024, 2048, 1024, 4096).expect("advance"), 3072);
    }

    #[test]
    fn overlap_copy_works() {
        let mut buf = [0u8; 8];
        copy_overlap(4, &[1, 2, 3, 4], 6, &mut buf);
        assert_eq!(buf, [3, 4, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn checksum_is_stable() {
        assert_eq!(header_checksum(b""), 0x811c_9dc5);
        assert_ne!(header_checksum(b"a"), header_checksum(b"b"));
    }

    #[test]
    fn block_records_replay_to_the_filesystem() {
        let mut disk = MemDisk(vec![0u8; 2 * 1024 * 1024]);
        let mut journal = test_journal(1024 * 1024, 1024 * 1024);
        committed_log(&mut disk, &mut journal, &[(4096, 0x11), (8192, 0x22)]);
        let mut replay = test_journal(1024 * 1024, 1024 * 1024);
        replay.replay(&mut disk, 2 * 1024 * 1024).expect("replay");
        assert_eq!(&disk.0[4096..4608], &[0x11; 512]);
        assert_eq!(&disk.0[8192..8704], &[0x22; 512]);
        let (head, tail) = replay
            .read_header(&mut disk)
            .expect("header")
            .map(|header| (header.head, header.tail))
            .unwrap_or((0, 0));
        assert_eq!((head, tail), (0, 0));
    }

    #[test]
    fn last_staged_version_wins() {
        let mut disk = MemDisk(vec![0u8; 2 * 1024 * 1024]);
        let mut journal = test_journal(1024 * 1024, 1024 * 1024);
        journal.stage(4096, &[0x33; 512]);
        journal.stage(4096, &[0x44; 512]);
        let mut off = journal.circ_off;
        journal
            .write_block_records(&mut disk, &mut off)
            .expect("records");
        journal.generation = 1;
        let header = journal.make_header(off, 0);
        journal.write_header(&mut disk, &header).expect("header");
        let mut replay = test_journal(1024 * 1024, 1024 * 1024);
        replay.replay(&mut disk, 2 * 1024 * 1024).expect("replay");
        assert_eq!(&disk.0[4096..4608], &[0x44; 512]);
    }

    #[test]
    fn torn_tail_is_ignored() {
        let mut disk = MemDisk(vec![0u8; 4 * 1024 * 1024]);
        let mut journal = test_journal(1024 * 1024, 2 * 1024 * 1024);
        // 32 blocks make two BLOCKS records; damage the second record header.
        let blocks: Vec<(u64, u8)> = (0..32)
            .map(|index| (4096 + index * 512, 0x80 | (index as u8 & 0x7f)))
            .collect();
        let mut off = journal.circ_off;
        for (offset, value) in &blocks {
            journal.stage(*offset, &vec![*value; 512]);
        }
        journal
            .write_block_records(&mut disk, &mut off)
            .expect("records");
        journal.generation = 1;
        let header = journal.make_header(off, 0);
        journal.write_header(&mut disk, &header).expect("header");
        // The second BLOCKS record starts after the first 31 blocks.
        let second = journal.circ_off + 512 + 31 * 512;
        disk.0[(1024 * 1024 + second) as usize..(1024 * 1024 + second + 4) as usize]
            .copy_from_slice(&0u32.to_le_bytes());
        let mut replay = test_journal(1024 * 1024, 2 * 1024 * 1024);
        replay.replay(&mut disk, 4 * 1024 * 1024).expect("replay");
        assert_eq!(&disk.0[4096..4608], &[0x80; 512]);
        assert_eq!(&disk.0[4096 + 31 * 512..4096 + 32 * 512], &[0; 512]);
    }

    #[test]
    fn inodes_are_returned_for_cleanup() {
        let mut disk = MemDisk(vec![0u8; 2 * 1024 * 1024]);
        let mut journal = test_journal(1024 * 1024, 1024 * 1024);
        journal.note_inode(42, 0o100644);
        journal.note_inode(43, 0);
        let mut off = journal.circ_off;
        journal
            .write_inode_records(&mut disk, &mut off)
            .expect("inodes");
        journal.generation = 1;
        let header = journal.make_header(off, 0);
        journal.write_header(&mut disk, &header).expect("header");
        let mut replay = test_journal(1024 * 1024, 1024 * 1024);
        let inodes = replay.replay(&mut disk, 2 * 1024 * 1024).expect("replay");
        assert_eq!(inodes, vec![(42, 0o100644), (43, 0)]);
    }

    #[test]
    fn empty_log_is_clean() {
        let mut disk = MemDisk(vec![0u8; 2 * 1024 * 1024]);
        format_log(&mut disk, 1024 * 1024, 1024 * 1024, 512).expect("format log");
        let journal = test_journal(1024 * 1024, 1024 * 1024);
        assert!(!journal.is_dirty(&mut disk).expect("dirty"));
    }

    #[test]
    fn commit_stages_and_resets() {
        let mut disk = MemDisk(vec![0u8; 2 * 1024 * 1024]);
        let mut journal = test_journal(1024 * 1024, 1024 * 1024);
        journal.stage(4096, &[0x55; 512]);
        journal.stage(8192, &[0x66; 512]);
        journal.commit(&mut disk).expect("commit");
        assert_eq!(&disk.0[4096..4608], &[0x55; 512]);
        assert_eq!(&disk.0[8192..8704], &[0x66; 512]);
        assert!(!journal.is_dirty(&mut disk).expect("dirty"));
        assert!(journal.pending.is_empty());
    }

    #[test]
    fn block_record_has_the_netbsd_layout() {
        let mut disk = MemDisk(vec![0u8; 2 * 1024 * 1024]);
        let mut journal = test_journal(1024 * 1024, 1024 * 1024);
        journal.stage(4096, &[0x77; 512]);
        let mut off = journal.circ_off;
        journal
            .write_block_records(&mut disk, &mut off)
            .expect("record");
        let start = (1024 * 1024 + journal.circ_off) as usize;
        assert_eq!(read_u32(&disk.0, start).expect("type"), WAPBL_WC_BLOCKS);
        assert_eq!(read_i32(&disk.0, start + 4).expect("len"), 1024);
        assert_eq!(read_i32(&disk.0, start + 8).expect("count"), 1);
        assert_eq!(read_u64(&disk.0, start + 16).expect("daddr"), 8);
        assert_eq!(read_i32(&disk.0, start + 28).expect("dlen"), 512);
    }

    #[test]
    fn circular_io_wraps() {
        let mut disk = MemDisk(vec![0u8; 2 * 1024 * 1024]);
        let journal = test_journal(1024 * 1024, 3 * 1024);
        let start = journal.circ_off + journal.circ_size - 400;
        let data = vec![0x5a; 1000];
        journal.circ_write(&mut disk, start, &data).expect("write");
        let mut read = vec![0u8; 1000];
        journal
            .circ_read(&mut disk, start, &mut read)
            .expect("read");
        assert_eq!(read, data);
    }

    #[test]
    fn bad_header_geometry_is_rejected() {
        let journal = test_journal(1024 * 1024, 1024 * 1024);
        let mut header = journal.make_header(0, 0);
        header.circ_off = 0;
        assert!(journal.validate_header(&header).is_err());
        header = journal.make_header(0, 0);
        header.head = header.circ_off + header.circ_size;
        assert!(journal.validate_header(&header).is_err());
    }

    #[test]
    fn generation_selects_the_newer_header() {
        let mut disk = MemDisk(vec![0u8; 2 * 1024 * 1024]);
        let journal = test_journal(1024 * 1024, 1024 * 1024);
        let older = JournalHeader {
            checksum: 0,
            generation: 3,
            head: 0,
            tail: 0,
            circ_off: CIRC_OFFSET,
            circ_size: journal.circ_size,
            log_bshift: 9,
            fs_bshift: 9,
        };
        let newer = JournalHeader {
            generation: 4,
            ..older.clone()
        };
        journal.write_header(&mut disk, &older).expect("old");
        let mut newer_journal = test_journal(1024 * 1024, 1024 * 1024);
        newer_journal.generation = 4;
        newer_journal.write_header(&mut disk, &newer).expect("new");
        let found = newer_journal
            .read_header(&mut disk)
            .expect("read")
            .expect("header");
        assert_eq!(found.generation, 4);
    }

    #[test]
    fn revocation_removes_a_logged_block() {
        let mut disk = MemDisk(vec![0u8; 2 * 1024 * 1024]);
        let mut journal = test_journal(1024 * 1024, 1024 * 1024);
        let mut off = journal.circ_off;
        let mut blocks = vec![0u8; 512];
        write_u32(&mut blocks, 0, WAPBL_WC_BLOCKS).expect("type");
        write_i32(&mut blocks, 4, 1024).expect("len");
        write_i32(&mut blocks, 8, 1).expect("count");
        write_u64(&mut blocks, 16, 8).expect("daddr");
        write_i32(&mut blocks, 28, 512).expect("dlen");
        journal.circ_write(&mut disk, off, &blocks).expect("blocks");
        off = circ_advance(off, 512, journal.circ_off, journal.circ_size).expect("advance");
        journal
            .circ_write(&mut disk, off, &[0xaa; 512])
            .expect("data");
        off = circ_advance(off, 512, journal.circ_off, journal.circ_size).expect("advance");
        let mut revoke = vec![0u8; 512];
        write_u32(&mut revoke, 0, WAPBL_WC_REVOCATIONS).expect("type");
        write_i32(&mut revoke, 4, 512).expect("len");
        write_i32(&mut revoke, 8, 1).expect("count");
        write_u64(&mut revoke, 16, 8).expect("daddr");
        write_i32(&mut revoke, 28, 512).expect("dlen");
        journal.circ_write(&mut disk, off, &revoke).expect("revoke");
        off = circ_advance(off, 512, journal.circ_off, journal.circ_size).expect("advance");
        journal.generation = 1;
        let header = journal.make_header(off, 0);
        journal.write_header(&mut disk, &header).expect("header");
        let mut replay = test_journal(1024 * 1024, 1024 * 1024);
        replay.replay(&mut disk, 2 * 1024 * 1024).expect("replay");
        assert_eq!(&disk.0[4096..4608], &[0; 512]);
    }
}
