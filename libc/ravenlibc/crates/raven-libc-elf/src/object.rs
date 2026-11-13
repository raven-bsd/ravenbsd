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
//! The object view: program headers, the dynamic section and every table the loader needs.
//!
//! `digest` is the only place the load bias is added. It runs two passes over `_DYNAMIC`
//! (T-11): the first records the table addresses and sizes, the second resolves the string
//! offsets once `DT_STRTAB` and `DT_STRSZ` are known. Every recorded range is checked against
//! the `PT_LOAD` segments (A-ld-9) before it is stored; a read is always `mem.read(..)`.

use alloc::vec::Vec;

use crate::consts::{
    DF_1_NODEFLIB, DF_1_NOW, DF_BIND_NOW, DF_ORIGIN, DF_SYMBOLIC, DF_TEXTREL, DT_BIND_NOW,
    DT_DEBUG, DT_FINI, DT_FINI_ARRAY, DT_FINI_ARRAYSZ, DT_FLAGS, DT_FLAGS_1, DT_GNU_HASH, DT_HASH,
    DT_INIT, DT_INIT_ARRAY, DT_INIT_ARRAYSZ, DT_JMPREL, DT_NEEDED, DT_NULL, DT_PLTREL, DT_PLTRELSZ,
    DT_PREINIT_ARRAY, DT_PREINIT_ARRAYSZ, DT_REL, DT_RELA, DT_RELAENT, DT_RELASZ, DT_RELENT,
    DT_RELR, DT_RELRENT, DT_RELRSZ, DT_RELSZ, DT_RPATH, DT_RUNPATH, DT_SONAME, DT_STRSZ, DT_STRTAB,
    DT_SYMBOLIC, DT_SYMENT, DT_SYMTAB, DT_TEXTREL, DT_VERDEF, DT_VERDEFNUM, DT_VERNEED,
    DT_VERNEEDNUM, DT_VERSYM, DYN_SIZE, PF_X, PT_DYNAMIC, PT_GNU_EH_FRAME, PT_GNU_RELRO,
    PT_GNU_STACK, PT_LOAD, PT_TLS, SYM_SIZE,
};
use crate::elf::{Dyn, Sym};
use crate::errors::DigestError;
use crate::limits::{
    MAX_DYN_ENTRIES, MAX_INIT_ARRAY, MAX_LOAD_SEGMENTS, MAX_NEEDED, MAX_PHDRS, MAX_SYMBOLS,
};
use crate::lookup::{GnuHash, SysvHash, read_symbol, sysv_hash_bytes};
use crate::mem::MemView;

/// The page size of every supported target (PROPOSED: `-z max-page-size=4096`).
pub const PAGE_SIZE: u64 = 4096;

/// The byte size of one `Elf64_Dyn` entry.
const DYNAMIC_ENTRY: u64 = DYN_SIZE as u64;

/// One `PT_LOAD` segment, with an absolute address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadSeg {
    /// Absolute virtual address.
    pub vaddr: u64,
    /// `p_memsz`.
    pub memsz: u64,
    /// `p_filesz`.
    pub filesz: u64,
    /// `p_offset`.
    pub offset: u64,
    /// `p_flags`.
    pub flags: u32,
}

/// The `PT_TLS` template, with an absolute address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TlsTemplate {
    /// Absolute address of the `.tdata` image.
    pub vaddr: u64,
    /// `p_filesz`: bytes to copy from the image.
    pub filesz: u64,
    /// `p_memsz`: the full block.
    pub memsz: u64,
    /// `p_align`.
    pub align: u64,
}

