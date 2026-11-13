// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright 1996-1998 John D. Polstra.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/map_object.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS OR
// IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES
// OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED.
// IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY DIRECT, INDIRECT,
// INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
// NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
// THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
//! Segment planning: the arithmetic of `map_object.c` without the mapping.
//!
//! The donor reserves the span and overlays each segment with `mmap(MAP_FIXED)`. Raven's `FIXED`
//! refuses to overlap (A-ld-10, G-5), so this module only describes the steps; C62 chooses the
//! address and performs the maps. The validation is new: the donor trusts the file.

use alloc::vec::Vec;

use crate::elf::Phdr;
use crate::errors::ElfError;
use crate::limits::MAX_LOAD_SEGMENTS;
use crate::object::TlsTemplate;

use crate::consts::{PF_W, PF_X, PT_GNU_EH_FRAME, PT_GNU_RELRO, PT_LOAD, PT_TLS};

/// The kind of one mapping step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    /// Map file content (`MAP` private, copy-on-write).
    File,
    /// Anonymous zero pages.
    Zero,
    /// The partial last file page: map the file page, then zero from `p_filesz` to the page end.
    Tail,
}

/// One mapping step, in the order the mapper must perform it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapStep {
    /// Page-aligned virtual address.
    pub vaddr: u64,
    /// Length in bytes (a page multiple).
    pub len: u64,
    /// File offset for a `File` step; 0 for `Zero` and `Tail`.
    pub file_off: u64,
    /// `PF_*` protection bits.
    pub prot: u8,
    /// Step kind.
    pub kind: StepKind,
}

/// The plan for one object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentPlan {
    /// The reserved address span.
    pub reserve_len: u64,
    /// The page-aligned lowest virtual address.
    pub base_vaddr: u64,
    /// The ordered steps.
    pub steps: Vec<MapStep>,
    /// The page-rounded `PT_GNU_RELRO` range.
    pub relro: Option<(u64, u64)>,
    /// The `PT_TLS` template.
    pub tls: Option<TlsTemplate>,
    /// The `PT_GNU_EH_FRAME` range.
    pub eh_frame: Option<(u64, u64)>,
}

fn page_floor(value: u64, page: u64) -> u64 {
    value & !page.wrapping_sub(1)
}

fn page_ceil(value: u64, page: u64) -> Result<u64, ElfError> {
    let last = value
        .checked_add(page.wrapping_sub(1))
        .ok_or(ElfError::BadSegment)?;
    Ok(page_floor(last, page))
}

/// Plan the mapping of `phdrs` with `page`.
pub fn plan_segments(phdrs: &[Phdr], page: u64) -> Result<SegmentPlan, ElfError> {
    if page == 0 || !page.is_power_of_two() {
        return Err(ElfError::BadEntry);
    }
    let mut loads: Vec<&Phdr> = phdrs.iter().filter(|phdr| phdr.p_type == PT_LOAD).collect();
    if loads.is_empty() {
        return Err(ElfError::NoLoad);
    }
    if loads.len() > MAX_LOAD_SEGMENTS {
        return Err(ElfError::TooManySegments);
    }
    loads.sort_by_key(|phdr| phdr.p_vaddr);

    let mut steps = Vec::new();
    let mut previous_end = None;
    let mut lowest = u64::MAX;
    let mut highest = 0u64;
    for phdr in &loads {
        if phdr.p_filesz > phdr.p_memsz {
            return Err(ElfError::BadSegment);
        }
        if phdr.p_flags & PF_W != 0 && phdr.p_flags & PF_X != 0 {
            return Err(ElfError::BadSegment);
        }
        if phdr.p_align > 65_536 {
            return Err(ElfError::BadSegment);
        }
        if phdr.p_offset.checked_rem(page) != phdr.p_vaddr.checked_rem(page) {
            return Err(ElfError::BadSegment);
        }
        let segment_end = phdr
            .p_vaddr
            .checked_add(phdr.p_memsz)
            .ok_or(ElfError::BadSegment)?;
        if let Some(end) = previous_end
            && phdr.p_vaddr < end
        {
            return Err(ElfError::BadSegment);
        }
        previous_end = Some(segment_end);
        lowest = lowest.min(phdr.p_vaddr);
        highest = highest.max(segment_end);

        let prot = u8::try_from(phdr.p_flags & 0x7).map_err(|_| ElfError::BadSegment)?;
        let file_start = page_floor(phdr.p_vaddr, page);
        let skip = phdr.p_vaddr.saturating_sub(file_start);
        let file_off = phdr
            .p_offset
            .checked_sub(skip)
            .ok_or(ElfError::BadSegment)?;
        let memory_end = page_ceil(segment_end, page)?;
        if phdr.p_filesz == 0 {
            steps.push(MapStep {
                vaddr: file_start,
                len: memory_end.saturating_sub(file_start),
                file_off: 0,
                prot,
                kind: StepKind::Zero,
            });
            continue;
        }
        let file_limit = phdr
            .p_vaddr
            .checked_add(phdr.p_filesz)
            .ok_or(ElfError::BadSegment)?;
        let file_end = page_ceil(file_limit, page)?;
        if file_end > file_start {
            steps.push(MapStep {
                vaddr: file_start,
                len: file_end.saturating_sub(file_start),
                file_off,
                prot,
                kind: StepKind::File,
            });
        }
        let unaligned_end = file_limit;
        if unaligned_end.checked_rem(page) != Some(0) && phdr.p_memsz > phdr.p_filesz {
            steps.push(MapStep {
                vaddr: unaligned_end,
                len: file_end.saturating_sub(unaligned_end),
                file_off: 0,
                prot,
                kind: StepKind::Tail,
            });
        }
        if memory_end > file_end {
            steps.push(MapStep {
                vaddr: file_end,
                len: memory_end.saturating_sub(file_end),
                file_off: 0,
                prot,
                kind: StepKind::Zero,
            });
        }
    }
    let base_vaddr = page_floor(lowest, page);
    let reserve_len = page_ceil(highest, page)?.saturating_sub(base_vaddr);

    let mut relro = None;
    let mut tls = None;
    let mut eh_frame = None;
    for phdr in phdrs {
        match phdr.p_type {
            PT_GNU_RELRO => {
                let start = phdr.p_vaddr;
                let end = page_floor(
                    phdr.p_vaddr
                        .checked_add(phdr.p_memsz)
                        .ok_or(ElfError::BadSegment)?,
                    page,
                );
                relro = Some((start, end));
            }
            PT_TLS => {
                if phdr.p_filesz > phdr.p_memsz {
                    return Err(ElfError::BadSegment);
                }
                tls = Some(TlsTemplate {
                    vaddr: phdr.p_vaddr,
                    filesz: phdr.p_filesz,
                    memsz: phdr.p_memsz,
                    align: phdr.p_align,
                });
            }
            PT_GNU_EH_FRAME => {
                let end = phdr
                    .p_vaddr
                    .checked_add(phdr.p_filesz)
                    .ok_or(ElfError::BadSegment)?;
                eh_frame = Some((phdr.p_vaddr, end));
            }
            _ => {}
        }
    }

    Ok(SegmentPlan {
        reserve_len,
        base_vaddr,
        steps,
        relro,
        tls,
        eh_frame,
    })
}

#[cfg(test)]
#[path = "segments_ut.rs"]
mod tests;
