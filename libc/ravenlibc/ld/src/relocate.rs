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
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @
// 133344405f532bd54ff3629a77d4c4e062f01fdb
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
//! Relocation phases, the symbol resolver and the RELRO/IFUNC order.

use raven_libc_elf::{
    BindMode, IfuncArg, MemView, ObjId, ObjTable, Phase, RelocError, SymDef, SymLook, SymResolver,
    apply_object, lookup::SymFlags, scope::symlook_default,
};

use crate::loadobj::{LoadError, Loader, ObjectTable};
use crate::mapper::{Platform, Prot};

/// The resolver handed to C52's relocation engine.
struct Resolver<'a, M: MemView> {
    table: &'a ObjectTable<'a>,
    mem: &'a M,
    scope: &'a raven_libc_elf::Scope,
    dynamic_weak: bool,
}

impl<M: MemView> SymResolver for Resolver<'_, M> {
    fn resolve(
        &mut self,
        referencing: ObjId,
        sym_index: u32,
    ) -> Result<Option<SymDef>, RelocError> {
        let view = self.table.view(referencing).ok_or(RelocError::BadSymbol)?;
        let sym = view.sym(self.mem, sym_index).ok_or(RelocError::BadSymbol)?;
        let name = view
            .name_at(self.mem, sym.name)
            .map_err(|_| RelocError::BadSymbol)?;
        let request = SymLook::new(name).with_flags(SymFlags::NO_TLS);
        let hit = symlook_default(
            self.table,
            self.mem,
            self.scope,
            &request,
            referencing,
            self.dynamic_weak,
        )
        .map_err(|_| RelocError::BadSymbol)?;
        match hit {
            Some(hit) => Ok(Some(SymDef {
                obj: hit.obj,
                value: hit.value,
                st_value: hit.sym.value,
                size: hit.sym.size,
                is_ifunc: hit.is_ifunc,
                is_tls: hit.is_tls,
                weak_undef: false,
            })),
            None if sym.bind() == raven_libc_elf::consts::STB_WEAK => Ok(None),
            None => Err(RelocError::Undefined(sym_index)),
        }
    }
}

impl<P: Platform, M: MemView, A: raven_libc_elf::RelocApplier> Loader<P, M, A> {
    /// Relocate every loaded object: non-PLT, RELR, PLT, then the deferred IFUNC pass.
    pub fn relocate_all(&mut self) -> Result<(), LoadError> {
        for index in 0..self.count {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            let id = ObjId(index);
            let Some(object) = self.object(id) else {
                continue;
            };
            if object.relocated {
                continue;
            }
            self.relocate_one(id, Phase::NonPlt)?;
            self.relocate_one(id, Phase::Relr)?;
            self.relocate_one(id, Phase::Plt)?;
            if let Some(object) = self.object_mut(id) {
                object.relocated = true;
            }
        }
        self.ifunc_pass()?;
        self.enforce_relro()
    }

    fn relocate_one(&mut self, id: ObjId, phase: Phase) -> Result<(), LoadError> {
        let table = ObjectTable {
            objects: &self.objects,
        };
        let view = table.view(id).ok_or(LoadError::TooManyObjects)?;
        let mem = &self.mem;
        let scope = &self.scope;
        let applier = &mut self.applier;
        let mut resolver = Resolver {
            table: &table,
            mem,
            scope,
            dynamic_weak: true,
        };
        let result = apply_object(
            self.arch,
            id,
            view,
            self.scope.main == id,
            mem,
            &mut resolver,
            applier,
            BindMode::Now,
            phase,
            &IfuncArg {
                hwcap: 0,
                hwcap2: 0,
            },
        );
        if let Err(error) = result {
            let phase_name = match phase {
                Phase::NonPlt => "non-plt",
                Phase::Relr => "relr",
                Phase::Plt => "plt",
                Phase::Ifunc => "ifunc",
            };
            self.error.set(format_args!("{phase_name}: {error:?}"));
            return Err(LoadError::Reloc(error));
        }
        Ok(())
    }

    /// The IFUNC/IRELATIVE pass, after every other relocation.
    pub fn ifunc_pass(&mut self) -> Result<(), LoadError> {
        for index in 0..self.count {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            let id = ObjId(index);
            let Some(object) = self.object(id) else {
                continue;
            };
            if object.ifunc_done {
                continue;
            }
            self.relocate_one(id, Phase::Ifunc)?;
            if let Some(object) = self.object_mut(id) {
                object.ifunc_done = true;
            }
        }
        Ok(())
    }

    /// Enforce `PT_GNU_RELRO` after the IFUNC pass.
    pub fn enforce_relro(&mut self) -> Result<(), LoadError> {
        for index in 0..self.count {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            let range = self
                .object(ObjId(index))
                .and_then(|object| object.view.relro);
            if let Some((start, end)) = range {
                let start = start & !0xfff;
                let len = end.saturating_sub(start);
                if len != 0 {
                    self.platform
                        .protect(start, len, Prot::READ)
                        .map_err(LoadError::Map)?;
                }
            }
        }
        Ok(())
    }
}
