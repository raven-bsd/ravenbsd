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
//! Symbol lookup in one object: the SysV bucket/chain walk and the GNU bloom/bucket/chain walk.

use crate::consts::{
    SHN_UNDEF, STB_LOCAL, STT_FUNC, STT_GNU_IFUNC, STT_NOTYPE, STT_OBJECT, STT_TLS, STV_HIDDEN,
    STV_INTERNAL, SYM_SIZE, VER_FLG_BASE, VER_NDX_GIVEN, VER_NDX_HIDDEN, VER_NDX_MASK,
};
use crate::errors::LookupError;
use crate::hash::{elf_hash, gnu_hash};
use crate::limits::{MAX_SYMBOLS, MAX_VERSIONS};
use crate::mem::MemView;
use crate::object::ObjView;
use crate::scope::ObjId;

/// Request flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SymFlags(pub u32);

impl SymFlags {
    /// Looking up a `PLT` reference (`SYMLOOK_IN_PLT`).
    pub const IN_PLT: Self = Self(1);
    /// Do not consider `STT_TLS` symbols (`SYMLOOK_NOTLS`).
    pub const NO_TLS: Self = Self(2);
    /// Resolve IFUNC definitions now (`SYMLOOK_IFUNC`).
    pub const IFUNC: Self = Self(4);
    /// An early start-up lookup (`SYMLOOK_EARLY`).
    pub const EARLY: Self = Self(8);
    /// Search this object before the global scope (`RTLD_DEEPBIND`).
    pub const DEEPBIND: Self = Self(16);
    /// The object is the referencing object, so its hidden definitions are visible.
    pub const SELF: Self = Self(32);

    /// Whether every bit of `other` is set.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The union of two flag sets.
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// A version reference from the referencing object (`DT_VERSYM` + `DT_VERNEED`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerRef<'a> {
    /// The version name.
    pub name: &'a [u8],
    /// The `vna_hash` of the requirement.
    pub hash: u32,
    /// `VER_FLG_WEAK`: a missing definition is not an error.
    pub weak: bool,
    /// The dependency file name, when known.
    pub file: Option<&'a [u8]>,
}

/// One symbol request, with both hashes computed once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymLook<'a> {
    /// The symbol name.
    pub name: &'a [u8],
    /// `elf_hash(name)`.
    pub hash_sysv: u32,
    /// `gnu_hash(name)`.
    pub hash_gnu: u32,
    /// The requested version, if any.
    pub ver: Option<VerRef<'a>>,
    /// Request flags.
    pub flags: SymFlags,
}

impl<'a> SymLook<'a> {
    /// A request for `name` with both hashes filled in.
    pub fn new(name: &'a [u8]) -> Self {
        Self {
            name,
            hash_sysv: elf_hash(name),
            hash_gnu: gnu_hash(name),
            ver: None,
            flags: SymFlags::default(),
        }
    }

    /// The same request with a version requirement.
    pub fn with_version(mut self, ver: VerRef<'a>) -> Self {
        self.ver = Some(ver);
        self
    }

    /// The same request with extra flags.
    pub fn with_flags(mut self, flags: SymFlags) -> Self {
        self.flags = self.flags.union(flags);
        self
    }
}

/// A resolved definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymHit {
    /// The defining object.
    pub obj: ObjId,
    /// The symbol table entry.
    pub sym: crate::elf::Sym,
    /// `bias + st_value` for a normal symbol, `st_value` for a TLS symbol.
    pub value: u64,
    /// Whether the definition is `STT_GNU_IFUNC`.
    pub is_ifunc: bool,
    /// Whether the definition is `STT_TLS`.
    pub is_tls: bool,
}

/// A validated SysV hash table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SysvHash {
    /// `nchain`/`nbucket` header values.
    pub nbucket: u32,
    /// Number of chains, which is also the dynamic symbol count.
    pub nchain: u32,
    /// Absolute address of the header (two `u32` words).
    pub addr: u64,
}

/// A validated GNU hash table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GnuHash {
    /// `nbuckets`.
    pub nbuckets: u32,
    /// `symoffset`: index of the first hashed symbol.
    pub symoffset: u32,
    /// Number of bloom words (a power of two).
    pub bloom_size: u32,
    /// `bloom_shift`.
    pub bloom_shift: u32,
    /// Absolute address of the bloom word array.
    pub bloom: u64,
    /// Absolute address of the bucket array.
    pub buckets: u64,
    /// Absolute address of the chain array (the word for `symoffset`).
    pub chains: u64,
}

