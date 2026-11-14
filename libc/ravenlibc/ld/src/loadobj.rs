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
//! The loaded-object table, object loading and the `DT_NEEDED` closure.

use raven_libc_elf::{
    Arch, DigestError, Ehdr, ElfError, MemView, ObjId, ObjTable, ObjView, Phdr, Scope, SliceMem,
    plan_segments,
};

use crate::arch;
use crate::errbuf::ErrBuf;
use crate::ldalloc::LdAlloc;
use crate::lock::RtldLock;
use crate::mapper::{MapErrKind, Mapped, Platform, map_object};
use crate::rdebug::{LinkMapTable, RDebug};
use crate::search;

/// The maximum number of objects.
pub const MAX_OBJECTS: usize = 64;
/// The maximum number of names per object.
pub const MAX_NAMES: usize = 8;
/// The maximum length of one name.
pub const MAX_NAME: usize = 128;
/// The maximum number of direct dependencies.
pub const MAX_DEPS: usize = 32;
/// The maximum number of library search directories.
pub const MAX_DIRS: usize = 8;

/// A fixed byte string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Name {
    bytes: [u8; MAX_NAME],
    len: u8,
}

impl Name {
    /// An empty name.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            bytes: [0; MAX_NAME],
            len: 0,
        }
    }

    /// Copy `text` into a name.
    #[must_use]
    pub fn new(text: &[u8]) -> Option<Self> {
        let len = text.len().min(MAX_NAME);
        let mut name = Self::empty();
        if let Some(slot) = name.bytes.get_mut(..len) {
            slot.copy_from_slice(text.get(..len)?);
        }
        name.len = u8::try_from(len).ok()?;
        Some(name)
    }

    /// The bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.len)).unwrap_or(&[])
    }

    /// Whether the name is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// One loaded object.
pub struct LoadedObject {
    /// The C52 view (addresses are absolute).
    pub view: ObjView,
    /// The mapping result.
    pub mapped: Mapped,
    /// The names the object is known by.
    pub names: [Name; MAX_NAMES],
    /// Number of valid names.
    pub name_count: usize,
    /// Direct dependencies.
    pub deps: [ObjId; MAX_DEPS],
    /// Number of valid dependencies.
    pub dep_count: usize,
    /// `dlopen` references.
    pub refcount: u32,
    /// `DF_1_NODELETE`.
    pub nodelete: bool,
    /// The static TLS offset.
    pub tls_offset: usize,
    /// The static TLS size.
    pub tls_size: usize,
    /// Whether the object's non-IFUNC relocation phases have run.
    pub relocated: bool,
    /// Whether the IFUNC/IRELATIVE pass has run.
    pub ifunc_done: bool,
}

impl LoadedObject {
    /// Whether this object has a static TLS block.
    #[must_use]
    pub fn has_tls(&self) -> bool {
        self.view.tls.is_some()
    }

    /// Add one name.
    pub fn add_name(&mut self, name: &[u8]) -> bool {
        if self.name_count >= MAX_NAMES {
            return false;
        }
        let Some(value) = Name::new(name) else {
            return false;
        };
        if let Some(slot) = self.names.get_mut(self.name_count) {
            *slot = value;
            self.name_count = self.name_count.saturating_add(1);
            true
        } else {
            false
        }
    }

    /// Whether `name` is one of the aliases.
    #[must_use]
    pub fn is_named(&self, name: &[u8]) -> bool {
        self.names
            .iter()
            .take(self.name_count)
            .any(|known| known.as_bytes() == name)
    }
}

/// The object table view used by C52's scope/lookup code.
pub struct ObjectTable<'a> {
    pub(crate) objects: &'a [Option<LoadedObject>; MAX_OBJECTS],
}