/// Everything the loader learns from one mapped object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjView {
    /// The load bias.
    pub bias: u64,
    /// Absolute address and count of the program headers.
    pub phdr: (u64, u16),
    /// The loadable segments.
    pub segments: Vec<LoadSeg>,
    /// Absolute address of `_DYNAMIC`.
    pub dynamic: u64,
    /// The TLS template, when the object has one.
    pub tls: Option<TlsTemplate>,
    /// The page-rounded `PT_GNU_RELRO` range.
    pub relro: Option<(u64, u64)>,
    /// The `PT_GNU_EH_FRAME` range.
    pub eh_frame: Option<(u64, u64)>,
    /// `PT_GNU_STACK` has `PF_X` (the loader refuses it).
    pub gnu_stack_exec: bool,
    /// `DT_NEEDED` string-table offsets in file order.
    pub needed: Vec<u32>,
    /// `DT_SONAME` string-table offset.
    pub soname: Option<u32>,
    /// `DT_RPATH`, `None` when `DT_RUNPATH` is present (T-12).
    pub rpath: Option<u32>,
    /// `DT_RUNPATH`.
    pub runpath: Option<u32>,
    /// Absolute address of the dynamic symbol table.
    pub symtab: u64,
    /// Absolute address of the dynamic string table.
    pub strtab: u64,
    /// `DT_STRSZ`.
    pub strsz: u64,
    /// Number of dynamic symbols.
    pub dynsymcount: u32,
    /// The SysV hash table.
    pub hash_sysv: Option<SysvHash>,
    /// The GNU hash table.
    pub hash_gnu: Option<GnuHash>,
    /// Absolute address of `DT_VERSYM`.
    pub versym: Option<u64>,
    /// Absolute address and count of `DT_VERDEF`.
    pub verdef: Option<(u64, u32)>,
    /// Absolute address and count of `DT_VERNEED`.
    pub verneed: Option<(u64, u32)>,
    /// The `.rela.dyn` range: address and size in bytes.
    pub rela: (u64, u64),
    /// The `.rela.plt` range: address and size in bytes.
    pub plt_rela: (u64, u64),
    /// The `DT_RELR` range: address and size in bytes.
    pub relr: (u64, u64),
    /// `DT_INIT`.
    pub init: u64,
    /// `DT_FINI`.
    pub fini: u64,
    /// `DT_INIT_ARRAY`: address and size in bytes.
    pub init_array: (u64, u64),
    /// `DT_FINI_ARRAY`: address and size in bytes.
    pub fini_array: (u64, u64),
    /// `DT_PREINIT_ARRAY`: address and size in bytes.
    pub preinit_array: (u64, u64),
    /// `DT_FLAGS`.
    pub flags: u32,
    /// `DT_FLAGS_1`.
    pub flags1: u32,
    /// `DT_SYMBOLIC` or `DF_SYMBOLIC`.
    pub symbolic: bool,
    /// Bind every symbol at load (`DT_BIND_NOW`, `DF_BIND_NOW`, `DF_1_NOW`).
    pub bind_now: bool,
    /// Absolute address of the `DT_DEBUG` slot.
    pub debug_slot: Option<u64>,
    /// `DT_SONAME` and every alias, as string-table offsets.
    pub names: Vec<u32>,
}

/// Whether `[addr, addr + len)` lies inside one loadable segment.
fn range_in_segments(segments: &[LoadSeg], addr: u64, len: u64) -> bool {
    let Some(end) = addr.checked_add(len) else {
        return false;
    };
    segments.iter().any(|segment| {
        let segment_end = segment.vaddr.saturating_add(segment.memsz);
        addr >= segment.vaddr && end <= segment_end
    })
}

fn check_range(
    segments: &[LoadSeg],
    addr: u64,
    len: u64,
    tag: &'static str,
) -> Result<(), DigestError> {
    if len == 0 && addr == 0 {
        return Ok(());
    }
    if range_in_segments(segments, addr, len) {
        Ok(())
    } else {
        Err(DigestError::RangeOutsideLoad(tag))
    }
}

/// Raw values from pass 1.
#[derive(Default)]
struct Raw {
    strtab: u64,
    strsz: u64,
    symtab: u64,
    hash: u64,
    gnu_hash: u64,
    rela: u64,
    relasz: u64,
    relaent: u64,
    plt: u64,
    pltsz: u64,
    pltrel: i64,
    relr: u64,
    relrsz: u64,
    relrent: u64,
    versym: u64,
    verdef: u64,
    verdefnum: u32,
    verneed: u64,
    verneednum: u32,
    init: u64,
    fini: u64,
    init_array: u64,
    init_size: u64,
    fini_array: u64,
    fini_size: u64,
    preinit_array: u64,
    preinit_size: u64,
    flags: u32,
    flags1: u32,
    debug_slot: Option<u64>,
    needed: Vec<u32>,
    soname: Option<u32>,
    rpath: Option<u32>,
    runpath: Option<u32>,
    rel_present: bool,
    textrel: bool,
    symbolic: bool,
    bind_now: bool,
}

