// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ufs/dinode.h (revision 36eddebcc36cb0775aa2962005b9c05419a6ff01).
// The original notices follow.
//
// Copyright (c) 2002 Networks Associates Technology, Inc.
// All rights reserved.
//
// This software was developed for the FreeBSD Project by Marshall
// Kirk McKusick and Network Associates Laboratories, the Security
// Research Division of Network Associates, Inc. under DARPA/SPAWAR
// contract N66001-01-C-8035 ("CBOSS"), as part of the DARPA CHATS
// research program
//
// Copyright (c) 1982, 1989, 1993
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

//! UFS1 and UFS2 on-disk inode decoding.

use alloc_crate::vec::Vec;

use crate::error::UfsError;
use crate::types::{FileType, Timespec};

pub(crate) const UFS_NDADDR: usize = 12;
pub(crate) const UFS_NIADDR: usize = 3;
pub(crate) const UFS1_MAXSYMLINKLEN: usize = 60;
pub(crate) const UFS2_MAXSYMLINKLEN: usize = 120;

const NSEC_LIMIT: i64 = 1_000_000_000;

/// A decoded inode. Field names follow the NetBSD `ufs1_dinode` / `ufs2_dinode` layout.
#[derive(Clone, Debug)]
pub(crate) struct Inode {
    pub(crate) raw: Vec<u8>,
    pub(crate) mode: u16,
    pub(crate) nlink: u32,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) size: u64,
    pub(crate) blocks: u64,
    pub(crate) atime: Timespec,
    pub(crate) mtime: Timespec,
    pub(crate) ctime: Timespec,
    pub(crate) flags: u32,
    pub(crate) r#gen: u32,
    pub(crate) rdev: u64,
    pub(crate) db: [u64; UFS_NDADDR],
    pub(crate) ib: [u64; UFS_NIADDR],
}

impl Inode {
    /// Decode one inode from its fixed-width on-disk bytes.
    pub(crate) fn decode(ufs2: bool, raw: &[u8]) -> Result<Self, UfsError> {
        let expected = if ufs2 { 256 } else { 128 };
        if raw.len() != expected {
            return Err(UfsError::Corrupt("inode size"));
        }
        let mode = read_u16(raw, 0)?;
        let nlink_raw = read_i16(raw, 2)?;
        if nlink_raw < 0 {
            return Err(UfsError::Corrupt("negative link count"));
        }
        let (uid, gid, size, blocks, atime, mtime, ctime, db, ib, flags, generation) = if ufs2 {
            (
                read_u32(raw, 4)?,
                read_u32(raw, 8)?,
                read_u64(raw, 16)?,
                read_u64(raw, 24)?,
                Timespec {
                    sec: read_i64(raw, 32)?,
                    nsec: read_nsec(raw, 68)?,
                },
                Timespec {
                    sec: read_i64(raw, 40)?,
                    nsec: read_nsec(raw, 64)?,
                },
                Timespec {
                    sec: read_i64(raw, 48)?,
                    nsec: read_nsec(raw, 72)?,
                },
                read_db2(raw)?,
                read_ib2(raw)?,
                read_u32(raw, 88)?,
                read_i32(raw, 80)? as u32,
            )
        } else {
            (
                read_u32(raw, 112)?,
                read_u32(raw, 116)?,
                read_u64(raw, 8)?,
                u64::from(read_u32(raw, 104)?),
                Timespec {
                    sec: i64::from(read_i32(raw, 16)?),
                    nsec: read_nsec(raw, 20)?,
                },
                Timespec {
                    sec: i64::from(read_i32(raw, 24)?),
                    nsec: read_nsec(raw, 28)?,
                },
                Timespec {
                    sec: i64::from(read_i32(raw, 32)?),
                    nsec: read_nsec(raw, 36)?,
                },
                read_db1(raw)?,
                read_ib1(raw)?,
                read_u32(raw, 100)?,
                read_i32(raw, 108)? as u32,
            )
        };
        let mut inode = Self {
            raw: raw.to_vec(),
            mode,
            nlink: nlink_raw as u32,
            uid,
            gid,
            size,
            blocks,
            atime,
            mtime,
            ctime,
            flags,
            r#gen: generation,
            rdev: 0,
            db,
            ib,
        };
        let kind = inode.file_type()?;
        if matches!(kind, FileType::CharDevice | FileType::BlockDevice) {
            inode.rdev = inode.db[0];
        }
        Ok(inode)
    }