impl ObjTable for ObjectTable<'_> {
    fn view(&self, id: ObjId) -> Option<&ObjView> {
        self.objects
            .get(usize::try_from(id.0).ok()?)
            .and_then(|slot| slot.as_ref())
            .map(|object| &object.view)
    }

    fn deps(&self, id: ObjId) -> &[ObjId] {
        self.objects
            .get(usize::try_from(id.0).ok().unwrap_or(usize::MAX))
            .and_then(|slot| slot.as_ref())
            .map(|object| object.deps.get(..object.dep_count).unwrap_or(&[]))
            .unwrap_or(&[])
    }

    fn dldags(&self, _id: ObjId) -> &[ObjId] {
        &[]
    }

    fn deepbind(&self, _id: ObjId) -> bool {
        false
    }
}

/// The loader state.
pub struct Loader<P: Platform, M: MemView, A: raven_libc_elf::RelocApplier> {
    /// The platform operations.
    pub platform: P,
    /// The live memory view.
    pub mem: M,
    /// The relocation applier.
    pub applier: A,
    /// The loaded objects.
    pub objects: [Option<LoadedObject>; MAX_OBJECTS],
    /// The global lookup scope.
    pub scope: Scope,
    /// Number of valid object slots.
    pub count: usize,
    /// The architecture.
    pub arch: Arch,
    /// The `r_debug` record.
    pub rdebug: RDebug,
    /// The link map.
    pub linkmap: LinkMapTable,
    /// The loader lock set.
    pub lock: RtldLock,
    /// The loader allocator.
    pub alloc: LdAlloc,
    /// The `dlerror` buffer.
    pub error: ErrBuf,
    /// The search path from `LD_LIBRARY_PATH`.
    pub library_path: [Name; MAX_DIRS],
    /// Number of search path entries.
    pub library_path_count: usize,
    /// `LD_PRELOAD` names.
    pub preload: [Name; 4],
    /// Number of preload entries.
    pub preload_count: usize,
    /// The trusted (not secure-exec) flag.
    pub trust: bool,
    /// Bind every relocation now.
    pub bind_now: bool,
    /// The installed `RtldTable` address, if any.
    pub table_address: u64,
}

impl<P: Platform, M: MemView, A: raven_libc_elf::RelocApplier> Loader<P, M, A> {
    /// A loader with no objects.
    pub fn new(platform: P, mem: M, applier: A) -> Self {
        Self {
            platform,
            mem,
            applier,
            objects: [const { None }; MAX_OBJECTS],
            scope: Scope::new(ObjId(0)),
            count: 0,
            arch: arch::target_arch(),
            rdebug: RDebug::new(0),
            linkmap: LinkMapTable::new(),
            lock: RtldLock::new(),
            alloc: LdAlloc::new(),
            error: ErrBuf::new(),
            library_path: [Name::empty(); MAX_DIRS],
            library_path_count: 0,
            preload: [Name::empty(); 4],
            preload_count: 0,
            trust: true,
            bind_now: true,
            table_address: 0,
        }
    }

