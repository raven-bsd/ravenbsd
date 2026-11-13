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
//! Symbol versioning: `DT_VERDEF`, `DT_VERNEED` and `DT_VERSYM`.
//!
//! The rules are the donor's (T-24): the `DT_VERSYM` word carries the index in its low 15 bits
//! and a hidden bit (`@` rather than `@@`); index 0 is local and 1 is global; a versioned
//! reference matches a definition whose name and hash agree; an unversioned reference matches the
//! default (non-hidden) definition; a `VER_FLG_WEAK` requirement may be missing.

use alloc::vec::Vec;

use crate::consts::{
    VER_DEF_CURRENT, VER_FLG_BASE, VER_FLG_WEAK, VER_NDX_HIDDEN, VER_NDX_MASK, VER_NEED_CURRENT,
};
use crate::elf::{Verdef, Vernaux, Verneed};
use crate::errors::VersionError;
use crate::limits::MAX_VERSIONS;
use crate::lookup::versym;
use crate::mem::MemView;
use crate::object::ObjView;
use crate::scope::{ObjId, ObjTable};

/// One entry of the version table indexed by `DT_VERSYM`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerEntry {
    /// String-table offset of the version name.
    pub name_off: u32,
    /// The version hash.
    pub hash: u32,
    /// For a requirement, the string-table offset of the dependency name.
    pub file_off: Option<u32>,
    /// `VER_FLG_WEAK`.
    pub weak: bool,
    /// The version index.
    pub index: u16,
    /// The hidden bit was set on this reference.
    pub hidden: bool,
}

fn read_verneed_aux(
    mem: &dyn MemView,
    base: u64,
    record: &Verneed,
) -> Result<Vec<Vernaux>, VersionError> {
    let mut auxes = Vec::new();
    let mut offset = u64::from(record.aux);
    for _ in 0..record.cnt.min(MAX_VERSIONS as u16) {
        let address = base.checked_add(offset).ok_or(VersionError::BadTable)?;
        let bytes = mem.read(address, 16).ok_or(VersionError::BadTable)?;
        let aux = Vernaux::parse(bytes).map_err(|_| VersionError::BadTable)?;
        auxes.push(aux);
        if aux.next == 0 {
            break;
        }
        offset = offset
            .checked_add(u64::from(aux.next))
            .ok_or(VersionError::BadTable)?;
    }
    if auxes.is_empty() {
        return Err(VersionError::BadTable);
    }
    Ok(auxes)
}

/// The version entry for `symnum`, or `None` for index 0/1.
pub fn fetch_ventry(
    obj: &ObjView,
    mem: &dyn MemView,
    symnum: u32,
) -> Result<Option<VerEntry>, VersionError> {
    let Some(_) = obj.versym else {
        return Ok(None);
    };
    let raw = versym(obj, mem, symnum).map_err(|_| VersionError::BadTable)?;
    let index = raw & VER_NDX_MASK;
    if index < 2 {
        return Ok(None);
    }
    let hidden = raw & VER_NDX_HIDDEN != 0;
    let mut name_off = 0u32;
    let mut hash = 0u32;
    let mut file_off = None;
    let mut weak = false;
    let mut found = false;

    if let Some((base, count)) = obj.verneed {
        let mut offset = 0u64;
        for _ in 0..count.min(MAX_VERSIONS as u32) {
            let address = base.checked_add(offset).ok_or(VersionError::BadTable)?;
            let bytes = mem.read(address, 16).ok_or(VersionError::BadTable)?;
            let record = Verneed::parse(bytes).map_err(|_| VersionError::BadTable)?;
            if record.version != VER_NEED_CURRENT {
                return Err(VersionError::BadTable);
            }
            for aux in read_verneed_aux(mem, address, &record)? {
                if aux.other & VER_NDX_MASK == index {
                    name_off = aux.name;
                    hash = aux.hash;
                    weak = aux.flags & VER_FLG_WEAK != 0;
                    file_off = Some(record.file);
                    found = true;
                }
            }
            if found || record.next == 0 {
                break;
            }
            offset = offset
                .checked_add(u64::from(record.next))
                .ok_or(VersionError::BadTable)?;
        }
    }
    if !found && let Some((base, count)) = obj.verdef {
        let mut offset = 0u64;
        for _ in 0..count.min(MAX_VERSIONS as u32) {
            let address = base.checked_add(offset).ok_or(VersionError::BadTable)?;
            let bytes = mem.read(address, 20).ok_or(VersionError::BadTable)?;
            let record = Verdef::parse(bytes).map_err(|_| VersionError::BadTable)?;
            if record.version != VER_DEF_CURRENT {
                return Err(VersionError::BadTable);
            }
            if record.ndx & VER_NDX_MASK == index {
                let aux_address = address
                    .checked_add(u64::from(record.aux))
                    .ok_or(VersionError::BadTable)?;
                let aux_bytes = mem.read(aux_address, 8).ok_or(VersionError::BadTable)?;
                let aux =
                    crate::elf::Verdaux::parse(aux_bytes).map_err(|_| VersionError::BadTable)?;
                name_off = aux.name;
                hash = record.hash;
                found = true;
            }
            if found || record.next == 0 {
                break;
            }
            offset = offset
                .checked_add(u64::from(record.next))
                .ok_or(VersionError::BadTable)?;
        }
    }
    if !found {
        return Err(VersionError::BadTable);
    }
    Ok(Some(VerEntry {
        name_off,
        hash,
        file_off,
        weak,
        index,
        hidden,
    }))
}

