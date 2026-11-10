// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ufs/dir.h (revision 36eddebcc36cb0775aa2962005b9c05419a6ff01).
// The original notice follows.
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

//! UFS directory entry decoding and `ufs_dirbadentry` validation.

use alloc::vec::Vec;

use crate::error::UfsError;
use crate::fs::{FFS_MAXNAMLEN, UFS_DIRBLKSIZ};
use crate::types::FileType;

pub(crate) const DT_UNKNOWN: u8 = 0;
pub(crate) const DT_FIFO: u8 = 1;
pub(crate) const DT_CHR: u8 = 2;
pub(crate) const DT_DIR: u8 = 4;
pub(crate) const DT_BLK: u8 = 6;
pub(crate) const DT_REG: u8 = 8;
pub(crate) const DT_LNK: u8 = 10;
pub(crate) const DT_SOCK: u8 = 12;
pub(crate) const DT_WHT: u8 = 14;

/// One validated on-disk directory record.
#[derive(Clone, Debug)]
pub(crate) struct RawEntry {
    pub(crate) ino: u32,
    pub(crate) dtype: u8,
    pub(crate) name: Vec<u8>,
    pub(crate) next: usize,
}

/// Decode one directory record from `buf`, which starts at the record.
///
/// `max` is the number of bytes until the enclosing 512-byte directory block boundary (or EOF);
/// a record may not cross it. Free records have `ino == 0` and an empty name.
pub(crate) fn parse(buf: &[u8], max: usize) -> Result<Option<RawEntry>, UfsError> {
    if buf.is_empty() {
        return Ok(None);
    }
    if buf.len() < 8 || max < 8 {
        return Err(UfsError::Corrupt("truncated directory entry"));
    }
    let ino = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let reclen = usize::from(u16::from_le_bytes([buf[4], buf[5]]));
    let dtype = buf[6];
    let namlen = usize::from(buf[7]);

    if reclen == 0 {
        return Err(UfsError::Corrupt("zero directory record length"));
    }
    if reclen & 0x3 != 0 {
        return Err(UfsError::Corrupt("unaligned directory record"));
    }
    if reclen > max || reclen > buf.len() {
        return Err(UfsError::Corrupt("directory record too long"));
    }
    if namlen > FFS_MAXNAMLEN {
        return Err(UfsError::Corrupt("directory name too long"));
    }
    let dirsiz = 8usize
        .checked_add((namlen + 1 + 3) & !3)
        .ok_or(UfsError::Corrupt("directory size overflow"))?;
    if dirsiz > reclen {
        return Err(UfsError::Corrupt("directory record too small"));
    }

    if ino == 0 {
        return Ok(Some(RawEntry {
            ino: 0,
            dtype: 0,
            name: Vec::new(),
            next: reclen,
        }));
    }

    let name_end = 8usize
        .checked_add(namlen)
        .ok_or(UfsError::Corrupt("directory name overflow"))?;
    let name = buf
        .get(8..name_end)
        .ok_or(UfsError::Corrupt("directory name out of record"))?;
    if name.contains(&0) {
        return Err(UfsError::Corrupt("NUL in directory name"));
    }
    if buf.get(name_end) != Some(&0) {
        return Err(UfsError::Corrupt("missing NUL after directory name"));
    }
    Ok(Some(RawEntry {
        ino,
        dtype,
        name: name.to_vec(),
        next: reclen,
    }))
}

/// Map a `DT_*` value to a file type, or `None` when the inode must be consulted.
pub(crate) fn file_type_from_dtype(dtype: u8) -> Option<FileType> {
    match dtype {
        DT_UNKNOWN => None,
        DT_FIFO => Some(FileType::Fifo),
        DT_CHR => Some(FileType::CharDevice),
        DT_DIR => Some(FileType::Directory),
        DT_BLK => Some(FileType::BlockDevice),
        DT_REG => Some(FileType::Regular),
        DT_LNK => Some(FileType::Symlink),
        DT_SOCK => Some(FileType::Socket),
        _ => None,
    }
}