    /// The object table.
    #[must_use]
    pub fn table(&self) -> ObjectTable<'_> {
        ObjectTable {
            objects: &self.objects,
        }
    }

    /// The object at `id`.
    #[must_use]
    pub fn object(&self, id: ObjId) -> Option<&LoadedObject> {
        self.objects
            .get(usize::try_from(id.0).ok()?)
            .and_then(|slot| slot.as_ref())
    }

    /// The object at `id`, mutably.
    pub fn object_mut(&mut self, id: ObjId) -> Option<&mut LoadedObject> {
        self.objects
            .get_mut(usize::try_from(id.0).ok()?)
            .and_then(|slot| slot.as_mut())
    }

    /// Insert one object and return its id.
    pub fn insert(&mut self, object: LoadedObject) -> Option<ObjId> {
        for index in 0..MAX_OBJECTS {
            if self.objects.get(index).is_some_and(Option::is_none) {
                if let Some(slot) = self.objects.get_mut(index) {
                    *slot = Some(object);
                }
                self.count = self.count.saturating_add(1);
                return Some(ObjId(u32::try_from(index).ok()?));
            }
        }
        None
    }

    /// Add a search directory.
    pub fn add_library_path(&mut self, path: &[u8]) -> bool {
        if self.library_path_count >= MAX_DIRS {
            return false;
        }
        let Some(name) = Name::new(path) else {
            return false;
        };
        if let Some(slot) = self.library_path.get_mut(self.library_path_count) {
            *slot = name;
            self.library_path_count = self.library_path_count.saturating_add(1);
            true
        } else {
            false
        }
    }

    /// Find an object by name.
    #[must_use]
    pub fn by_name(&self, name: &[u8]) -> Option<ObjId> {
        for index in 0..MAX_OBJECTS {
            if let Some(object) = self.objects.get(index).and_then(|slot| slot.as_ref())
                && object.is_named(name)
            {
                return Some(ObjId(u32::try_from(index).ok()?));
            }
        }
        None
    }

    /// Open and map one object.
    pub fn load_object(&mut self, path: &[u8], requested: &[u8]) -> Result<ObjId, LoadError> {
        if let Some(id) = self.by_name(requested) {
            return Ok(id);
        }
        let file = self
            .platform
            .open(path)
            .map_err(|error| LoadError::Io(error.errno()))?;
        let mut header_bytes = [0u8; 64];
        self.platform
            .read(&file, 0, &mut header_bytes)
            .map_err(|error| LoadError::Io(error.errno()))?;
        let header = Ehdr::parse(&header_bytes).map_err(LoadError::Elf)?;
        let mut phdrs = [Phdr::parse(&[0u8; 56]).ok(); 16];
        let phnum = usize::from(header.e_phnum).min(16);
        for index in 0..phnum {
            let offset = header
                .e_phoff
                .checked_add((index as u64).saturating_mul(56))
                .ok_or(LoadError::Elf(ElfError::BadEntry))?;
            let mut bytes = [0u8; 56];
            self.platform
                .read(&file, offset, &mut bytes)
                .map_err(|error| LoadError::Io(error.errno()))?;
            let parsed = Phdr::parse(&bytes).map_err(LoadError::Elf)?;
            if let Some(slot) = phdrs.get_mut(index) {
                *slot = Some(parsed);
            }
        }
        let mut headers = [Phdr::parse(&[0u8; 56]).map_err(LoadError::Elf)?; 16];
        for index in 0..phnum {
            headers[index] = phdrs[index].ok_or(LoadError::Elf(ElfError::BadEntry))?;
        }
        let plan = plan_segments(&headers[..phnum], raven_libc_elf::object::PAGE_SIZE)
            .map_err(LoadError::Elf)?;
        let mapped = map_object(&plan, &file, &mut self.platform).map_err(LoadError::Map)?;
        self.platform.close(file);

        let view = ObjView::digest(&self.mem, header.e_phoff, header.e_phnum, mapped.bias)
            .map_err(LoadError::Digest)?;
        if view.gnu_stack_exec {
            return Err(LoadError::ExecutableStack);
        }
        let mut object = LoadedObject {
            view,
            mapped,
            names: [Name::empty(); MAX_NAMES],
            name_count: 0,
            deps: [ObjId(u32::MAX); MAX_DEPS],
            dep_count: 0,
            refcount: 0,
            nodelete: false,
            tls_offset: 0,
            tls_size: 0,
            relocated: false,
            ifunc_done: false,
        };
        object.add_name(requested);
        object.add_name(path);
        if let Some(soname) = object.view.soname
            && let Ok(name) = object.view.name_at(&self.mem, soname)
        {
            object.add_name(name);
        }
        let id = self.insert(object).ok_or(LoadError::TooManyObjects)?;
        Ok(id)
    }

    /// Load the `DT_NEEDED` closure of `root`, breadth-first.
    pub fn load_needed_objects(&mut self, root: ObjId) -> Result<(), LoadError> {
        let mut index = 0usize;
        let mut queue = [ObjId(u32::MAX); MAX_OBJECTS];
        queue[0] = root;
        let mut queue_len = 1usize;
        while index < queue_len {
            let id = queue[index];
            index = index.saturating_add(1);
            let needed = {
                let Some(object) = self.object(id) else {
                    continue;
                };
                let mut names = [Name::empty(); MAX_DEPS];
                let mut count = 0usize;
                for offset in &object.view.needed {
                    if let Ok(name) = object.view.name_at(&self.mem, *offset)
                        && count < MAX_DEPS
                        && let Some(name) = Name::new(name)
                    {
                        names[count] = name;
                        count = count.saturating_add(1);
                    }
                }
                (names, count)
            };
            let (names, count) = needed;
            for name in names.iter().take(count) {
                let loaded = if let Some(known) = self.by_name(name.as_bytes()) {
                    known
                } else {
                    let path = self
                        .find_library(name.as_bytes())
                        .ok_or(LoadError::NotFound)?;
                    self.load_object(path.as_bytes(), name.as_bytes())?
                };
                if let Some(object) = self.object_mut(id)
                    && object.dep_count < MAX_DEPS
                {
                    object.deps[object.dep_count] = loaded;
                    object.dep_count = object.dep_count.saturating_add(1);
                }
                if queue_len < MAX_OBJECTS {
                    queue[queue_len] = loaded;
                    queue_len = queue_len.saturating_add(1);
                }
            }
        }
        Ok(())
    }

    /// Record one object in the `r_debug` link map.
    pub fn register_linkmap(&mut self, id: ObjId) {
        let Some(object) = self.object(id) else {
            return;
        };
        let addr = object.mapped.base;
        let ld = object.view.dynamic;
        let _ = self.linkmap.add(id, addr, ld);
    }

    /// Find a library by name.
    #[must_use]
    pub fn find_library(&self, name: &[u8]) -> Option<Name> {
        let canonical = search::canonical(name);
        if search::has_slash(canonical) {
            return Name::new(canonical);
        }
        let mut buffer = [0u8; 256];
        for directory in self.library_path.iter().take(self.library_path_count) {
            if let Some(len) = search::join(directory.as_bytes(), canonical, &mut buffer) {
                let candidate = Name::new(buffer.get(..len)?)?;
                if self.platform_can_open(candidate.as_bytes()) {
                    return Some(candidate);
                }
            }
        }
        for directory in search::DEFAULT_DIRS {
            if let Some(len) = search::join(directory, canonical, &mut buffer) {
                let candidate = Name::new(buffer.get(..len)?)?;
                if self.platform_can_open(candidate.as_bytes()) {
                    return Some(candidate);
                }
            }
        }
        None
    }

    fn platform_can_open(&self, _path: &[u8]) -> bool {
        // `find_library` is a pure name chooser on the host; the caller's `open` decides.
        true
    }
}