/// Find the loaded object that carries `vn_file` as a `DT_SONAME` or alias.
///
/// The object ids are dense (`0..MAX_SCOPE_OBJECTS`); the scan stops at the first unloaded id
/// after the first loaded one. The referencing object never matches itself.
pub fn locate_dependency(
    obj: &ObjView,
    mem: &dyn MemView,
    vn_file: &[u8],
    table: &dyn ObjTable,
    names: &dyn Fn(ObjId) -> Vec<Vec<u8>>,
) -> Option<ObjId> {
    let _ = mem;
    let mut seen = false;
    for raw in 0..crate::limits::MAX_SCOPE_OBJECTS as u32 {
        let id = ObjId(raw);
        let Some(view) = table.view(id) else {
            if seen {
                break;
            }
            continue;
        };
        seen = true;
        if core::ptr::eq(view, obj) {
            continue;
        }
        if names(id).iter().any(|name| name == vn_file) {
            return Some(id);
        }
    }
    None
}

fn dependency_defines(
    dep: &ObjView,
    mem: &dyn MemView,
    name: &[u8],
    hash: u32,
) -> Result<bool, VersionError> {
    let Some((base, count)) = dep.verdef else {
        return Ok(false);
    };
    let mut offset = 0u64;
    for _ in 0..count.min(MAX_VERSIONS as u32) {
        let address = base.checked_add(offset).ok_or(VersionError::BadTable)?;
        let bytes = mem.read(address, 28).ok_or(VersionError::BadTable)?;
        let record = Verdef::parse(bytes).map_err(|_| VersionError::BadTable)?;
        if record.version != VER_DEF_CURRENT {
            return Err(VersionError::BadTable);
        }
        if record.flags & VER_FLG_BASE == 0 && record.hash == hash {
            let aux_address = address
                .checked_add(u64::from(record.aux))
                .ok_or(VersionError::BadTable)?;
            let aux_bytes = mem.read(aux_address, 8).ok_or(VersionError::BadTable)?;
            let aux = crate::elf::Verdaux::parse(aux_bytes).map_err(|_| VersionError::BadTable)?;
            if dep
                .name_at(mem, aux.name)
                .map_err(|_| VersionError::BadTable)?
                == name
            {
                return Ok(true);
            }
        }
        if record.next == 0 {
            break;
        }
        offset = offset
            .checked_add(u64::from(record.next))
            .ok_or(VersionError::BadTable)?;
    }
    Ok(false)
}

/// Check every `DT_VERNEED` requirement of `obj` against the located dependency.
pub fn verify_object_versions(
    obj: &ObjView,
    mem: &dyn MemView,
    locate: &dyn Fn(&[u8]) -> Option<ObjId>,
    table: &dyn ObjTable,
) -> Result<(), VersionError> {
    let Some((base, count)) = obj.verneed else {
        return Ok(());
    };
    let mut offset = 0u64;
    let mut seen = Vec::new();
    for _ in 0..count.min(MAX_VERSIONS as u32) {
        if seen.contains(&offset) {
            return Err(VersionError::BadTable);
        }
        seen.push(offset);
        let address = base.checked_add(offset).ok_or(VersionError::BadTable)?;
        let bytes = mem.read(address, 16).ok_or(VersionError::BadTable)?;
        let record = Verneed::parse(bytes).map_err(|_| VersionError::BadTable)?;
        if record.version != VER_NEED_CURRENT {
            return Err(VersionError::BadTable);
        }
        let file = obj
            .name_at(mem, record.file)
            .map_err(|_| VersionError::BadTable)?;
        let dependency = locate(file).ok_or(VersionError::MissingDependency)?;
        let dep_view = table
            .view(dependency)
            .ok_or(VersionError::MissingDependency)?;
        for aux in read_verneed_aux(mem, address, &record)? {
            let name = obj
                .name_at(mem, aux.name)
                .map_err(|_| VersionError::BadTable)?;
            if !dependency_defines(dep_view, mem, name, aux.hash)? {
                if aux.flags & VER_FLG_WEAK != 0 {
                    continue;
                }
                return Err(VersionError::Unsatisfied {
                    version: aux.name,
                    library: record.file,
                    required_by: 0,
                });
            }
        }
        if record.next == 0 {
            break;
        }
        offset = offset
            .checked_add(u64::from(record.next))
            .ok_or(VersionError::BadTable)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "version_ut.rs"]
mod tests;