fn word32(mem: &dyn MemView, addr: u64) -> Result<u32, LookupError> {
    let bytes = mem.read(addr, 4).ok_or(LookupError::BadTable)?;
    let array: [u8; 4] = bytes.try_into().map_err(|_| LookupError::BadTable)?;
    Ok(u32::from_le_bytes(array))
}

fn word64(mem: &dyn MemView, addr: u64) -> Result<u64, LookupError> {
    let bytes = mem.read(addr, 8).ok_or(LookupError::BadTable)?;
    let array: [u8; 8] = bytes.try_into().map_err(|_| LookupError::BadTable)?;
    Ok(u64::from_le_bytes(array))
}

fn word16(mem: &dyn MemView, addr: u64) -> Result<u16, LookupError> {
    let bytes = mem.read(addr, 2).ok_or(LookupError::BadTable)?;
    let array: [u8; 2] = bytes.try_into().map_err(|_| LookupError::BadTable)?;
    Ok(u16::from_le_bytes(array))
}

/// The `DT_VERSYM` word for `symnum`.
pub(crate) fn versym(obj: &ObjView, mem: &dyn MemView, symnum: u32) -> Result<u16, LookupError> {
    let base = obj.versym.ok_or(LookupError::BadVersion)?;
    let offset = u64::from(symnum)
        .checked_mul(2)
        .ok_or(LookupError::BadVersion)?;
    word16(
        mem,
        base.checked_add(offset).ok_or(LookupError::BadVersion)?,
    )
}

/// Find the `VERDEF` record with `index`; returns its name offset, hash and flags.
pub(crate) fn version_definition(
    obj: &ObjView,
    mem: &dyn MemView,
    index: u16,
) -> Result<Option<(u32, u32, u16)>, LookupError> {
    let Some((base, count)) = obj.verdef else {
        return Ok(None);
    };
    let mut offset = 0u64;
    for _ in 0..count.min(MAX_VERSIONS as u32) {
        let address = base.checked_add(offset).ok_or(LookupError::BadVersion)?;
        let bytes = mem.read(address, 20).ok_or(LookupError::BadVersion)?;
        let record = crate::elf::Verdef::parse(bytes).map_err(|_| LookupError::BadVersion)?;
        if record.ndx & VER_NDX_MASK == index {
            let aux_address = address
                .checked_add(u64::from(record.aux))
                .ok_or(LookupError::BadVersion)?;
            let aux_bytes = mem.read(aux_address, 8).ok_or(LookupError::BadVersion)?;
            let aux = crate::elf::Verdaux::parse(aux_bytes).map_err(|_| LookupError::BadVersion)?;
            return Ok(Some((aux.name, record.hash, record.flags)));
        }
        if record.next == 0 {
            break;
        }
        offset = offset
            .checked_add(u64::from(record.next))
            .ok_or(LookupError::BadVersion)?;
    }
    Ok(None)
}

/// The highest version index used by the object's definition or requirement tables.
pub(crate) fn max_version(obj: &ObjView, mem: &dyn MemView) -> Result<u16, LookupError> {
    let mut maximum = 0u16;
    if let Some((base, count)) = obj.verdef {
        let mut offset = 0u64;
        for _ in 0..count.min(MAX_VERSIONS as u32) {
            let address = base.checked_add(offset).ok_or(LookupError::BadVersion)?;
            let bytes = mem.read(address, 20).ok_or(LookupError::BadVersion)?;
            let record = crate::elf::Verdef::parse(bytes).map_err(|_| LookupError::BadVersion)?;
            let index = record.ndx & VER_NDX_MASK;
            maximum = maximum.max(index);
            if record.next == 0 {
                break;
            }
            offset = offset
                .checked_add(u64::from(record.next))
                .ok_or(LookupError::BadVersion)?;
        }
    }
    if let Some((base, count)) = obj.verneed {
        let mut offset = 0u64;
        for _ in 0..count.min(MAX_VERSIONS as u32) {
            let address = base.checked_add(offset).ok_or(LookupError::BadVersion)?;
            let bytes = mem.read(address, 16).ok_or(LookupError::BadVersion)?;
            let record = crate::elf::Verneed::parse(bytes).map_err(|_| LookupError::BadVersion)?;
            let mut aux_offset = u64::from(record.aux);
            for _ in 0..record.cnt.min(MAX_VERSIONS as u16) {
                let aux_address = address
                    .checked_add(aux_offset)
                    .ok_or(LookupError::BadVersion)?;
                let aux_bytes = mem.read(aux_address, 16).ok_or(LookupError::BadVersion)?;
                let aux =
                    crate::elf::Vernaux::parse(aux_bytes).map_err(|_| LookupError::BadVersion)?;
                maximum = maximum.max(aux.other & VER_NDX_MASK);
                if aux.next == 0 {
                    break;
                }
                aux_offset = aux_offset
                    .checked_add(u64::from(aux.next))
                    .ok_or(LookupError::BadVersion)?;
            }
            if record.next == 0 {
                break;
            }
            offset = offset
                .checked_add(u64::from(record.next))
                .ok_or(LookupError::BadVersion)?;
        }
    }
    Ok(maximum)
}