/// Why a load failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// A file or read error.
    Io(i32),
    /// A malformed ELF.
    Elf(ElfError),
    /// A digest error.
    Digest(DigestError),
    /// A mapping error.
    Map(MapErrKind),
    /// The library was not found.
    NotFound,
    /// The object table is full.
    TooManyObjects,
    /// `PT_GNU_STACK` is executable.
    ExecutableStack,
    /// A relocation error.
    Reloc(raven_libc_elf::RelocError),
}

impl LoadError {
    /// The C `errno` for this failure.
    #[must_use]
    pub const fn errno(self) -> i32 {
        match self {
            Self::Io(value) => value,
            Self::Elf(_) | Self::Digest(_) => 8,
            Self::Map(error) => error.errno(),
            Self::NotFound => 2,
            Self::TooManyObjects => 12,
            Self::ExecutableStack => 8,
            Self::Reloc(_) => 8,
        }
    }
}

/// A helper for the host test: map one file from a byte slice (not used on target).
pub fn digest_slice(
    bytes: &[u8],
    phdr_vaddr: u64,
    phnum: u16,
    bias: u64,
) -> Result<ObjView, DigestError> {
    let mem = SliceMem::new(0, bytes);
    ObjView::digest(&mem, phdr_vaddr, phnum, bias)
}
