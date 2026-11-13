// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright 1996, 1997, 1998, 1999, 2000 John D. Polstra.
// Copyright 2003 Alexander Kabaev <kan@FreeBSD.ORG>.
// Copyright 2009-2013 Konstantin Belousov <kib@FreeBSD.ORG>.
// Copyright 2012 John Marino <draco@marino.st>.
// Copyright 2014-2017 The FreeBSD Foundation
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: 133344405f532bd54ff3629a77d4c4e062f01fdb)
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
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! `DT_RELR`: relative relocations packed as addresses and 63-bit bitmaps.
//!
//! The decoder yields **link-time offsets**, never writes: the applier adds the bias. The
//! sequence is one address entry, then zero or more bitmap entries that apply to the 63
//! pointer-sized words following that address.

use alloc::vec::Vec;

use crate::errors::RelocError;
use crate::mem::MemView;
use crate::object::{LoadSeg, ObjView};

/// The `DT_RELR` decoder.
#[derive(Clone)]
pub struct RelrIter<'a> {
    mem: &'a dyn MemView,
    address: u64,
    remaining: u64,
    segments: Vec<LoadSeg>,
    bias: u64,
    bitmap: u64,
    bit: u32,
    base: u64,
    failed: bool,
}

impl<'a> RelrIter<'a> {
    fn word(&mut self) -> Result<u64, RelocError> {
        if self.remaining < 8 {
            return Err(RelocError::BadRelr);
        }
        let bytes = self.mem.read(self.address, 8).ok_or(RelocError::BadRelr)?;
        let array: [u8; 8] = bytes.try_into().map_err(|_| RelocError::BadRelr)?;
        self.address = self.address.checked_add(8).ok_or(RelocError::BadRelr)?;
        self.remaining = self.remaining.saturating_sub(8);
        Ok(u64::from_le_bytes(array))
    }

    fn check(&self, offset: u64) -> Result<u64, RelocError> {
        let absolute = self.bias.checked_add(offset).ok_or(RelocError::BadRelr)?;
        let end = absolute.checked_add(8).ok_or(RelocError::BadRelr)?;
        let inside = self.segments.iter().any(|segment| {
            let segment_end = segment.vaddr.saturating_add(segment.memsz);
            absolute >= segment.vaddr && end <= segment_end
        });
        if inside {
            Ok(offset)
        } else {
            Err(RelocError::BadRelr)
        }
    }
}

impl Iterator for RelrIter<'_> {
    type Item = Result<u64, RelocError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        loop {
            if self.bit <= 63 {
                let bit = self.bit;
                self.bit = self.bit.saturating_add(1);
                if self.bitmap & (1u64 << bit) != 0 {
                    // The address entry relocates its own word and advances: bitmap bit `b`
                    // relocates the word at `base + b * 8` (the donor's `where++`).
                    let offset = self.base.wrapping_add(u64::from(bit).wrapping_mul(8));
                    let result = self.check(offset);
                    if result.is_err() {
                        self.failed = true;
                    }
                    return Some(result);
                }
                continue;
            }
            if self.remaining == 0 {
                return None;
            }
            let entry = match self.word() {
                Ok(entry) => entry,
                Err(error) => {
                    self.failed = true;
                    return Some(Err(error));
                }
            };
            if entry & 1 == 0 {
                let result = self.check(entry);
                if result.is_err() {
                    self.failed = true;
                    return Some(result);
                }
                self.base = entry;
                self.bitmap = 0;
                self.bit = 64;
                return Some(Ok(entry));
            }
            self.bitmap = entry;
            self.bit = 1;
        }
    }
}

/// Decode `obj`'s `DT_RELR` stream into link-time offsets.
pub fn relr_offsets<'a>(obj: &ObjView, mem: &'a dyn MemView) -> Result<RelrIter<'a>, RelocError> {
    let (address, size) = obj.relr;
    if size % 8 != 0 {
        return Err(RelocError::BadRelr);
    }
    Ok(RelrIter {
        mem,
        address,
        remaining: size,
        segments: obj.segments.clone(),
        bias: obj.bias,
        bitmap: 0,
        bit: 64,
        base: 0,
        failed: false,
    })
}

#[cfg(test)]
#[path = "relr_ut.rs"]
mod tests;