/// Whether this object carries one of the names in its `DT_SONAME`/alias list.
fn object_matches(obj: &ObjView, mem: &dyn MemView, name: &[u8]) -> bool {
    obj.names
        .iter()
        .any(|offset| matches!(obj.name_at(mem, *offset), Ok(text) if text == name))
}

enum Match {
    Accept,
    Remember,
    Reject,
}

fn match_symbol(
    req: &SymLook<'_>,
    obj: &ObjView,
    mem: &dyn MemView,
    symnum: u32,
) -> Result<Match, LookupError> {
    let sym = obj.sym(mem, symnum).ok_or(LookupError::BadSymbol)?;
    // A local symbol is not a definition for any lookup, including its own object (T-22).
    if sym.bind() == STB_LOCAL {
        return Ok(Match::Reject);
    }
    let kind = sym.typ();
    match kind {
        STT_FUNC | STT_NOTYPE | STT_OBJECT | crate::consts::STT_COMMON | STT_GNU_IFUNC => {
            if sym.value == 0 {
                return Ok(Match::Reject);
            }
        }
        STT_TLS => {}
        _ => return Ok(Match::Reject),
    }
    if sym.shndx == SHN_UNDEF {
        // An undefined function with a non-zero value is a PLT reference in the main program;
        // it is a definition only for the PLT lookup (T-23).
        let plt_function =
            req.flags.contains(SymFlags::IN_PLT) && sym.value != 0 && kind == STT_FUNC;
        if !plt_function {
            return Ok(Match::Reject);
        }
    }
    if kind == STT_TLS && req.flags.contains(SymFlags::NO_TLS) {
        return Ok(Match::Reject);
    }
    let text = obj
        .name_at(mem, sym.name)
        .map_err(|_| LookupError::BadString)?;
    if text != req.name {
        return Ok(Match::Reject);
    }
    if sym.visibility() == STV_HIDDEN || sym.visibility() == STV_INTERNAL {
        // Hidden and internal definitions are not visible to another object; only a lookup in
        // the defining object itself may see them (the scope walker sets `SELF` for `refobj`).
        if !req.flags.contains(SymFlags::SELF) {
            return Ok(Match::Reject);
        }
    }

    let Some(requested) = req.ver else {
        let Some(_) = obj.versym else {
            return Ok(Match::Accept);
        };
        let raw = versym(obj, mem, symnum)?;
        let index = raw & VER_NDX_MASK;
        if index > max_version(obj, mem)? {
            return Err(LookupError::BadVersion);
        }
        if index == VER_NDX_GIVEN {
            // The first version node after the object became versioned is the base version; a
            // hidden bit still hides it from an unversioned reference.
            if raw & VER_NDX_HIDDEN == 0 {
                return Ok(Match::Accept);
            }
            return Ok(Match::Reject);
        }
        if index >= VER_NDX_GIVEN {
            if raw & VER_NDX_HIDDEN == 0 {
                return Ok(Match::Remember);
            }
            return Ok(Match::Reject);
        }
        return Ok(Match::Accept);
    };

    if obj.versym.is_none() {
        if object_matches(obj, mem, requested.file.unwrap_or(requested.name)) {
            return Ok(Match::Reject);
        }
        return Ok(Match::Accept);
    }
    let raw = versym(obj, mem, symnum)?;
    let index = raw & VER_NDX_MASK;
    if index > max_version(obj, mem)? {
        return Err(LookupError::BadVersion);
    }
    let mismatched = match version_definition(obj, mem, index)? {
        Some((name, hash, flags)) => {
            hash != requested.hash
                || obj.name_at(mem, name).map_err(|_| LookupError::BadString)? != requested.name
                || flags & VER_FLG_BASE != 0
        }
        None => true,
    };
    if mismatched && (index >= VER_NDX_GIVEN || raw & VER_NDX_HIDDEN != 0) {
        return Ok(Match::Reject);
    }
    Ok(Match::Accept)
}

