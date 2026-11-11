// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ufs/ufs_rename.c (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notice follows.
//
// Copyright (c) 2012 The NetBSD Foundation, Inc.
// All rights reserved.
//
// This code is derived from software contributed to The NetBSD Foundation
// by Taylor R Campbell.
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

//! `rename` for UFS directories, including replacement and reparenting.

use crate::blockio::BlockIo;
use crate::dir::DT_DIR;
use crate::dirwrite::{self, dtype};
use crate::error::UfsError;
use crate::fs::UFS_ROOTINO;
use crate::types::{FileType, Ino, Timespec};
use crate::ufs::Ufs;

/// NetBSD `LINK_MAX` for the 16-bit on-disk link count.
const LINK_MAX: u32 = 32767;

impl<D: BlockIo> Ufs<D> {
    fn rename_dir(&mut self, dir: Ino) -> Result<crate::dinode::Inode, UfsError> {
        let ip = self.load_inode(dir)?;
        if ip.file_type()? != FileType::Directory {
            return Err(UfsError::NotDir);
        }
        Ok(ip)
    }

    fn rename_ctime(&mut self, inode: &mut crate::dinode::Inode) {
        let now = self.next_time();
        inode.ctime = Timespec { sec: now, nsec: 0 };
    }

    fn is_descendant(&mut self, start: Ino, ancestor: Ino) -> Result<bool, UfsError> {
        let mut current = start;
        let limit = u64::from(self.fs.ncg)
            .saturating_mul(u64::from(self.fs.ipg))
            .saturating_add(1);
        for _ in 0..limit {
            if current == ancestor {
                return Ok(true);
            }
            if current == UFS_ROOTINO {
                return Ok(false);
            }
            let ip = self.load_inode(current)?;
            if ip.file_type()? != FileType::Directory {
                return Err(UfsError::NotDir);
            }
            let parent = self
                .lookup_entry(current, b"..")?
                .ok_or(UfsError::Corrupt("directory missing dotdot"))?;
            if parent.entry.ino == current {
                return Ok(false);
            }
            current = parent.entry.ino;
        }
        Err(UfsError::Corrupt("directory parent loop"))
    }

