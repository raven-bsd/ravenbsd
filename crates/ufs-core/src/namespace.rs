// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ufs/ufs_vnops.c (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notices follow.
//
// Copyright (c) 2008, 2020 The NetBSD Foundation, Inc.
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
//
// Copyright (c) 1982, 1986, 1989, 1993, 1995
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

//! POSIX namespace operations built on the directory entry primitives.

use crate::blockio::BlockIo;
use crate::dinode::Inode;
use crate::dirwrite::{self, dtype};
use crate::error::UfsError;
use crate::types::{FileType, Ino, Timespec};
use crate::ufs::Ufs;

/// NetBSD `LINK_MAX` for the 16-bit on-disk link count.
const LINK_MAX: u32 = 32767;

impl<D: BlockIo> Ufs<D> {
    fn directory_inode(&mut self, dir: Ino) -> Result<Inode, UfsError> {
        let ip = self.load_inode(dir)?;
        if ip.file_type()? != FileType::Directory {
            return Err(UfsError::NotDir);
        }
        Ok(ip)
    }

    fn new_inode_times(&mut self, inode: &mut Inode) {
        let now = self.next_time();
        inode.atime = Timespec { sec: now, nsec: 0 };
        inode.mtime = inode.atime;
        inode.ctime = inode.atime;
    }

    fn touch_ctime(&mut self, inode: &mut Inode) {
        let now = self.next_time();
        inode.ctime = Timespec { sec: now, nsec: 0 };
    }

    fn discard_inode(&mut self, ino: Ino, mut inode: Inode) -> Result<(), UfsError> {
        if inode.file_type()? == FileType::Directory {
            self.truncate_inode(ino, &mut inode, 0)?;
        } else {
            self.truncate(ino, 0)?;
        }
        self.free_inode(ino, inode.mode)
    }

    /// Create an empty regular file and enter it in `dir`.
    pub fn create(
        &mut self,
        dir: Ino,
        name: &[u8],
        mode: u16,
        uid: u32,
        gid: u32,
    ) -> Result<Ino, UfsError> {
        self.ensure_writable()?;
        dirwrite::validate_new_name(name)?;
        let mut dp = self.directory_inode(dir)?;
        if self.lookup_entry(dir, name)?.is_some() {
            return Err(UfsError::Exists);
        }
        let ino = self.alloc_inode(dir, 0o100000 | (mode & 0o7777))?;
        let mut inode = self.load_inode(ino)?;
        inode.nlink = 1;
        inode.uid = uid;
        inode.gid = gid;
        self.new_inode_times(&mut inode);
        self.store_inode(ino, &inode)?;
        if let Err(error) = self.dir_insert(dir, &mut dp, name, ino, dtype(FileType::Regular)) {
            let _ = self.discard_inode(ino, inode);
            return Err(error);
        }
        Ok(ino)
    }

    /// Create a directory with `.` and `..`, and enter it in `dir`.
    pub fn mkdir(
        &mut self,
        dir: Ino,
        name: &[u8],
        mode: u16,
        uid: u32,
        gid: u32,
    ) -> Result<Ino, UfsError> {
        self.ensure_writable()?;
        dirwrite::validate_new_name(name)?;
        let mut dp = self.directory_inode(dir)?;
        if self.lookup_entry(dir, name)?.is_some() {
            return Err(UfsError::Exists);
        }
        if dp.nlink >= LINK_MAX {
            return Err(UfsError::Invalid);
        }
        let ino = self.alloc_inode(dir, 0o040000 | (mode & 0o7777))?;
        let mut inode = self.load_inode(ino)?;
        inode.nlink = 2;
        inode.uid = uid;
        inode.gid = gid;
        self.new_inode_times(&mut inode);
        self.store_inode(ino, &inode)?;

        // The parent link count is made durable before the child reference appears.
        dp.nlink += 1;
        self.store_inode(dir, &dp)?;

        let block = dirwrite::dot_entries(ino, dir)?;
        if let Err(error) = self.write_dir_block(ino, &mut inode, 0, &block) {
            dp.nlink -= 1;
            let _ = self.store_inode(dir, &dp);
            let _ = self.discard_inode(ino, inode);
            return Err(error);
        }
        inode.size = crate::fs::UFS_DIRBLKSIZ as u64;
        self.store_inode(ino, &inode)?;

        if let Err(error) = self.dir_insert(dir, &mut dp, name, ino, dtype(FileType::Directory)) {
            dp.nlink -= 1;
            let _ = self.store_inode(dir, &dp);
            let _ = self.discard_inode(ino, inode);
            return Err(error);
        }
        Ok(ino)
    }