/// Whether `symnum` in `obj` is a definition of the looked-up name (the version rules included).
pub fn matched_symbol(
    req: &SymLook<'_>,
    obj: &ObjView,
    mem: &dyn MemView,
    symnum: u32,
) -> Result<bool, LookupError> {
    Ok(matches!(
        match_symbol(req, obj, mem, symnum)?,
        Match::Accept
    ))
}

fn hit(id: ObjId, obj: &ObjView, sym: crate::elf::Sym) -> SymHit {
    let is_tls = sym.typ() == STT_TLS;
    SymHit {
        obj: id,
        sym,
        value: if is_tls {
            sym.value
        } else {
            obj.bias.saturating_add(sym.value)
        },
        is_ifunc: sym.typ() == STT_GNU_IFUNC,
        is_tls,
    }
}

/// Look the request up in one object; GNU table first when present, else SysV.
pub fn lookup_obj(
    id: ObjId,
    obj: &ObjView,
    mem: &dyn MemView,
    req: &SymLook<'_>,
) -> Result<Option<SymHit>, LookupError> {
    if let Some(table) = obj.hash_gnu {
        lookup_gnu(id, obj, mem, req, table)
    } else if let Some(table) = obj.hash_sysv {
        lookup_sysv(id, obj, mem, req, table)
    } else {
        Err(LookupError::BadTable)
    }
}

fn lookup_sysv(
    id: ObjId,
    obj: &ObjView,
    mem: &dyn MemView,
    req: &SymLook<'_>,
    table: SysvHash,
) -> Result<Option<SymHit>, LookupError> {
    if table.nbucket == 0 || table.nchain == 0 {
        return Err(LookupError::BadTable);
    }
    let bucket_index = req
        .hash_sysv
        .checked_rem(table.nbucket)
        .ok_or(LookupError::BadTable)?;
    let buckets = table.addr.checked_add(8).ok_or(LookupError::BadTable)?;
    let index_address = buckets
        .checked_add(
            u64::from(bucket_index)
                .checked_mul(4)
                .ok_or(LookupError::BadTable)?,
        )
        .ok_or(LookupError::BadTable)?;
    let mut symnum = word32(mem, index_address)?;
    let chains = buckets
        .checked_add(
            u64::from(table.nbucket)
                .checked_mul(4)
                .ok_or(LookupError::BadTable)?,
        )
        .ok_or(LookupError::BadTable)?;
    let mut remembered = None;
    let mut remember_count = 0u32;
    let mut steps = 0u32;
    while symnum != 0 {
        if symnum >= table.nchain {
            return Err(LookupError::BadTable);
        }
        steps = steps.saturating_add(1);
        if steps > table.nchain {
            return Err(LookupError::ChainLoop);
        }
        match match_symbol(req, obj, mem, symnum)? {
            Match::Accept => {
                let sym = obj.sym(mem, symnum).ok_or(LookupError::BadSymbol)?;
                return Ok(Some(hit(id, obj, sym)));
            }
            Match::Remember => {
                if remembered.is_none() {
                    remembered = Some(symnum);
                }
                remember_count = remember_count.saturating_add(1);
            }
            Match::Reject => {}
        }
        let next = chains
            .checked_add(
                u64::from(symnum)
                    .checked_mul(4)
                    .ok_or(LookupError::BadTable)?,
            )
            .ok_or(LookupError::BadTable)?;
        symnum = word32(mem, next)?;
    }
    if remember_count == 1
        && let Some(index) = remembered
    {
        let sym = obj.sym(mem, index).ok_or(LookupError::BadSymbol)?;
        return Ok(Some(hit(id, obj, sym)));
    }
    Ok(None)
}