    /// Rename `old_name` in `old_dir` to `new_name` in `new_dir`.
    ///
    /// Replacement follows FFS: the target entry is rewritten to the source inode before
    /// the source entry is removed, so a crash never leaves a dangling directory entry.
    pub fn rename(
        &mut self,
        old_dir: Ino,
        old_name: &[u8],
        new_dir: Ino,
        new_name: &[u8],
    ) -> Result<(), UfsError> {
        self.ensure_writable()?;
        dirwrite::validate_new_name(old_name)?;
        dirwrite::validate_new_name(new_name)?;
        let mut new_parent = self.rename_dir(new_dir)?;
        self.rename_dir(old_dir)?;

        let source_entry = self
            .lookup_entry(old_dir, old_name)?
            .ok_or(UfsError::NotFound)?;
        let source_ino = source_entry.entry.ino;
        let mut source = self.load_inode(source_ino)?;
        let source_kind = source.file_type()?;
        if source.nlink == 0 {
            return Err(UfsError::Corrupt("link count"));
        }

        if old_dir == new_dir && old_name == new_name {
            return Ok(());
        }

        let target = self.lookup_entry(new_dir, new_name)?;
        if let Some(entry) = &target
            && entry.entry.ino == source_ino
        {
            return Ok(());
        }

        if source_kind == FileType::Directory && self.is_descendant(new_dir, source_ino)? {
            return Err(UfsError::Invalid);
        }

        if let Some(entry) = &target {
            let target_kind = self.load_inode(entry.entry.ino)?.file_type()?;
            if (source_kind == FileType::Directory) != (target_kind == FileType::Directory) {
                return Err(if source_kind == FileType::Directory {
                    UfsError::NotDir
                } else {
                    UfsError::IsDir
                });
            }
            if source_kind == FileType::Directory {
                let target_dir = self.load_inode(entry.entry.ino)?;
                if target_dir.nlink != 2
                    || !self.dir_is_empty(entry.entry.ino, &target_dir, new_dir)?
                {
                    return Err(UfsError::NotEmpty);
                }
            }
        }

        if source_kind == FileType::Directory && old_dir != new_dir && new_parent.nlink >= LINK_MAX
        {
            return Err(UfsError::Invalid);
        }
        if source.nlink >= LINK_MAX {
            return Err(UfsError::Invalid);
        }

        // A temporary extra link keeps the inode alive while two entries may reference it.
        source.nlink += 1;
        self.rename_ctime(&mut source);
        self.store_inode(source_ino, &source)?;
        let source_type = dtype(source_kind);

        if let Some(entry) = &target {
            let target_ino = entry.entry.ino;
            let mut target_inode = self.load_inode(target_ino)?;
            if target_inode.nlink == 0 {
                return Err(UfsError::Corrupt("link count"));
            }
            self.dir_rewrite_entry(
                new_dir,
                &mut new_parent,
                entry.offset,
                source_ino,
                source_type,
            )?;
            if source_kind == FileType::Directory {
                // The target directory was verified empty; its "." reference is quashed.
                if old_dir == new_dir {
                    new_parent.nlink = new_parent
                        .nlink
                        .checked_sub(1)
                        .ok_or(UfsError::Corrupt("link count"))?;
                }
                target_inode.nlink = 0;
                self.store_inode(target_ino, &target_inode)?;
                self.truncate_inode(target_ino, &mut target_inode, 0)?;
                self.free_inode(target_ino, target_inode.mode)?;
                if old_dir == new_dir {
                    self.store_inode(new_dir, &new_parent)?;
                }
            } else {
                target_inode.nlink = target_inode
                    .nlink
                    .checked_sub(1)
                    .ok_or(UfsError::Corrupt("link count"))?;
                self.rename_ctime(&mut target_inode);
                if target_inode.nlink == 0 {
                    let target_kind = target_inode.file_type()?;
                    self.store_inode(target_ino, &target_inode)?;
                    if matches!(target_kind, FileType::Regular | FileType::Symlink) {
                        self.truncate(target_ino, 0)?;
                    }
                    self.free_inode(target_ino, target_inode.mode)?;
                } else {
                    self.store_inode(target_ino, &target_inode)?;
                }
            }
        } else {
            self.dir_insert(new_dir, &mut new_parent, new_name, source_ino, source_type)?;
        }

        if source_kind == FileType::Directory && old_dir != new_dir {
            if target.is_none() {
                new_parent.nlink = new_parent
                    .nlink
                    .checked_add(1)
                    .ok_or(UfsError::Corrupt("link count"))?;
                self.store_inode(new_dir, &new_parent)?;
            }
            let dotdot = self
                .lookup_entry(source_ino, b"..")?
                .ok_or(UfsError::Corrupt("directory missing dotdot"))?;
            self.dir_rewrite_entry(source_ino, &mut source, dotdot.offset, new_dir, DT_DIR)?;
            let mut old_parent_latest = self.load_inode(old_dir)?;
            old_parent_latest.nlink = old_parent_latest
                .nlink
                .checked_sub(1)
                .ok_or(UfsError::Corrupt("link count"))?;
            self.store_inode(old_dir, &old_parent_latest)?;
        }

        let mut old_parent_latest = self.load_inode(old_dir)?;
        let source_entry = self
            .lookup_entry(old_dir, old_name)?
            .ok_or(UfsError::Corrupt("source entry vanished"))?;
        self.dir_remove_entry(old_dir, &mut old_parent_latest, &source_entry)?;
        let mut source_latest = self.load_inode(source_ino)?;
        source_latest.nlink = source_latest
            .nlink
            .checked_sub(1)
            .ok_or(UfsError::Corrupt("link count"))?;
        self.rename_ctime(&mut source_latest);
        self.store_inode(source_ino, &source_latest)?;
        Ok(())
    }
}
