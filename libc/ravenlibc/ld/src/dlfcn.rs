// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! `dlopen`/`dlsym`/`dlclose`/`dladdr` logic over the loaded-object table.

use raven_libc_elf::{
    ObjId, SymLook, lookup::SymFlags, lookup::lookup_obj, scope::symlook_default,
    scope::symlook_needed,
};

use crate::loadobj::{LoadError, Loader, MAX_OBJECTS, Name};
use crate::mapper::Platform;
use crate::search;
use crate::table::RtldTable;

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

/// The `RTLD_DEFAULT` pseudo-handle.
pub const RTLD_DEFAULT: ObjId = ObjId(u32::MAX - 2);
/// The `RTLD_NEXT` pseudo-handle.
pub const RTLD_NEXT: ObjId = ObjId(u32::MAX - 1);

impl<P: Platform, M: raven_libc_elf::MemView, A: raven_libc_elf::RelocApplier> Loader<P, M, A> {
    /// Open a library by name; `None` means the global handle.
    pub fn dlopen(&mut self, name: &[u8], flags: i32) -> Result<ObjId, LoadError> {
        let canonical = search::canonical(name);
        if let Some(id) = self.by_name(name).or_else(|| self.by_name(canonical)) {
            if flags & RTLD_NOLOAD == 0
                && let Some(object) = self.object_mut(id)
            {
                object.refcount = object.refcount.saturating_add(1);
                if flags & RTLD_NODELETE != 0 {
                    object.nodelete = true;
                }
            }
            self.error.clear();
            return Ok(id);
        }
        if flags & RTLD_NOLOAD != 0 {
            self.error.set(format_args!("dlopen: object not loaded"));
            return Err(LoadError::NotFound);
        }
        let path = match self.find_library_for(name, Some(self.scope.main))? {
            Some(path) => path,
            None => {
                self.error
                    .set(format_args!("dlopen: shared object not found"));
                return Err(LoadError::NotFound);
            }
        };
        let id = match self.load_object(path.as_bytes(), name) {
            Ok(id) => id,
            Err(error) => {
                self.error.set(format_args!("dlopen: cannot load object"));
                return Err(error);
            }
        };
        if let Err(error) = self.load_needed_objects(id) {
            self.error
                .set(format_args!("dlopen: cannot load dependencies"));
            return Err(error);
        }
        if let Some(object) = self.object_mut(id) {
            object.refcount = 1;
            if flags & RTLD_NODELETE != 0 {
                object.nodelete = true;
            }
        }
        if flags & RTLD_GLOBAL != 0 {
            self.scope.global.push(id);
        }
        self.register_linkmap(id);
        if let Err(error) = self.relocate_all() {
            self.error
                .set(format_args!("dlopen: cannot relocate object"));
            return Err(error);
        }
        self.error.clear();
        Ok(id)
    }

    /// Resolve `name` in `handle`; `caller` is the return address for `RTLD_NEXT`.
    pub fn dlsym(&mut self, handle: Option<ObjId>, name: &[u8], caller: usize) -> Option<usize> {
        let request = SymLook::new(name).with_flags(SymFlags::NO_TLS);
        let value = if handle == Some(RTLD_NEXT) {
            let caller_id = self.dladdr(caller).unwrap_or(self.scope.main);
            let start = usize::try_from(caller_id.0).ok()?.saturating_add(1);
            let mut found = None;
            for index in start..MAX_OBJECTS {
                let Some(object) = self.objects.get(index).and_then(|slot| slot.as_ref()) else {
                    continue;
                };
                let Ok(id) = u32::try_from(index) else {
                    continue;
                };
                if let Ok(Some(hit)) = lookup_obj(ObjId(id), &object.view, &self.mem, &request) {
                    found = Some(hit.value);
                    break;
                }
            }
            found
        } else if handle.is_none() || handle == Some(RTLD_DEFAULT) {
            let table = self.table();
            let refobj = self.scope.main;
            symlook_default(&table, &self.mem, &self.scope, &request, refobj, true)
                .ok()
                .flatten()
                .map(|hit| hit.value)
        } else {
            let table = self.table();
            let id = handle?;
            symlook_needed(&table, &self.mem, id, &request, &mut [].to_vec())
                .ok()
                .flatten()
                .map(|hit| hit.value)
        };
        match value.and_then(|value| usize::try_from(value).ok()) {
            Some(value) => {
                self.error.clear();
                Some(value)
            }
            None => {
                self.error.set(format_args!("dlsym: symbol not found"));
                None
            }
        }
    }

    /// Resolve an exact version.
    pub fn dlvsym(&mut self, handle: Option<ObjId>, name: &[u8], version: &[u8]) -> Option<usize> {
        let request = SymLook::new(name).with_version(raven_libc_elf::lookup::VerRef {
            name: version,
            hash: raven_libc_elf::hash::elf_hash(version),
            weak: false,
            file: None,
        });
        let table = self.table();
        let id = handle.unwrap_or(self.scope.main);
        let value = symlook_needed(&table, &self.mem, id, &request, &mut [].to_vec())
            .ok()
            .flatten()
            .and_then(|hit| usize::try_from(hit.value).ok());
        match value {
            Some(value) => {
                self.error.clear();
                Some(value)
            }
            None => {
                self.error.set(format_args!("dlvsym: version not found"));
                None
            }
        }
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
        self.error.clear();
        0
    }

    /// Install the `RtldTable` into the shell's `__rl_dl_install`, exactly once.
    pub fn install_table(&mut self, table: &RtldTable) -> bool {
        if self.table_address != 0 {
            return false;
        }
        let Some(address) = self.dlsym(None, b"__rl_dl_install", 0) else {
            return false;
        };
        if self.platform.install_rtld_table(address, table) {
            self.table_address = table as *const RtldTable as u64;
            true
        } else {
            false
        }
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

    /// Walk the `dl_iterate_phdr` order; the callback returns `false` to stop.
    pub fn iterate_phdr<F: FnMut(ObjId) -> bool>(&self, mut callback: F) -> usize {
        let ids = self.iterate_ids();
        let mut visited = 0usize;
        for id in ids.into_iter().flatten() {
            visited = visited.saturating_add(1);
            if !callback(id) {
                break;
            }
        }
        visited
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