impl ObjView {
    /// Digest one already-mapped object.
    ///
    /// `phdr_vaddr` is the link-time address of the program header array; `digest` adds `bias`
    /// and every address it records is absolute (T-10).
    pub fn digest(
        mem: &dyn MemView,
        phdr_vaddr: u64,
        phnum: u16,
        bias: u64,
    ) -> Result<Self, DigestError> {
        if usize::from(phnum) > MAX_PHDRS {
            return Err(DigestError::BadDynamic);
        }
        let absolute_phdr = bias
            .checked_add(phdr_vaddr)
            .ok_or(DigestError::BadDynamic)?;
        let headers = crate::elf::read_phdrs(mem, absolute_phdr, phnum)
            .map_err(|_| DigestError::BadDynamic)?;
        let mut segments = Vec::new();
        let mut dynamic = None;
        let mut tls = None;
        let mut relro = None;
        let mut eh_frame = None;
        let mut gnu_stack_exec = false;
        for header in &headers {
            match header.p_type {
                PT_LOAD => {
                    if header.p_filesz > header.p_memsz {
                        return Err(DigestError::BadSize);
                    }
                    if segments.len() >= MAX_LOAD_SEGMENTS {
                        return Err(DigestError::BadSize);
                    }
                    segments.push(LoadSeg {
                        vaddr: bias
                            .checked_add(header.p_vaddr)
                            .ok_or(DigestError::BadDynamic)?,
                        memsz: header.p_memsz,
                        filesz: header.p_filesz,
                        offset: header.p_offset,
                        flags: header.p_flags,
                    });
                }
                PT_DYNAMIC => {
                    dynamic = Some(
                        bias.checked_add(header.p_vaddr)
                            .ok_or(DigestError::BadDynamic)?,
                    );
                }
                PT_TLS => {
                    if header.p_filesz > header.p_memsz {
                        return Err(DigestError::BadSize);
                    }
                    tls = Some(TlsTemplate {
                        vaddr: bias
                            .checked_add(header.p_vaddr)
                            .ok_or(DigestError::BadDynamic)?,
                        filesz: header.p_filesz,
                        memsz: header.p_memsz,
                        align: header.p_align,
                    });
                }
                PT_GNU_RELRO => {
                    let start = bias
                        .checked_add(header.p_vaddr)
                        .ok_or(DigestError::BadDynamic)?;
                    let end = start
                        .checked_add(header.p_memsz)
                        .ok_or(DigestError::BadDynamic)?;
                    // The protected range ends on a page boundary; rounding down keeps the
                    // following data writable (13 3.12).
                    relro = Some((start, end & !(PAGE_SIZE - 1)));
                }
                PT_GNU_EH_FRAME => {
                    let start = bias
                        .checked_add(header.p_vaddr)
                        .ok_or(DigestError::BadDynamic)?;
                    eh_frame = Some((start, header.p_filesz));
                }
                PT_GNU_STACK => {
                    gnu_stack_exec = header.p_flags & PF_X != 0;
                }
                _ => {}
            }
        }
        let dynamic = dynamic.ok_or(DigestError::NotDynamic)?;
        if segments.is_empty() {
            return Err(DigestError::BadDynamic);
        }

        let mut raw = Raw::default();
        for index in 0..MAX_DYN_ENTRIES {
            let offset = u64::try_from(index)
                .ok()
                .and_then(|value| value.checked_mul(DYNAMIC_ENTRY))
                .ok_or(DigestError::BadDynamic)?;
            let address = dynamic.checked_add(offset).ok_or(DigestError::BadDynamic)?;
            let bytes = mem.read(address, 16).ok_or(DigestError::BadDynamic)?;
            let entry = Dyn::parse(bytes).map_err(|_| DigestError::BadDynamic)?;
            if entry.tag == DT_NULL {
                return finish(
                    mem,
                    &segments,
                    bias,
                    absolute_phdr,
                    phnum,
                    dynamic,
                    raw,
                    tls,
                    relro,
                    eh_frame,
                    gnu_stack_exec,
                );
            }
            read_entry(&mut raw, entry, address, bias)?;
        }
        Err(DigestError::BadDynamic)
    }

