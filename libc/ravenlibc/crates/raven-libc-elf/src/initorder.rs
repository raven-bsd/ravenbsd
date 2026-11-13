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
//! Init and fini order: dependencies first, each object once, cycles broken at the back edge.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::errors::DigestError;
use crate::limits::MAX_INIT_ARRAY;
use crate::mem::MemView;
use crate::object::ObjView;
use crate::scope::ObjId;

/// Which array to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitKind {
    /// `DT_PREINIT_ARRAY` (main program only).
    Preinit,
    /// `DT_INIT_ARRAY`.
    Init,
    /// `DT_FINI_ARRAY`.
    Fini,
}

fn visit(
    id: ObjId,
    deps: &dyn Fn(ObjId) -> Vec<ObjId>,
    state: &mut BTreeMap<ObjId, u8>,
    order: &mut Vec<ObjId>,
) {
    match state.get(&id) {
        // 1 = on the stack, 2 = emitted.
        Some(2) => return,
        Some(1) => return,
        _ => {}
    }
    state.insert(id, 1);
    for dependency in deps(id) {
        visit(dependency, deps, state, order);
    }
    state.insert(id, 2);
    order.push(id);
}

/// The init order of `roots`: every dependency before its dependent, `DT_NEEDED` order kept,
/// each object once, a cycle broken at the edge that would close it.
pub fn init_order(roots: &[ObjId], deps: &dyn Fn(ObjId) -> Vec<ObjId>) -> Vec<ObjId> {
    let mut state = BTreeMap::new();
    let mut order = Vec::new();
    for root in roots {
        visit(*root, deps, &mut state, &mut order);
    }
    order
}

/// The fini order: the exact reverse of `init`.
pub fn fini_order(init: &[ObjId]) -> Vec<ObjId> {
    let mut order = init.to_vec();
    order.reverse();
    order
}

/// The entries of one init/fini array, with the `0`, `1` and `u64::MAX` placeholders skipped
/// (A-ld-20). The values are link-time until relocation (`T-18`).
pub fn init_array_entries(
    obj: &ObjView,
    mem: &dyn MemView,
    which: InitKind,
) -> Result<Vec<u64>, DigestError> {
    let (address, size) = match which {
        InitKind::Preinit => obj.preinit_array,
        InitKind::Init => obj.init_array,
        InitKind::Fini => obj.fini_array,
    };
    if size == 0 {
        return Ok(Vec::new());
    }
    let count = usize::try_from(size / 8).map_err(|_| DigestError::BadSize)?;
    if count > MAX_INIT_ARRAY {
        return Err(DigestError::BadSize);
    }
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let offset = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_mul(8))
            .ok_or(DigestError::BadSize)?;
        let at = address.checked_add(offset).ok_or(DigestError::BadSize)?;
        let bytes = mem.read(at, 8).ok_or(DigestError::BadSize)?;
        let array: [u8; 8] = bytes.try_into().map_err(|_| DigestError::BadSize)?;
        let entry = u64::from_le_bytes(array);
        if entry != 0 && entry != 1 && entry != u64::MAX {
            entries.push(entry);
        }
    }
    Ok(entries)
}

#[cfg(test)]
#[path = "initorder_ut.rs"]
mod tests;
