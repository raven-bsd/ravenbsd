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
//! Scope walking: which objects are searched, in which order.
//!
//! The caller owns the loaded-object table and the lists; this module only decides the order and
//! calls [`lookup_obj`]. The `donelist` is a plain `Vec<ObjId>` with a linear scan (bounded by the
//! number of objects).

use alloc::vec::Vec;

use crate::consts::{STB_WEAK, STV_PROTECTED};
use crate::errors::LookupError;
use crate::limits::MAX_SCOPE_OBJECTS;
use crate::lookup::{SymFlags, SymHit, SymLook, lookup_obj};
use crate::mem::MemView;
use crate::object::ObjView;

/// An object reference. The loader allocates the ids.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjId(pub u32);

/// A read-only view of the loaded-object table (C62 owns the real list).
pub trait ObjTable {
    /// The digested view of `id`.
    fn view(&self, id: ObjId) -> Option<&ObjView>;
    /// `DT_NEEDED` order of the loaded direct dependencies.
    fn deps(&self, id: ObjId) -> &[ObjId];
    /// The `dlopen` DAGs that contain the object.
    fn dldags(&self, id: ObjId) -> &[ObjId];
    /// `RTLD_DEEPBIND` was requested for the object.
    fn deepbind(&self, id: ObjId) -> bool;
}

/// The global scope.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    /// The main program.
    pub main: ObjId,
    /// `LD_PRELOAD` objects, in order, searched before the main DAG.
    pub preload: Vec<ObjId>,
    /// `RTLD_GLOBAL` objects, in load order; each root's DAG is searched.
    pub global: Vec<ObjId>,
    /// The dynamic linker itself, searched last.
    pub rtld: Option<ObjId>,
}

impl Scope {
    /// A scope with only a main program.
    pub fn new(main: ObjId) -> Self {
        Self {
            main,
            preload: Vec::new(),
            global: Vec::new(),
            rtld: None,
        }
    }
}

fn is_weak(hit: &SymHit) -> bool {
    hit.sym.bind() == STB_WEAK
}

fn prefer(
    current: Option<SymHit>,
    candidate: Option<SymHit>,
    dynamic_weak: bool,
) -> Option<SymHit> {
    match (current, candidate) {
        (None, candidate) => candidate,
        (Some(current), None) => Some(current),
        (Some(current), Some(candidate)) => {
            if dynamic_weak && is_weak(&current) && !is_weak(&candidate) {
                Some(candidate)
            } else {
                Some(current)
            }
        }
    }
}

fn is_final(hit: &Option<SymHit>, dynamic_weak: bool) -> bool {
    match hit {
        None => false,
        Some(hit) => !dynamic_weak || !is_weak(hit),
    }
}

/// The root and its dependencies, breadth-first, each object once.
pub fn dag_members(table: &dyn ObjTable, root: ObjId) -> Vec<ObjId> {
    let mut members = Vec::new();
    let mut queue = Vec::new();
    queue.push(root);
    while let Some(id) = queue.first().copied() {
        queue.remove(0);
        if members.contains(&id) || members.len() >= MAX_SCOPE_OBJECTS {
            continue;
        }
        members.push(id);
        queue.extend_from_slice(table.deps(id));
    }
    members
}

fn lookup_in(
    table: &dyn ObjTable,
    mem: &dyn MemView,
    id: ObjId,
    req: &SymLook<'_>,
    self_object: bool,
) -> Result<Option<SymHit>, LookupError> {
    let Some(view) = table.view(id) else {
        return Ok(None);
    };
    let request = if self_object {
        req.with_flags(SymFlags::SELF)
    } else {
        *req
    };
    lookup_obj(id, view, mem, &request)
}

/// Search one ordered list, skipping objects already in `done`.
pub fn symlook_list(
    table: &dyn ObjTable,
    mem: &dyn MemView,
    list: &[ObjId],
    req: &SymLook<'_>,
    done: &mut Vec<ObjId>,
) -> Result<Option<SymHit>, LookupError> {
    symlook_list_dw(table, mem, list, req, done, true)
}

