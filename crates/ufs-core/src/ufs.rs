// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Attribute/symlink behaviour ported from NetBSD sys/ufs/ufs/ufs_vnops.c
// (revision 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notices follow.
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

//! Mounted filesystem, caches and the public read-only operations.

use alloc::vec;
use alloc::vec::Vec;

use crate::blockio::BlockIo;
use crate::dinode::Inode;
use crate::error::UfsError;
use crate::ffs_vfsops;
use crate::fs::{Superblock, UFS_ROOTINO};
use crate::inode as inode_load;
use crate::types::{Attr, DirEntry, FileType, Ino, MountOptions, StatFs};
use crate::ufs_bmap;

const INODE_CACHE_SIZE: usize = 64;
const BLOCK_CACHE_SIZE: usize = 32;

#[derive(Clone, Debug)]
struct CachedInode {
    ino: Ino,
    inode: Inode,
}

#[derive(Debug)]
struct CachedBlock {
    frag: u64,
    data: Vec<u8>,
}

/// A mounted UFS1/UFS2 filesystem.
///
/// The type owns the [`BlockIo`] device. All operations take `&mut self`; there is no global
/// state, no locking and no background thread. Drop the value through [`Ufs::into_inner`] to get
/// the device back.
pub struct Ufs<D: BlockIo> {
    disk: D,
    pub(crate) fs: Superblock,
    inodes: Vec<CachedInode>,
    blocks: Vec<CachedBlock>,
}

impl<D: BlockIo> Ufs<D> {
    /// Mount an image. Only read-only mounts are supported in this ticket.
    ///
    /// The superblock is probed at the upstream locations (`SBLOCK_UFS2`, `SBLOCK_UFS1`,
    /// `SBLOCK_FLOPPY`, `SBLOCK_PIGGY`) and validated before any inode is read.
    pub fn mount(mut disk: D, opts: MountOptions) -> Result<Self, UfsError> {
        if !opts.read_only {
            return Err(UfsError::Unsupported("read-write mount"));
        }
        let fs = ffs_vfsops::probe(&mut disk)?;
        Ok(Self {
            disk,
            fs,
            inodes: Vec::new(),
            blocks: Vec::new(),
        })
    }

    /// Root inode number (always [`UFS_ROOTINO`]).
    pub fn root(&self) -> Ino {
        UFS_ROOTINO
    }

    /// Look up one component in a directory.
    pub fn lookup(&mut self, dir: Ino, name: &[u8]) -> Result<Ino, UfsError> {
        self.lookup_in_dir(dir, name)
    }

    /// Read inode attributes.
    pub fn getattr(&mut self, ino: Ino) -> Result<Attr, UfsError> {
        let inode = self.load_inode(ino)?;
        let kind = inode.file_type()?;
        let rdev = if matches!(kind, FileType::CharDevice | FileType::BlockDevice) {
            inode.rdev
        } else {
            0
        };
        Ok(Attr {
            ino,
            kind,
            mode: inode.mode & 0o7777,
            nlink: inode.nlink,
            uid: inode.uid,
            gid: inode.gid,
            size: inode.size,
            blocks: inode.blocks,
            atime: inode.atime,
            mtime: inode.mtime,
            ctime: inode.ctime,
            rdev,
            r#gen: inode.r#gen,
            flags: inode.flags,
        })
    }

    /// Read a regular file at `offset`; holes read as zeros and EOF is a short read.
    pub fn read(&mut self, ino: Ino, offset: u64, buf: &mut [u8]) -> Result<usize, UfsError> {
        let inode = self.load_inode(ino)?;
        match inode.file_type()? {
            FileType::Regular => self.read_inode_data(&inode, offset, buf),
            FileType::Directory => Err(UfsError::IsDir),
            _ => Err(UfsError::Invalid),
        }
    }

    /// Iterate a directory from a byte-offset cookie.
    ///
    /// `f` receives each live entry and the cookie to pass to the next call. Returning `false`
    /// stops the iteration.
    pub fn readdir(
        &mut self,
        dir: Ino,
        cookie: u64,
        f: &mut dyn FnMut(&DirEntry, u64) -> bool,
    ) -> Result<(), UfsError> {
        self.readdir_from(dir, cookie, f)
    }

    /// Read a symlink target into `buf`; truncation is not an error.
    pub fn readlink(&mut self, ino: Ino, buf: &mut [u8]) -> Result<usize, UfsError> {
        let inode = self.load_inode(ino)?;
        self.readlink_inode(&inode, buf)
    }