    /// Object kind from the inode mode bits.
    pub(crate) fn file_type(&self) -> Result<FileType, UfsError> {
        file_type_from_mode(self.mode)
    }

    /// Whether the symlink target is stored in the inode itself.
    pub(crate) fn is_fast_symlink(&self, maxsymlinklen: i32) -> bool {
        self.size < maxsymlinklen as u64 || (maxsymlinklen == 0 && self.blocks == 0)
    }

    /// Fast-symlink target bytes, stored in the `di_db` area.
    pub(crate) fn shortlink(&self, ufs2: bool) -> Result<&[u8], UfsError> {
        let off = if ufs2 { 112usize } else { 40usize };
        let end = off
            .checked_add(self.size as usize)
            .ok_or(UfsError::Corrupt("symlink size"))?;
        self.raw
            .get(off..end)
            .ok_or(UfsError::Corrupt("fast symlink target"))
    }
}

pub(crate) fn file_type_from_mode(mode: u16) -> Result<FileType, UfsError> {
    match mode & 0o170000 {
        0o100000 => Ok(FileType::Regular),
        0o040000 => Ok(FileType::Directory),
        0o120000 => Ok(FileType::Symlink),
        0o020000 => Ok(FileType::CharDevice),
        0o060000 => Ok(FileType::BlockDevice),
        0o010000 => Ok(FileType::Fifo),
        0o140000 => Ok(FileType::Socket),
        _ => Err(UfsError::Corrupt("unknown inode type")),
    }
}

fn read_db1(raw: &[u8]) -> Result<[u64; UFS_NDADDR], UfsError> {
    let mut out = [0u64; UFS_NDADDR];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u64::from(read_u32(raw, 40 + i * 4)?);
    }
    Ok(out)
}

fn read_ib1(raw: &[u8]) -> Result<[u64; UFS_NIADDR], UfsError> {
    let mut out = [0u64; UFS_NIADDR];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u64::from(read_u32(raw, 88 + i * 4)?);
    }
    Ok(out)
}

fn read_db2(raw: &[u8]) -> Result<[u64; UFS_NDADDR], UfsError> {
    let mut out = [0u64; UFS_NDADDR];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = read_u64(raw, 112 + i * 8)?;
    }
    Ok(out)
}

fn read_ib2(raw: &[u8]) -> Result<[u64; UFS_NIADDR], UfsError> {
    let mut out = [0u64; UFS_NIADDR];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = read_u64(raw, 208 + i * 8)?;
    }
    Ok(out)
}

fn read_nsec(raw: &[u8], off: usize) -> Result<u32, UfsError> {
    let nsec = i64::from(read_i32(raw, off)?);
    if !(0..NSEC_LIMIT).contains(&nsec) {
        return Err(UfsError::Corrupt("timestamp nanoseconds"));
    }
    Ok(nsec as u32)
}

fn read_u16(raw: &[u8], off: usize) -> Result<u16, UfsError> {
    let bytes = raw
        .get(off..off + 2)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    let mut value = [0u8; 2];
    value.copy_from_slice(bytes);
    Ok(u16::from_le_bytes(value))
}

fn read_i16(raw: &[u8], off: usize) -> Result<i16, UfsError> {
    let bytes = raw
        .get(off..off + 2)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    let mut value = [0u8; 2];
    value.copy_from_slice(bytes);
    Ok(i16::from_le_bytes(value))
}