    /// Resolve a string-table offset to its bytes.
    pub fn name_at<'a>(&self, mem: &'a dyn MemView, str_off: u32) -> Result<&'a [u8], DigestError> {
        if u64::from(str_off) >= self.strsz {
            return Err(DigestError::BadString);
        }
        let address = self
            .strtab
            .checked_add(u64::from(str_off))
            .ok_or(DigestError::BadString)?;
        mem.cstr(address).ok_or(DigestError::BadString)
    }

    /// The dynamic symbol at `index`.
    pub fn sym(&self, mem: &dyn MemView, index: u32) -> Option<Sym> {
        if index >= self.dynsymcount {
            return None;
        }
        read_symbol(self, mem, index)
    }

    /// Whether `DF_1_NODEFLIB` is set.
    pub fn nodeflib(&self) -> bool {
        self.flags1 & u32::try_from(DF_1_NODEFLIB).unwrap_or(0) != 0
    }

    /// Whether `DF_ORIGIN`/`DF_1_ORIGIN` is set.
    pub fn has_origin(&self) -> bool {
        self.flags & u32::try_from(DF_ORIGIN).unwrap_or(0) != 0
            || self.flags1 & u32::try_from(crate::consts::DF_1_ORIGIN).unwrap_or(0) != 0
    }
}