    /// Remove a non-directory entry. The last link frees the inode and its data.
    pub fn unlink(&mut self, dir: Ino, name: &[u8]) -> Result<(), UfsError> {
        self.ensure_writable()?;
        dirwrite::validate_new_name(name)?;
        let mut dp = self.directory_inode(dir)?;
        let located = self.lookup_entry(dir, name)?.ok_or(UfsError::NotFound)?;
        let target = located.entry.ino;
        let mut inode = self.load_inode(target)?;
        let kind = inode.file_type()?;
        if kind == FileType::Directory {
            return Err(UfsError::IsDir);
        }
        if inode.nlink == 0 {
            return Err(UfsError::Corrupt("link count"));
        }
        self.dir_remove_entry(dir, &mut dp, &located)?;
        inode.nlink = inode
            .nlink
            .checked_sub(1)
            .ok_or(UfsError::Corrupt("link count"))?;
        self.touch_ctime(&mut inode);
        self.store_inode(target, &inode)?;
        if inode.nlink == 0 {
            if matches!(kind, FileType::Regular | FileType::Symlink) {
                self.truncate(target, 0)?;
            }
            self.free_inode(target, inode.mode)?;
        }
        Ok(())
    }

    /// Remove an empty directory. `NotEmpty` is returned while any entry remains.
    pub fn rmdir(&mut self, dir: Ino, name: &[u8]) -> Result<(), UfsError> {
        self.ensure_writable()?;
        dirwrite::validate_new_name(name)?;
        let mut dp = self.directory_inode(dir)?;
        let located = self.lookup_entry(dir, name)?.ok_or(UfsError::NotFound)?;
        let target = located.entry.ino;
        if target == dir {
            return Err(UfsError::Invalid);
        }
        let mut inode = self.load_inode(target)?;
        if inode.file_type()? != FileType::Directory {
            return Err(UfsError::NotDir);
        }
        if inode.nlink != 2 || !self.dir_is_empty(target, &inode, dir)? {
            return Err(UfsError::NotEmpty);
        }
        self.dir_remove_entry(dir, &mut dp, &located)?;
        dp.nlink = dp
            .nlink
            .checked_sub(1)
            .ok_or(UfsError::Corrupt("link count"))?;
        self.store_inode(dir, &dp)?;
        inode.nlink = 0;
        self.store_inode(target, &inode)?;
        self.truncate_inode(target, &mut inode, 0)?;
        self.free_inode(target, inode.mode)
    }

    /// Add a hard link named `name` in `dir` pointing at `target`.
    pub fn link(&mut self, dir: Ino, name: &[u8], target: Ino) -> Result<(), UfsError> {
        self.ensure_writable()?;
        dirwrite::validate_new_name(name)?;
        let mut dp = self.directory_inode(dir)?;
        if self.lookup_entry(dir, name)?.is_some() {
            return Err(UfsError::Exists);
        }
        if target < 2 || target >= self.fs.ncg.saturating_mul(self.fs.ipg) {
            return Err(UfsError::NotFound);
        }
        let mut inode = match self.load_inode(target) {
            Ok(inode) => inode,
            // A freed inode is zeroed and no longer decodes as an object.
            Err(UfsError::Corrupt(_)) => return Err(UfsError::NotFound),
            Err(error) => return Err(error),
        };
        let kind = inode.file_type()?;
        if kind == FileType::Directory {
            return Err(UfsError::Invalid);
        }
        if inode.nlink == 0 {
            return Err(UfsError::NotFound);
        }
        if inode.nlink >= LINK_MAX {
            return Err(UfsError::Invalid);
        }
        inode.nlink += 1;
        self.touch_ctime(&mut inode);
        self.store_inode(target, &inode)?;
        if let Err(error) = self.dir_insert(dir, &mut dp, name, target, dtype(kind)) {
            inode.nlink -= 1;
            let _ = self.store_inode(target, &inode);
            return Err(error);
        }
        Ok(())
    }

    /// Create a symbolic link and enter it in `dir`.
    pub fn symlink(
        &mut self,
        dir: Ino,
        name: &[u8],
        target: &[u8],
        uid: u32,
        gid: u32,
    ) -> Result<Ino, UfsError> {
        self.ensure_writable()?;
        dirwrite::validate_new_name(name)?;
        if target.contains(&0) || target.len() > self.fs.bsize as usize {
            return Err(UfsError::Invalid);
        }
        let mut dp = self.directory_inode(dir)?;
        if self.lookup_entry(dir, name)?.is_some() {
            return Err(UfsError::Exists);
        }
        let ino = self.alloc_inode(dir, 0o120777)?;
        let mut inode = self.load_inode(ino)?;
        inode.nlink = 1;
        inode.uid = uid;
        inode.gid = gid;
        self.new_inode_times(&mut inode);
        self.store_inode(ino, &inode)?;
        if let Err(error) = self.dir_insert(dir, &mut dp, name, ino, dtype(FileType::Symlink)) {
            let _ = self.discard_inode(ino, inode);
            return Err(error);
        }
        self.write(ino, 0, target)?;
        Ok(ino)
    }
}
