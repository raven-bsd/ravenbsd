// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! `DT_INIT`/`DT_INIT_ARRAY`/`DT_FINI` ordering helpers.
//!
//! The actual indirect calls are in the platform entry glue; the core only produces the order
//! and the addresses, so host tests can prove the order without executing arbitrary code.

use raven_libc_elf::{
    ObjId, ObjTable, init_order, initorder::InitKind, initorder::init_array_entries,
};

use crate::loadobj::{Loader, MAX_OBJECTS};
use crate::mapper::Platform;

/// The addresses of one object's initialisers and finalisers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InitAddresses {
    /// `DT_INIT`.
    pub init: u64,
    /// `DT_INIT_ARRAY` address and count.
    pub init_array: (u64, u64),
    /// `DT_FINI`.
    pub fini: u64,
    /// `DT_FINI_ARRAY` address and count.
    pub fini_array: (u64, u64),
    /// `DT_PREINIT_ARRAY` address and count.
    pub preinit_array: (u64, u64),
}

impl<P: Platform, M: raven_libc_elf::MemView, A: raven_libc_elf::RelocApplier> Loader<P, M, A> {
    /// The initialiser addresses of one object.
    #[must_use]
    pub fn init_addresses(&self, id: ObjId) -> Option<InitAddresses> {
        let view = &self.object(id)?.view;
        Some(InitAddresses {
            init: view.init,
            init_array: view.init_array,
            fini: view.fini,
            fini_array: view.fini_array,
            preinit_array: view.preinit_array,
        })
    }

    /// Run `DT_INIT`, then `DT_INIT_ARRAY` in file order, when an object is loaded.
    pub fn run_init(&mut self, id: ObjId) -> bool {
        let Some(object) = self.object(id) else {
            return false;
        };
        let init = object.view.init;
        let entries =
            init_array_entries(&object.view, &self.mem, InitKind::Init).unwrap_or_default();
        if init != 0
            && let Ok(address) = usize::try_from(init)
        {
            self.platform.call_function(address);
        }
        for entry in entries.iter() {
            if let Ok(address) = usize::try_from(*entry) {
                self.platform.call_function(address);
            }
        }
        true
    }

    /// Run `DT_FINI_ARRAY` backwards, then the legacy `DT_FINI`, before an object unloads.
    pub fn run_fini(&mut self, id: ObjId) -> bool {
        let Some(object) = self.object(id) else {
            return false;
        };
        let entries =
            init_array_entries(&object.view, &self.mem, InitKind::Fini).unwrap_or_default();
        let fini = object.view.fini;
        for entry in entries.iter().rev() {
            if let Ok(address) = usize::try_from(*entry) {
                self.platform.call_function(address);
            }
        }
        if fini != 0
            && let Ok(address) = usize::try_from(fini)
        {
            self.platform.call_function(address);
        }
        true
    }

    /// The dependency-first initialisation order of the initial objects.
    #[must_use]
    pub fn initial_order(&self) -> [Option<ObjId>; MAX_OBJECTS] {
        let roots = [self.scope.main];
        let table = self.table();
        let deps = |id: ObjId| table.deps(id).to_vec();
        let order = init_order(&roots, &deps);
        let mut out = [None; MAX_OBJECTS];
        for (index, id) in order.iter().take(MAX_OBJECTS).enumerate() {
            out[index] = Some(*id);
        }
        out
    }
}