fn symlook_list_dw(
    table: &dyn ObjTable,
    mem: &dyn MemView,
    list: &[ObjId],
    req: &SymLook<'_>,
    done: &mut Vec<ObjId>,
    dynamic_weak: bool,
) -> Result<Option<SymHit>, LookupError> {
    let mut found = None;
    for id in list {
        if done.contains(id) {
            continue;
        }
        done.push(*id);
        let hit = lookup_in(table, mem, *id, req, false)?;
        found = prefer(found, hit, dynamic_weak);
    }
    Ok(found)
}

/// Search every DAG of the global scope, after `preload` and the main DAG.
pub fn symlook_global(
    table: &dyn ObjTable,
    mem: &dyn MemView,
    scope: &Scope,
    req: &SymLook<'_>,
    done: &mut Vec<ObjId>,
) -> Result<Option<SymHit>, LookupError> {
    symlook_global_dw(table, mem, scope, req, done, true)
}

fn symlook_global_dw(
    table: &dyn ObjTable,
    mem: &dyn MemView,
    scope: &Scope,
    req: &SymLook<'_>,
    done: &mut Vec<ObjId>,
    dynamic_weak: bool,
) -> Result<Option<SymHit>, LookupError> {
    let mut found = None;
    found = prefer(
        found,
        symlook_list_dw(table, mem, &scope.preload, req, done, dynamic_weak)?,
        dynamic_weak,
    );
    found = prefer(
        found,
        symlook_list_dw(
            table,
            mem,
            &dag_members(table, scope.main),
            req,
            done,
            dynamic_weak,
        )?,
        dynamic_weak,
    );
    for root in &scope.global {
        if is_final(&found, dynamic_weak) {
            break;
        }
        found = prefer(
            found,
            symlook_list_dw(
                table,
                mem,
                &dag_members(table, *root),
                req,
                done,
                dynamic_weak,
            )?,
            dynamic_weak,
        );
    }
    Ok(found)
}

/// A `dlsym(handle)`-style search: `root` and its dependencies, breadth-first.
pub fn symlook_needed(
    table: &dyn ObjTable,
    mem: &dyn MemView,
    root: ObjId,
    req: &SymLook<'_>,
    done: &mut Vec<ObjId>,
) -> Result<Option<SymHit>, LookupError> {
    symlook_list(table, mem, &dag_members(table, root), req, done)
}

/// The full lookup a relocation or `dlsym(RTLD_DEFAULT)` performs.
pub fn symlook_default(
    table: &dyn ObjTable,
    mem: &dyn MemView,
    scope: &Scope,
    req: &SymLook<'_>,
    refobj: ObjId,
    dynamic_weak: bool,
) -> Result<Option<SymHit>, LookupError> {
    let mut done = Vec::new();
    let mut found = None;
    let symbolic = table
        .view(refobj)
        .map(|view| view.symbolic)
        .unwrap_or(false);
    let deepbind = table.deepbind(refobj);

    let own = lookup_in(table, mem, refobj, req, true)?;
    if let Some(hit) = own
        && (symbolic || hit.sym.visibility() == STV_PROTECTED || deepbind)
    {
        found = Some(hit);
    }
    if symbolic || found.is_some() || deepbind {
        done.push(refobj);
    }

    if !deepbind {
        found = prefer(
            found,
            symlook_global_dw(table, mem, scope, req, &mut done, dynamic_weak)?,
            dynamic_weak,
        );
    }

    for root in table.dldags(refobj) {
        if is_final(&found, dynamic_weak) {
            break;
        }
        found = prefer(
            found,
            symlook_list_dw(
                table,
                mem,
                &dag_members(table, *root),
                req,
                &mut done,
                dynamic_weak,
            )?,
            dynamic_weak,
        );
    }

    if deepbind {
        found = prefer(
            found,
            symlook_global_dw(table, mem, scope, req, &mut done, dynamic_weak)?,
            dynamic_weak,
        );
    }

    if !is_final(&found, dynamic_weak)
        && let Some(rtld) = scope.rtld
        && !done.contains(&rtld)
    {
        found = prefer(
            found,
            lookup_in(table, mem, rtld, req, false)?,
            dynamic_weak,
        );
    }
    Ok(found)
}

#[cfg(test)]
#[path = "scope_ut.rs"]
mod tests;