    /// Filesystem statistics.
    pub fn statfs(&self) -> StatFs {
        ffs_vfsops::statfs(&self.fs)
    }

    /// Whether the filesystem was marked clean when mounted.
    ///
    /// A dirty flag is accepted on a read-only mount; this method lets the caller note it.
    pub fn is_clean(&self) -> bool {
        self.fs.is_clean()
    }

    /// Return the block device.
    pub fn into_inner(self) -> D {
        self.disk
    }

    /// Load and cache one inode.
    pub(crate) fn load_inode(&mut self, ino: Ino) -> Result<Inode, UfsError> {
        if ino < 2 {
            return Err(UfsError::NotFound);
        }
        if let Some(entry) = self.inodes.iter().find(|entry| entry.ino == ino) {
            return Ok(entry.inode.clone());
        }
        let (fsba, slot) = inode_load::location(&self.fs, ino)?;
        let fs = self.fs.clone();
        let block = self
            .block_data(fsba)?
            .ok_or(UfsError::Corrupt("inode block is a hole"))?;
        let inode = inode_load::decode_from_block(&fs, block, slot, ino)?;
        if self.inodes.len() >= INODE_CACHE_SIZE {
            self.inodes.remove(0);
        }
        self.inodes.push(CachedInode {
            ino,
            inode: inode.clone(),
        });
        Ok(inode)
    }

    /// Fetch a filesystem block, using the small write-through cache.
    pub(crate) fn block_data(&mut self, frag: u64) -> Result<Option<&[u8]>, UfsError> {
        if frag == 0 {
            return Ok(None);
        }
        let mut hit = None;
        for (index, entry) in self.blocks.iter().enumerate() {
            if entry.frag == frag {
                hit = Some(index);
                break;
            }
        }
        if let Some(index) = hit {
            return match self.blocks.get(index) {
                Some(entry) => Ok(Some(entry.data.as_slice())),
                None => Err(UfsError::Corrupt("block cache")),
            };
        }
        let offset = self.fs.frag_offset(frag)?;
        let end = offset
            .checked_add(u64::from(self.fs.bsize))
            .ok_or(UfsError::Corrupt("block offset overflow"))?;
        if end > self.disk.size_bytes() {
            return Err(UfsError::Corrupt("block beyond device"));
        }
        let mut data = vec![0u8; self.fs.bsize as usize];
        self.disk
            .read_at(offset, &mut data)
            .map_err(|_| UfsError::Io)?;
        if self.blocks.len() >= BLOCK_CACHE_SIZE {
            self.blocks.remove(0);
        }
        self.blocks.push(CachedBlock { frag, data });
        match self.blocks.last() {
            Some(entry) => Ok(Some(entry.data.as_slice())),
            None => Err(UfsError::Corrupt("block cache")),
        }
    }

    /// Map a logical block to a fragment address; `None` is a hole.
    pub(crate) fn bmap(&mut self, ip: &Inode, lbn: u64) -> Result<Option<u64>, UfsError> {
        let plan = ufs_bmap::plan(lbn, self.fs.nindir())?;
        let mut read_pointer = |frag: u64, index: u32| self.read_pointer(frag, index);
        ufs_bmap::follow(ip, plan, &mut read_pointer)
    }

    fn read_pointer(&mut self, frag: u64, index: u32) -> Result<u64, UfsError> {
        let width = self.fs.ptr_size();
        let ufs2 = self.fs.ufs2;
        let data = self
            .block_data(frag)?
            .ok_or(UfsError::Corrupt("indirect block is a hole"))?;
        let offset = (index as usize)
            .checked_mul(width)
            .ok_or(UfsError::Corrupt("pointer offset overflow"))?;
        let bytes = data
            .get(offset..offset + width)
            .ok_or(UfsError::Corrupt("pointer outside indirect block"))?;
        if ufs2 {
            let mut value = [0u8; 8];
            value.copy_from_slice(bytes);
            Ok(u64::from_le_bytes(value))
        } else {
            let mut value = [0u8; 4];
            value.copy_from_slice(bytes);
            Ok(u64::from(u32::from_le_bytes(value)))
        }
    }
}

impl<D: BlockIo> core::fmt::Debug for Ufs<D> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Ufs")
            .field("ufs2", &self.fs.ufs2)
            .field("bsize", &self.fs.bsize)
            .field("fsize", &self.fs.fsize)
            .finish_non_exhaustive()
    }
}