fn lookup_gnu(
    id: ObjId,
    obj: &ObjView,
    mem: &dyn MemView,
    req: &SymLook<'_>,
    table: GnuHash,
) -> Result<Option<SymHit>, LookupError> {
    if table.nbuckets == 0 || table.bloom_size == 0 || !table.bloom_size.is_power_of_two() {
        return Err(LookupError::BadTable);
    }
    if table.bloom_shift >= 32 {
        return Err(LookupError::BadTable);
    }
    let mask = table.bloom_size.saturating_sub(1);
    let word_index = (req.hash_gnu / 64) & mask;
    let bloom_address = table
        .bloom
        .checked_add(
            u64::from(word_index)
                .checked_mul(8)
                .ok_or(LookupError::BadTable)?,
        )
        .ok_or(LookupError::BadTable)?;
    let bloom_word = word64(mem, bloom_address)?;
    let high_bit = req.hash_gnu & 63;
    let low_bit = (req.hash_gnu >> table.bloom_shift) & 63;
    if (bloom_word >> high_bit) & (bloom_word >> low_bit) & 1 == 0 {
        return Ok(None);
    }
    let bucket_address = table
        .buckets
        .checked_add(
            u64::from(
                req.hash_gnu
                    .checked_rem(table.nbuckets)
                    .ok_or(LookupError::BadTable)?,
            )
            .checked_mul(4)
            .ok_or(LookupError::BadTable)?,
        )
        .ok_or(LookupError::BadTable)?;
    let mut symnum = word32(mem, bucket_address)?;
    if symnum == 0 {
        return Ok(None);
    }
    if symnum < table.symoffset {
        return Err(LookupError::BadTable);
    }
    let mut remembered = None;
    let mut remember_count = 0u32;
    let mut steps = 0usize;
    loop {
        if steps >= MAX_SYMBOLS {
            return Err(LookupError::ChainLoop);
        }
        steps = steps.saturating_add(1);
        let offset = symnum
            .checked_sub(table.symoffset)
            .ok_or(LookupError::BadTable)?;
        let chain_address = table
            .chains
            .checked_add(
                u64::from(offset)
                    .checked_mul(4)
                    .ok_or(LookupError::BadTable)?,
            )
            .ok_or(LookupError::BadTable)?;
        let chain_word = word32(mem, chain_address)?;
        if (chain_word ^ req.hash_gnu) >> 1 == 0 {
            match match_symbol(req, obj, mem, symnum)? {
                Match::Accept => {
                    let sym = obj.sym(mem, symnum).ok_or(LookupError::BadSymbol)?;
                    return Ok(Some(hit(id, obj, sym)));
                }
                Match::Remember => {
                    if remembered.is_none() {
                        remembered = Some(symnum);
                    }
                    remember_count = remember_count.saturating_add(1);
                }
                Match::Reject => {}
            }
        }
        if chain_word & 1 != 0 {
            break;
        }
        symnum = symnum.checked_add(1).ok_or(LookupError::BadTable)?;
    }
    if remember_count == 1
        && let Some(index) = remembered
    {
        let sym = obj.sym(mem, index).ok_or(LookupError::BadSymbol)?;
        return Ok(Some(hit(id, obj, sym)));
    }
    Ok(None)
}

/// The size in bytes of a SysV hash table with `nbucket` buckets and `nchain` chains.
pub fn sysv_hash_bytes(nbucket: u32, nchain: u32) -> Option<u64> {
    let words = u64::from(nbucket).checked_add(u64::from(nchain))?;
    words.checked_add(2)?.checked_mul(4)
}

/// Read one dynamic symbol of an object.
pub(crate) fn read_symbol(obj: &ObjView, mem: &dyn MemView, index: u32) -> Option<crate::elf::Sym> {
    let offset = u64::from(index).checked_mul(u64::try_from(SYM_SIZE).ok()?)?;
    let address = obj.symtab.checked_add(offset)?;
    let bytes = mem.read(address, SYM_SIZE)?;
    crate::elf::Sym::parse(bytes).ok()
}

#[cfg(test)]
#[path = "lookup_ut.rs"]
mod tests;
