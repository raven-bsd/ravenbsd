// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! `dlopen`/`dlsym`/`dlclose`/`dladdr` logic over the loaded-object table.

use raven_libc_elf::{ObjId, SymLook, scope::symlook_default, scope::symlook_needed};

use crate::loadobj::{LoadError, Loader, Name};
use crate::mapper::Platform;

/// `RTLD_LAZY` (accepted, but the loader always binds now).
pub const RTLD_LAZY: i32 = 0x1;
/// `RTLD_NOW`.
pub const RTLD_NOW: i32 = 0x2;
/// `RTLD_GLOBAL`.
pub const RTLD_GLOBAL: i32 = 0x100;
/// `RTLD_LOCAL`.
pub const RTLD_LOCAL: i32 = 0;
/// `RTLD_NOLOAD`.
pub const RTLD_NOLOAD: i32 = 0x4;
/// `RTLD_NODELETE`.
pub const RTLD_NODELETE: i32 = 0x1000;

impl<P: Platform, M: raven_libc_elf::MemView, A: raven_libc_elf::RelocApplier> Loader<P, M, A> {
    /// Open a library by name; `None` means the global handle.
    pub fn dlopen(&mut self, name: &[u8], flags: i32) -> Result<ObjId, LoadError> {
        if let Some(id) = self.by_name(name) {
            if flags & RTLD_NOLOAD == 0
                && let Some(object) = self.object_mut(id)
            {
                object.refcount = object.refcount.saturating_add(1);
                if flags & RTLD_NODELETE != 0 {
                    object.nodelete = true;
                }
            }
            return Ok(id);
        }
        if flags & RTLD_NOLOAD != 0 {
            return Err(LoadError::NotFound);
        }
        let path = self.find_library(name).ok_or(LoadError::NotFound)?;
        let id = self.load_object(path.as_bytes(), name)?;
        self.load_needed_objects(id)?;
        if flags & RTLD_GLOBAL != 0 {
            self.scope.global.push(id);
        }
        self.register_linkmap(id);
        self.relocate_all()?;
        Ok(id)
    }

    /// Resolve `name` in `handle`.
    pub fn dlsym(&self, handle: Option<ObjId>, name: &[u8], _caller: usize) -> Option<usize> {
        let request = SymLook::new(name);
        let table = self.table();
        if let Some(id) = handle {
            let hit = symlook_needed(&table, &self.mem, id, &request, &mut [].to_vec()).ok()??;
            usize::try_from(hit.value).ok()
        } else {
            let refobj = self.scope.main;
            let hit =
                symlook_default(&table, &self.mem, &self.scope, &request, refobj, true).ok()??;
            usize::try_from(hit.value).ok()
        }
    }

    /// Resolve an exact version.
    pub fn dlvsym(&self, handle: Option<ObjId>, name: &[u8], version: &[u8]) -> Option<usize> {
        let request = SymLook::new(name).with_version(raven_libc_elf::lookup::VerRef {
            name: version,
            hash: raven_libc_elf::hash::elf_hash(version),
            weak: false,
            file: None,
        });
        let table = self.table();
        let id = handle.unwrap_or(self.scope.main);
        let hit = symlook_needed(&table, &self.mem, id, &request, &mut [].to_vec()).ok()??;
        usize::try_from(hit.value).ok()
    }

    /// Close a handle; returns 0 on success.
    pub fn dlclose(&mut self, id: ObjId) -> i32 {
        let Some(object) = self.object_mut(id) else {
            self.error.set(format_args!("dlclose: invalid handle"));
            return -1;
        };
        if object.refcount > 0 {
            object.refcount = object.refcount.saturating_sub(1);
        }
        if object.refcount == 0 && !object.nodelete && !object.has_tls() {
            let _ = self.unload_object(id);
        }
        0
    }

    /// The last error, returned once.
    pub fn dlerror(&mut self) -> Option<&[u8]> {
        self.error.take_once()
    }

    /// Find the object that contains `address`.
    #[must_use]
    pub fn dladdr(&self, address: usize) -> Option<ObjId> {
        for index in 0..self.count {
            let id = ObjId(u32::try_from(index).ok()?);
            if let Some(object) = self.object(id) {
                let base = usize::try_from(object.mapped.base).ok()?;
                let end = base.checked_add(usize::try_from(object.mapped.span).ok()?)?;
                if address >= base && address < end {
                    return Some(id);
                }
            }
        }
        None
    }

    /// The `dl_iterate_phdr` object order: main first, then load order.
    #[must_use]
    pub fn iterate_ids(&self) -> [Option<ObjId>; crate::loadobj::MAX_OBJECTS] {
        let mut out = [None; crate::loadobj::MAX_OBJECTS];
        let mut position = 0usize;
        if let Some(main) = Some(self.scope.main)
            && self.object(main).is_some()
        {
            out[position] = Some(main);
            position = position.saturating_add(1);
        }
        for index in 0..self.count {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            let id = ObjId(index);
            if Some(id) != Some(self.scope.main)
                && self.object(id).is_some()
                && position < out.len()
            {
                out[position] = Some(id);
                position = position.saturating_add(1);
            }
        }
        out
    }
}

/// The name-copy helper used by the shell when it materialises `dlpi_name`.
#[must_use]
pub fn name_of(loader_name: Name) -> &'static [u8] {
    let _ = loader_name;
    &[]
}
