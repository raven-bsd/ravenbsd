// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ufs/ufs_lookup.c (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notice follows.
//
// Copyright (c) 1989, 1993
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

//! Directory lookup and iteration with cookie offsets.

use crate::blockio::BlockIo;
use crate::dinode::Inode;
use crate::dir::{self, DT_WHT, RawEntry};
use crate::error::UfsError;
use crate::fs::{FFS_MAXNAMLEN, UFS_DIRBLKSIZ};
use crate::types::{DirEntry, FileType, Ino};
use crate::ufs::Ufs;

impl<D: BlockIo> Ufs<D> {
    /// Read the record at byte offset `offset` in a directory.
    pub(crate) fn read_dir_entry(
        &mut self,
        ip: &Inode,
        offset: u64,
    ) -> Result<Option<RawEntry>, UfsError> {
        if offset >= ip.size {
            return Ok(None);
        }
        let remaining = usize::try_from(ip.size - offset).unwrap_or(usize::MAX);
        let want = remaining.min(dir::block_remaining(offset));
        if want < 8 {
            return Err(UfsError::Corrupt("truncated directory"));
        }
        let mut buf = [0u8; UFS_DIRBLKSIZ];
        let got = self.read_inode_data(ip, offset, &mut buf[..want])?;
        if got == 0 {
            return Ok(None);
        }
        dir::parse(&buf[..got], want)
    }

    /// Linear lookup of `name` in directory `dir`.
    pub(crate) fn lookup_in_dir(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, UfsError> {
        if name.len() > FFS_MAXNAMLEN {
            return Err(UfsError::NameTooLong);
        }
        if name.is_empty() {
            return Err(UfsError::NotFound);
        }
        if name.contains(&0) || name.contains(&b'/') {
            return Err(UfsError::Invalid);
        }
        let ip = self.load_inode(dir)?;
        if ip.file_type()? != FileType::Directory {
            return Err(UfsError::NotDir);
        }
        let mut offset = 0u64;
        while let Some(entry) = self.read_dir_entry(&ip, offset)? {
            let next = offset
                .checked_add(entry.next as u64)
                .ok_or(UfsError::Corrupt("directory offset overflow"))?;
            if next <= offset {
                return Err(UfsError::Corrupt("directory loop"));
            }
            if entry.ino != 0
                && entry.ino != 1
                && entry.dtype != DT_WHT
                && entry.name.as_slice() == name
            {
                return Ok(entry.ino);
            }
            offset = next;
        }
        Err(UfsError::NotFound)
    }

    /// Iterate a directory from a byte-offset cookie.
    pub(crate) fn readdir_from(
        &mut self,
        dir: Ino,
        cookie: u64,
        f: &mut dyn FnMut(&DirEntry, u64) -> bool,
    ) -> Result<(), UfsError> {
        let ip = self.load_inode(dir)?;
        if ip.file_type()? != FileType::Directory {
            return Err(UfsError::NotDir);
        }
        if cookie > ip.size {
            return Err(UfsError::Invalid);
        }
        let mut offset = cookie;
        while let Some(entry) = self.read_dir_entry(&ip, offset)? {
            let next = offset
                .checked_add(entry.next as u64)
                .ok_or(UfsError::Corrupt("directory offset overflow"))?;
            if next <= offset {
                return Err(UfsError::Corrupt("directory loop"));
            }
            if entry.ino != 0 && entry.ino != 1 && entry.dtype != DT_WHT {
                let kind = match dir::file_type_from_dtype(entry.dtype) {
                    Some(kind) => kind,
                    None => self.load_inode(entry.ino)?.file_type()?,
                };
                let item = DirEntry {
                    ino: entry.ino,
                    kind,
                    name: entry.name,
                };
                if !f(&item, next) {
                    return Ok(());
                }
            }
            offset = next;
        }
        Ok(())
    }
}