fn read_u32(raw: &[u8], off: usize) -> Result<u32, UfsError> {
    let bytes = raw
        .get(off..off + 4)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    let mut value = [0u8; 4];
    value.copy_from_slice(bytes);
    Ok(u32::from_le_bytes(value))
}

fn read_i32(raw: &[u8], off: usize) -> Result<i32, UfsError> {
    let bytes = raw
        .get(off..off + 4)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    let mut value = [0u8; 4];
    value.copy_from_slice(bytes);
    Ok(i32::from_le_bytes(value))
}

fn read_u64(raw: &[u8], off: usize) -> Result<u64, UfsError> {
    let bytes = raw
        .get(off..off + 8)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(value))
}

fn read_i64(raw: &[u8], off: usize) -> Result<i64, UfsError> {
    let bytes = raw
        .get(off..off + 8)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Ok(i64::from_le_bytes(value))
}

/// Encode the decoded inode fields back into its on-disk bytes, preserving other fields.
pub(crate) fn encode(ufs2: bool, raw: &mut [u8], inode: &Inode) -> Result<(), UfsError> {
    let expected = if ufs2 { 256 } else { 128 };
    if raw.len() != expected {
        return Err(UfsError::Corrupt("inode encode size"));
    }
    write_u16(raw, 0, inode.mode)?;
    write_i16(raw, 2, i16::try_from(inode.nlink).unwrap_or(i16::MAX))?;
    if ufs2 {
        write_u32(raw, 4, inode.uid)?;
        write_u32(raw, 8, inode.gid)?;
        write_u64(raw, 16, inode.size)?;
        write_u64(raw, 24, inode.blocks)?;
        write_i64(raw, 32, inode.atime.sec)?;
        write_i64(raw, 40, inode.mtime.sec)?;
        write_i64(raw, 48, inode.ctime.sec)?;
        write_i32(raw, 68, inode.atime.nsec as i32)?;
        write_i32(raw, 64, inode.mtime.nsec as i32)?;
        write_i32(raw, 72, inode.ctime.nsec as i32)?;
        write_i32(raw, 80, inode.r#gen as i32)?;
        write_u32(raw, 88, inode.flags)?;
        for (i, value) in inode.db.iter().enumerate() {
            write_u64(raw, 112 + i * 8, *value)?;
        }
        for (i, value) in inode.ib.iter().enumerate() {
            write_u64(raw, 208 + i * 8, *value)?;
        }
    } else {
        write_u32(raw, 112, inode.uid)?;
        write_u32(raw, 116, inode.gid)?;
        write_u64(raw, 8, inode.size)?;
        write_u32(raw, 104, u32::try_from(inode.blocks).unwrap_or(u32::MAX))?;
        write_i32(raw, 16, sec32(inode.atime.sec))?;
        write_i32(raw, 24, sec32(inode.mtime.sec))?;
        write_i32(raw, 32, sec32(inode.ctime.sec))?;
        write_i32(raw, 20, inode.atime.nsec as i32)?;
        write_i32(raw, 28, inode.mtime.nsec as i32)?;
        write_i32(raw, 36, inode.ctime.nsec as i32)?;
        write_i32(raw, 108, inode.r#gen as i32)?;
        write_u32(raw, 100, inode.flags)?;
        for (i, value) in inode.db.iter().enumerate() {
            write_u32(
                raw,
                40 + i * 4,
                u32::try_from(*value).map_err(|_| UfsError::Corrupt("32-bit block pointer"))?,
            )?;
        }
        for (i, value) in inode.ib.iter().enumerate() {
            write_u32(
                raw,
                88 + i * 4,
                u32::try_from(*value).map_err(|_| UfsError::Corrupt("32-bit indirect pointer"))?,
            )?;
        }
    }
    Ok(())
}

fn sec32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

fn write_u16(raw: &mut [u8], off: usize, value: u16) -> Result<(), UfsError> {
    let dst = raw
        .get_mut(off..off + 2)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_i16(raw: &mut [u8], off: usize, value: i16) -> Result<(), UfsError> {
    let dst = raw
        .get_mut(off..off + 2)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u32(raw: &mut [u8], off: usize, value: u32) -> Result<(), UfsError> {
    let dst = raw
        .get_mut(off..off + 4)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_i32(raw: &mut [u8], off: usize, value: i32) -> Result<(), UfsError> {
    let dst = raw
        .get_mut(off..off + 4)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u64(raw: &mut [u8], off: usize, value: u64) -> Result<(), UfsError> {
    let dst = raw
        .get_mut(off..off + 8)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_i64(raw: &mut [u8], off: usize, value: i64) -> Result<(), UfsError> {
    let dst = raw
        .get_mut(off..off + 8)
        .ok_or(UfsError::Corrupt("truncated inode"))?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_u16(buf: &mut [u8], off: usize, value: u16) {
        buf[off..off + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_i16(buf: &mut [u8], off: usize, value: i16) {
        buf[off..off + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(buf: &mut [u8], off: usize, value: u32) {
        buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_i32(buf: &mut [u8], off: usize, value: i32) {
        buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(buf: &mut [u8], off: usize, value: u64) {
        buf[off..off + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn put_i64(buf: &mut [u8], off: usize, value: i64) {
        buf[off..off + 8].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn ufs1_fields_decode() {
        let mut raw = [0u8; 128];
        put_u16(&mut raw, 0, 0o100644);
        put_i16(&mut raw, 2, 2);
        put_u64(&mut raw, 8, 123);
        put_i32(&mut raw, 16, 1);
        put_i32(&mut raw, 20, 2);
        put_i32(&mut raw, 24, 3);
        put_i32(&mut raw, 28, 4);
        put_i32(&mut raw, 32, 5);
        put_i32(&mut raw, 36, 6);
        put_u32(&mut raw, 40, 42);
        put_u32(&mut raw, 88, 43);
        put_u32(&mut raw, 100, 0x10);
        put_u32(&mut raw, 104, 9);
        put_i32(&mut raw, 108, 7);
        put_u32(&mut raw, 112, 1000);
        put_u32(&mut raw, 116, 100);
        let inode = Inode::decode(false, &raw).expect("decode");
        assert_eq!(inode.mode, 0o100644);
        assert_eq!(inode.nlink, 2);
        assert_eq!(inode.uid, 1000);
        assert_eq!(inode.gid, 100);
        assert_eq!(inode.size, 123);
        assert_eq!(inode.blocks, 9);
        assert_eq!(inode.atime, Timespec { sec: 1, nsec: 2 });
        assert_eq!(inode.mtime, Timespec { sec: 3, nsec: 4 });
        assert_eq!(inode.ctime, Timespec { sec: 5, nsec: 6 });
        assert_eq!(inode.db[0], 42);
        assert_eq!(inode.ib[0], 43);
        assert_eq!(inode.flags, 0x10);
        assert_eq!(inode.r#gen, 7);
        assert_eq!(inode.file_type().expect("type"), FileType::Regular);
    }

    #[test]
    fn ufs2_fields_decode() {
        let mut raw = [0u8; 256];
        put_u16(&mut raw, 0, 0o040755);
        put_i16(&mut raw, 2, 3);
        put_u32(&mut raw, 4, 1000);
        put_u32(&mut raw, 8, 100);
        put_u64(&mut raw, 16, 4096);
        put_u64(&mut raw, 24, 8);
        put_i64(&mut raw, 32, 10);
        put_i32(&mut raw, 68, 11);
        put_i64(&mut raw, 40, 12);
        put_i32(&mut raw, 64, 13);
        put_i64(&mut raw, 48, 14);
        put_i32(&mut raw, 72, 15);
        put_u64(&mut raw, 112, 99);
        put_u64(&mut raw, 208, 100);
        put_u32(&mut raw, 88, 0x20);
        put_i32(&mut raw, 80, 9);
        let inode = Inode::decode(true, &raw).expect("decode");
        assert_eq!(inode.file_type().expect("type"), FileType::Directory);
        assert_eq!(inode.nlink, 3);
        assert_eq!(inode.uid, 1000);
        assert_eq!(inode.gid, 100);
        assert_eq!(inode.size, 4096);
        assert_eq!(inode.blocks, 8);
        assert_eq!(inode.atime, Timespec { sec: 10, nsec: 11 });
        assert_eq!(inode.mtime, Timespec { sec: 12, nsec: 13 });
        assert_eq!(inode.ctime, Timespec { sec: 14, nsec: 15 });
        assert_eq!(inode.db[0], 99);
        assert_eq!(inode.ib[0], 100);
        assert_eq!(inode.flags, 0x20);
        assert_eq!(inode.r#gen, 9);
    }

    #[test]
    fn all_file_types_map() {
        let cases = [
            (0o100000, FileType::Regular),
            (0o040000, FileType::Directory),
            (0o120000, FileType::Symlink),
            (0o020000, FileType::CharDevice),
            (0o060000, FileType::BlockDevice),
            (0o010000, FileType::Fifo),
            (0o140000, FileType::Socket),
        ];
        for (mode, expected) in cases {
            assert_eq!(file_type_from_mode(mode).expect("type"), expected);
        }
    }

    #[test]
    fn unknown_file_type_is_rejected() {
        assert!(file_type_from_mode(0o150000).is_err());
    }

    #[test]
    fn negative_link_count_is_rejected() {
        let mut raw = [0u8; 128];
        put_u16(&mut raw, 0, 0o100644);
        put_i16(&mut raw, 2, -1);
        assert!(Inode::decode(false, &raw).is_err());
    }

    #[test]
    fn bad_nanoseconds_are_rejected() {
        let mut raw = [0u8; 128];
        put_u16(&mut raw, 0, 0o100644);
        put_i16(&mut raw, 2, 1);
        put_i32(&mut raw, 20, 1_000_000_000);
        assert!(Inode::decode(false, &raw).is_err());
    }

    #[test]
    fn fast_symlink_target_is_in_inode() {
        let mut raw = [0u8; 128];
        put_u16(&mut raw, 0, 0o120777);
        put_i16(&mut raw, 2, 1);
        put_u64(&mut raw, 8, 11);
        raw[40..51].copy_from_slice(b"fast-target");
        let inode = Inode::decode(false, &raw).expect("decode");
        assert!(inode.is_fast_symlink(60));
        assert_eq!(inode.shortlink(false).expect("target"), b"fast-target");
    }

    #[test]
    fn slow_symlink_is_not_in_inode() {
        let mut raw = [0u8; 128];
        put_u16(&mut raw, 0, 0o120777);
        put_i16(&mut raw, 2, 1);
        put_u64(&mut raw, 8, 200);
        put_u32(&mut raw, 104, 1);
        let inode = Inode::decode(false, &raw).expect("decode");
        assert!(!inode.is_fast_symlink(60));
    }

    #[test]
    fn device_rdev_comes_from_first_pointer() {
        let mut raw = [0u8; 128];
        put_u16(&mut raw, 0, 0o020644);
        put_i16(&mut raw, 2, 1);
        put_u32(&mut raw, 40, 0x1234);
        let inode = Inode::decode(false, &raw).expect("decode");
        assert_eq!(inode.file_type().expect("type"), FileType::CharDevice);
        assert_eq!(inode.rdev, 0x1234);
    }

    #[test]
    fn truncated_inode_is_rejected() {
        let raw = [0u8; 100];
        assert!(Inode::decode(false, &raw).is_err());
        assert!(Inode::decode(true, &raw).is_err());
    }
}