/// Available bytes in the current 512-byte directory block.
pub(crate) fn block_remaining(offset: u64) -> usize {
    let in_block = (offset % UFS_DIRBLKSIZ as u64) as usize;
    UFS_DIRBLKSIZ - in_block
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(buf: &mut [u8], ino: u32, reclen: u16, dtype: u8, name: &[u8]) -> usize {
        buf[0..4].copy_from_slice(&ino.to_le_bytes());
        buf[4..6].copy_from_slice(&reclen.to_le_bytes());
        buf[6] = dtype;
        buf[7] = name.len() as u8;
        buf[8..8 + name.len()].copy_from_slice(name);
        buf[8 + name.len()] = 0;
        usize::from(reclen)
    }

    #[test]
    fn dot_entry_parses() {
        let mut buf = [0u8; 512];
        let len = entry(&mut buf, 2, 12, DT_DIR, b".");
        let parsed = parse(&buf[..len], 512).expect("parse").expect("entry");
        assert_eq!(parsed.ino, 2);
        assert_eq!(parsed.name, b".");
        assert_eq!(parsed.next, 12);
    }

    #[test]
    fn named_entry_parses() {
        let mut buf = [0u8; 512];
        let len = entry(&mut buf, 7, 16, DT_REG, b"hello");
        let parsed = parse(&buf[..len], 512).expect("parse").expect("entry");
        assert_eq!(parsed.ino, 7);
        assert_eq!(parsed.dtype, DT_REG);
        assert_eq!(parsed.name, b"hello");
        assert_eq!(parsed.next, 16);
    }

    #[test]
    fn free_entry_is_reported() {
        let mut buf = [0u8; 512];
        let len = entry(&mut buf, 0, 12, 0, b"");
        let parsed = parse(&buf[..len], 512).expect("parse").expect("entry");
        assert_eq!(parsed.ino, 0);
        assert!(parsed.name.is_empty());
    }

    #[test]
    fn zero_reclen_is_rejected() {
        let mut buf = [0u8; 512];
        entry(&mut buf, 2, 0, DT_DIR, b".");
        assert!(parse(&buf[..12], 512).is_err());
    }

    #[test]
    fn misaligned_record_is_rejected() {
        let mut buf = [0u8; 512];
        let len = entry(&mut buf, 2, 13, DT_DIR, b".");
        assert!(parse(&buf[..len], 512).is_err());
    }

    #[test]
    fn record_beyond_block_is_rejected() {
        let mut buf = [0u8; 512];
        let len = entry(&mut buf, 2, 16, DT_REG, b"hello");
        assert!(parse(&buf[..len], 12).is_err());
    }

    #[test]
    fn record_smaller_than_name_is_rejected() {
        let mut buf = [0u8; 512];
        let len = entry(&mut buf, 2, 12, DT_REG, b"0123456789");
        assert!(parse(&buf[..len], 512).is_err());
    }

    #[test]
    fn name_longer_than_limit_is_rejected() {
        let mut buf = [0u8; 512];
        buf[0..4].copy_from_slice(&2u32.to_le_bytes());
        buf[4..6].copy_from_slice(&12u16.to_le_bytes());
        buf[7] = 255;
        assert!(parse(&buf[..12], 12).is_err());
    }

    #[test]
    fn nul_inside_name_is_rejected() {
        let mut buf = [0u8; 512];
        let len = entry(&mut buf, 2, 16, DT_REG, b"a\0b");
        assert!(parse(&buf[..len], 512).is_err());
    }

    #[test]
    fn missing_nul_after_name_is_rejected() {
        let mut buf = [0u8; 512];
        let len = entry(&mut buf, 2, 16, DT_REG, b"abc");
        buf[8 + 3] = b'x';
        assert!(parse(&buf[..len], 512).is_err());
    }

    #[test]
    fn maximum_name_length_parses() {
        let mut buf = [0u8; 512];
        let mut name = [b'n'; 255];
        name[254] = b'x';
        let len = entry(&mut buf, 2, 264, DT_REG, &name);
        let parsed = parse(&buf[..len], 512).expect("parse").expect("entry");
        assert_eq!(parsed.name.len(), 255);
        assert_eq!(parsed.next, 264);
    }

    #[test]
    fn dtype_mapping() {
        assert_eq!(file_type_from_dtype(DT_FIFO), Some(FileType::Fifo));
        assert_eq!(file_type_from_dtype(DT_CHR), Some(FileType::CharDevice));
        assert_eq!(file_type_from_dtype(DT_DIR), Some(FileType::Directory));
        assert_eq!(file_type_from_dtype(DT_BLK), Some(FileType::BlockDevice));
        assert_eq!(file_type_from_dtype(DT_REG), Some(FileType::Regular));
        assert_eq!(file_type_from_dtype(DT_LNK), Some(FileType::Symlink));
        assert_eq!(file_type_from_dtype(DT_SOCK), Some(FileType::Socket));
        assert_eq!(file_type_from_dtype(DT_UNKNOWN), None);
        assert_eq!(file_type_from_dtype(DT_WHT), None);
        assert_eq!(file_type_from_dtype(200), None);
    }

    #[test]
    fn block_remaining_values() {
        assert_eq!(block_remaining(0), 512);
        assert_eq!(block_remaining(100), 412);
        assert_eq!(block_remaining(512), 512);
        assert_eq!(block_remaining(1024), 512);
    }
}
