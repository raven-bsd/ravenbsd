// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ufs/ufs_readwrite.c (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notice follows.
//
// Copyright (c) 1993
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

//! File and symlink read paths.

use crate::blockio::BlockIo;
use crate::dinode::Inode;
use crate::error::UfsError;
use crate::types::FileType;
use crate::ufs::Ufs;

impl<D: BlockIo> Ufs<D> {
    /// Copy up to `buf.len()` bytes from `ip` at `offset`; holes read as zero.
    pub(crate) fn read_inode_data(
        &mut self,
        ip: &Inode,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<usize, UfsError> {
        if offset >= ip.size || buf.is_empty() {
            return Ok(0);
        }
        let available = usize::try_from(ip.size - offset).unwrap_or(usize::MAX);
        let total = buf.len().min(available);
        let bsize = u64::from(self.fs.bsize);
        let mut done = 0usize;
        while done < total {
            let pos = offset
                .checked_add(done as u64)
                .ok_or(UfsError::Corrupt("read offset overflow"))?;
            let lbn = pos / bsize;
            let block_off = (pos % bsize) as usize;
            let chunk = (total - done).min(self.fs.bsize as usize - block_off);
            if chunk == 0 {
                break;
            }
            match self.bmap(ip, lbn)? {
                None => {
                    for byte in &mut buf[done..done + chunk] {
                        *byte = 0;
                    }
                }
                Some(frag) => {
                    let data = self
                        .block_data(frag)?
                        .ok_or(UfsError::Corrupt("missing mapped block"))?;
                    let src = data
                        .get(block_off..block_off + chunk)
                        .ok_or(UfsError::Corrupt("block offset"))?;
                    buf[done..done + chunk].copy_from_slice(src);
                }
            }
            done += chunk;
        }
        Ok(done)
    }

    /// Copy a symlink target into `buf`, returning the number of bytes written.
    pub(crate) fn readlink_inode(&mut self, ip: &Inode, buf: &mut [u8]) -> Result<usize, UfsError> {
        if ip.file_type()? != FileType::Symlink {
            return Err(UfsError::Invalid);
        }
        let total = usize::try_from(ip.size)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        if ip.is_fast_symlink(self.fs.maxsymlinklen) {
            let target = ip.shortlink(self.fs.ufs2)?;
            let n = total.min(target.len());
            buf[..n].copy_from_slice(&target[..n]);
            return Ok(n);
        }
        self.read_inode_data(ip, 0, &mut buf[..total])
    }
}
