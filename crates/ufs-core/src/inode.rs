// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD sys/ufs/ffs/ffs_subr.c (revision
// 36eddebcc36cb0775aa2962005b9c05419a6ff01). The original notice follows.
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

//! Inode location and loading from a cylinder-group block.

use crate::dinode::Inode;
use crate::error::UfsError;
use crate::fs::Superblock;
use crate::types::Ino;

/// Decode the inode at `slot` in a full filesystem block.
pub(crate) fn decode_from_block(
    fs: &Superblock,
    block: &[u8],
    slot: usize,
    ino: Ino,
) -> Result<Inode, UfsError> {
    if block.len() != fs.bsize as usize || slot >= fs.inopb as usize {
        return Err(UfsError::Corrupt("inode slot"));
    }
    let offset = slot
        .checked_mul(fs.inode_size())
        .ok_or(UfsError::Corrupt("inode offset"))?;
    let end = offset
        .checked_add(fs.inode_size())
        .ok_or(UfsError::Corrupt("inode offset"))?;
    let raw = block
        .get(offset..end)
        .ok_or(UfsError::Corrupt("inode outside block"))?;
    let inode = Inode::decode(fs.ufs2, raw)?;
    if ino == 0 {
        return Err(UfsError::Invalid);
    }
    Ok(inode)
}

/// The fragment block and slot that hold `ino`.
pub(crate) fn location(fs: &Superblock, ino: Ino) -> Result<(u64, usize), UfsError> {
    fs.ino_to_fsba(ino)
}