#[allow(clippy::too_many_arguments)] // the digest context, collected in one place
fn finish(
    mem: &dyn MemView,
    segments: &[LoadSeg],
    bias: u64,
    phdr: u64,
    phnum: u16,
    dynamic: u64,
    raw: Raw,
    tls: Option<TlsTemplate>,
    relro: Option<(u64, u64)>,
    eh_frame: Option<(u64, u64)>,
    gnu_stack_exec: bool,
) -> Result<ObjView, DigestError> {
    check_range(segments, dynamic, 16, "PT_DYNAMIC")?;
    let _ = mem;
    if raw.rel_present {
        return Err(DigestError::RelNotSupported);
    }
    if raw.textrel {
        return Err(DigestError::TextRel);
    }
    if raw.plt != 0 && raw.pltrel != DT_RELA {
        return Err(DigestError::RelNotSupported);
    }
    if raw.relaent != 0 && raw.relaent != 24 {
        return Err(DigestError::BadSize);
    }
    if raw.relrent != 0 && raw.relrent != 8 {
        return Err(DigestError::BadSize);
    }
    if raw.strtab == 0 || raw.symtab == 0 {
        return Err(DigestError::NoSymtab);
    }
    check_range(segments, raw.strtab, raw.strsz, "DT_STRTAB")?;
    check_range(segments, raw.rela, raw.relasz, "DT_RELA")?;
    check_range(segments, raw.plt, raw.pltsz, "DT_JMPREL")?;
    check_range(segments, raw.relr, raw.relrsz, "DT_RELR")?;
    if !raw.relasz.is_multiple_of(24) || !raw.pltsz.is_multiple_of(24) {
        return Err(DigestError::BadSize);
    }
    if raw.pltsz != 0 && raw.relaent != 0 && raw.relaent != 24 {
        return Err(DigestError::BadSize);
    }

    let hash_sysv = if raw.hash != 0 {
        let header = mem
            .read(raw.hash, 8)
            .ok_or(DigestError::RangeOutsideLoad("DT_HASH"))?;
        let first: [u8; 4] = header
            .get(..4)
            .and_then(|slice| slice.try_into().ok())
            .ok_or(DigestError::BadDynamic)?;
        let second: [u8; 4] = header
            .get(4..8)
            .and_then(|slice| slice.try_into().ok())
            .ok_or(DigestError::BadDynamic)?;
        let nbucket = u32::from_le_bytes(first);
        let nchain = u32::from_le_bytes(second);
        let bytes = sysv_hash_bytes(nbucket, nchain).ok_or(DigestError::BadSize)?;
        check_range(segments, raw.hash, bytes, "DT_HASH")?;
        if nbucket == 0 || nchain == 0 {
            None
        } else {
            Some(SysvHash {
                nbucket,
                nchain,
                addr: raw.hash,
            })
        }
    } else {
        None
    };

    let hash_gnu = if raw.gnu_hash != 0 {
        let header = mem
            .read(raw.gnu_hash, 16)
            .ok_or(DigestError::RangeOutsideLoad("DT_GNU_HASH"))?;
        let nbuckets = read_u32(header, 0)?;
        let symoffset = read_u32(header, 4)?;
        let bloom_size = read_u32(header, 8)?;
        let bloom_shift = read_u32(header, 12)?;
        if bloom_size == 0 || !bloom_size.is_power_of_two() {
            return Err(DigestError::NoHash);
        }
        let bytes = 16u64
            .checked_add(u64::from(bloom_size) * 8)
            .and_then(|value| value.checked_add(u64::from(nbuckets) * 4))
            .ok_or(DigestError::BadSize)?;
        check_range(segments, raw.gnu_hash, bytes, "DT_GNU_HASH")?;
        if nbuckets == 0 {
            None
        } else {
            let bloom = raw
                .gnu_hash
                .checked_add(16)
                .ok_or(DigestError::BadDynamic)?;
            let buckets = bloom
                .checked_add(u64::from(bloom_size) * 8)
                .ok_or(DigestError::BadDynamic)?;
            let chains = buckets
                .checked_add(u64::from(nbuckets) * 4)
                .ok_or(DigestError::BadDynamic)?;
            Some(GnuHash {
                nbuckets,
                symoffset,
                bloom_size,
                bloom_shift,
                bloom,
                buckets,
                chains,
            })
        }
    } else {
        None
    };

    if hash_sysv.is_none() && hash_gnu.is_none() {
        return Err(DigestError::NoHash);
    }
    let dynsymcount = dynsym_count(mem, hash_sysv, hash_gnu)?;
    if usize::try_from(dynsymcount).unwrap_or(usize::MAX) > MAX_SYMBOLS {
        return Err(DigestError::BadSize);
    }
    check_range(
        segments,
        raw.symtab,
        u64::from(dynsymcount)
            .checked_mul(u64::try_from(SYM_SIZE).unwrap_or(24))
            .ok_or(DigestError::BadSize)?,
        "DT_SYMTAB",
    )?;
    if raw.versym != 0 {
        check_range(
            segments,
            raw.versym,
            u64::from(dynsymcount) * 2,
            "DT_VERSYM",
        )?;
    }
    if raw.verdef != 0 {
        check_range(segments, raw.verdef, 20, "DT_VERDEF")?;
    }
    if raw.verneed != 0 {
        check_range(segments, raw.verneed, 16, "DT_VERNEED")?;
    }
    for (address, size, tag) in [
        (raw.init_array, raw.init_size, "DT_INIT_ARRAY"),
        (raw.fini_array, raw.fini_size, "DT_FINI_ARRAY"),
        (raw.preinit_array, raw.preinit_size, "DT_PREINIT_ARRAY"),
    ] {
        if size != 0 {
            if size % 8 != 0 {
                return Err(DigestError::BadSize);
            }
            if size / 8 > MAX_INIT_ARRAY as u64 {
                return Err(DigestError::BadSize);
            }
            check_range(segments, address, size, tag)?;
        }
    }

    if raw.needed.len() > MAX_NEEDED {
        return Err(DigestError::TooManyNeeded);
    }
    // Pass 2: validate every string-table offset now that STRTAB/STRSZ are known.
    let strtab = raw.strtab;
    let strsz = raw.strsz;
    let mut names = Vec::new();
    if let Some(soname) = raw.soname {
        check_string(strtab, strsz, soname)?;
        names.push(soname);
    }
    let rpath = match raw.rpath {
        Some(offset) if raw.runpath.is_none() => {
            check_string(strtab, strsz, offset)?;
            Some(offset)
        }
        _ => None,
    };
    let runpath = match raw.runpath {
        Some(offset) => {
            check_string(strtab, strsz, offset)?;
            Some(offset)
        }
        None => None,
    };
    for needed in &raw.needed {
        check_string(strtab, strsz, *needed)?;
    }

    let mut symbolic = raw.symbolic;
    symbolic |= raw.flags & u32::try_from(DF_SYMBOLIC).unwrap_or(0) != 0;
    let bind_now = raw.bind_now
        || raw.flags & u32::try_from(DF_BIND_NOW).unwrap_or(0) != 0
        || raw.flags1 & u32::try_from(DF_1_NOW).unwrap_or(0) != 0;
    let _ = DF_TEXTREL;

    Ok(ObjView {
        bias,
        phdr: (phdr, phnum),
        segments: segments.to_vec(),
        dynamic,
        tls,
        relro,
        eh_frame,
        gnu_stack_exec,
        needed: raw.needed,
        soname: raw.soname,
        rpath,
        runpath,
        symtab: raw.symtab,
        strtab,
        strsz,
        dynsymcount,
        hash_sysv,
        hash_gnu,
        versym: (raw.versym != 0).then_some(raw.versym),
        verdef: (raw.verdef != 0).then_some((raw.verdef, raw.verdefnum)),
        verneed: (raw.verneed != 0).then_some((raw.verneed, raw.verneednum)),
        rela: (raw.rela, raw.relasz),
        plt_rela: (raw.plt, raw.pltsz),
        relr: (raw.relr, raw.relrsz),
        init: raw.init,
        fini: raw.fini,
        init_array: (raw.init_array, raw.init_size),
        fini_array: (raw.fini_array, raw.fini_size),
        preinit_array: (raw.preinit_array, raw.preinit_size),
        flags: raw.flags,
        flags1: raw.flags1,
        symbolic,
        bind_now,
        debug_slot: raw.debug_slot,
        names,
    })
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, DigestError> {
    let slice = bytes
        .get(offset..offset.saturating_add(4))
        .ok_or(DigestError::BadDynamic)?;
    let array: [u8; 4] = slice.try_into().map_err(|_| DigestError::BadDynamic)?;
    Ok(u32::from_le_bytes(array))
}

fn check_string(strtab: u64, strsz: u64, offset: u32) -> Result<(), DigestError> {
    if u64::from(offset) >= strsz {
        return Err(DigestError::BadString);
    }
    strtab
        .checked_add(u64::from(offset))
        .ok_or(DigestError::BadString)?;
    Ok(())
}

fn dynsym_count(
    mem: &dyn MemView,
    sysv: Option<SysvHash>,
    gnu: Option<GnuHash>,
) -> Result<u32, DigestError> {
    if let Some(table) = sysv {
        return Ok(table.nchain);
    }
    let Some(table) = gnu else {
        return Err(DigestError::NoHash);
    };
    let mut count = table.symoffset;
    let mut steps = 0usize;
    for bucket in 0..table.nbuckets {
        let address = table
            .buckets
            .checked_add(u64::from(bucket) * 4)
            .ok_or(DigestError::BadDynamic)?;
        let bytes = mem.read(address, 4).ok_or(DigestError::BadDynamic)?;
        let value = read_u32(bytes, 0)?;
        if value == 0 {
            continue;
        }
        let mut index = value;
        loop {
            steps = steps.saturating_add(1);
            if steps > MAX_SYMBOLS {
                return Err(DigestError::BadDynamic);
            }
            let offset = index
                .checked_sub(table.symoffset)
                .ok_or(DigestError::BadDynamic)?;
            let chain_address = table
                .chains
                .checked_add(u64::from(offset) * 4)
                .ok_or(DigestError::BadDynamic)?;
            let bytes = mem.read(chain_address, 4).ok_or(DigestError::BadDynamic)?;
            let word = read_u32(bytes, 0)?;
            count = count.max(index.saturating_add(1));
            if word & 1 != 0 {
                break;
            }
            index = index.checked_add(1).ok_or(DigestError::BadDynamic)?;
        }
    }
    Ok(count)
}

fn read_entry(raw: &mut Raw, entry: Dyn, entry_address: u64, bias: u64) -> Result<(), DigestError> {
    let absolute = |value: u64| bias.checked_add(value).ok_or(DigestError::BadDynamic);
    match entry.tag {
        DT_NEEDED => {
            if raw.needed.len() >= MAX_NEEDED {
                return Err(DigestError::TooManyNeeded);
            }
            raw.needed
                .push(u32::try_from(entry.val).map_err(|_| DigestError::BadString)?);
        }
        DT_STRTAB => raw.strtab = absolute(entry.val)?,
        DT_STRSZ => raw.strsz = entry.val,
        DT_SYMTAB => raw.symtab = absolute(entry.val)?,
        DT_SYMENT => {
            if entry.val != 24 {
                return Err(DigestError::BadSize);
            }
        }
        DT_HASH => raw.hash = absolute(entry.val)?,
        DT_GNU_HASH => raw.gnu_hash = absolute(entry.val)?,
        DT_RELA => raw.rela = absolute(entry.val)?,
        DT_RELASZ => raw.relasz = entry.val,
        DT_RELAENT => raw.relaent = entry.val,
        DT_JMPREL => raw.plt = absolute(entry.val)?,
        DT_PLTRELSZ => raw.pltsz = entry.val,
        DT_PLTREL => raw.pltrel = i64::try_from(entry.val).map_err(|_| DigestError::BadDynamic)?,
        DT_RELR => raw.relr = absolute(entry.val)?,
        DT_RELRSZ => raw.relrsz = entry.val,
        DT_RELRENT => raw.relrent = entry.val,
        DT_REL | DT_RELSZ | DT_RELENT => raw.rel_present = true,
        DT_VERSYM => raw.versym = absolute(entry.val)?,
        DT_VERDEF => raw.verdef = absolute(entry.val)?,
        DT_VERDEFNUM => {
            raw.verdefnum = u32::try_from(entry.val).map_err(|_| DigestError::BadVersionTable)?;
        }
        DT_VERNEED => raw.verneed = absolute(entry.val)?,
        DT_VERNEEDNUM => {
            raw.verneednum = u32::try_from(entry.val).map_err(|_| DigestError::BadVersionTable)?;
        }
        DT_INIT => raw.init = absolute(entry.val)?,
        DT_FINI => raw.fini = absolute(entry.val)?,
        DT_INIT_ARRAY => raw.init_array = absolute(entry.val)?,
        DT_INIT_ARRAYSZ => raw.init_size = entry.val,
        DT_FINI_ARRAY => raw.fini_array = absolute(entry.val)?,
        DT_FINI_ARRAYSZ => raw.fini_size = entry.val,
        DT_PREINIT_ARRAY => raw.preinit_array = absolute(entry.val)?,
        DT_PREINIT_ARRAYSZ => raw.preinit_size = entry.val,
        DT_TEXTREL => raw.textrel = true,
        DT_SYMBOLIC => raw.symbolic = true,
        DT_BIND_NOW => raw.bind_now = true,
        DT_FLAGS => {
            raw.flags = u32::try_from(entry.val).map_err(|_| DigestError::BadDynamic)?;
            if entry.val & DF_TEXTREL != 0 {
                raw.textrel = true;
            }
            if entry.val & DF_SYMBOLIC != 0 {
                raw.symbolic = true;
            }
        }
        DT_FLAGS_1 => {
            raw.flags1 = u32::try_from(entry.val).map_err(|_| DigestError::BadDynamic)?;
        }
        DT_DEBUG => {
            raw.debug_slot = Some(
                entry_address
                    .checked_add(8)
                    .ok_or(DigestError::BadDynamic)?,
            );
        }
        DT_SONAME => {
            raw.soname = Some(u32::try_from(entry.val).map_err(|_| DigestError::BadString)?)
        }
        DT_RPATH => raw.rpath = Some(u32::try_from(entry.val).map_err(|_| DigestError::BadString)?),
        DT_RUNPATH => {
            raw.runpath = Some(u32::try_from(entry.val).map_err(|_| DigestError::BadString)?)
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
#[path = "object_ut.rs"]
mod tests;
