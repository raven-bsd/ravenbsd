// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 11: the ELF64 dynamic-loader logic (C52).
//!
//! The body digests a synthetic or arbitrary ET_DYN image through a recording `MemView`, then
//! exercises both hash tables, lookup, versions, relocation classification and application,
//! `$ORIGIN` search expansion, `LD_*` parsing and the init/fini order.

use std::cell::Cell;

use raven_libc_elf::elf::read_phdrs;
use raven_libc_elf::reloc::{RelocAction, classify};
use raven_libc_elf::{
    ApplyError, Arch, BindMode, Ehdr, EnvConfig, IfuncArg, MemView, ObjId, ObjTable, ObjView,
    Phase, Rela, RelocApplier, RelocError, SliceMem, SymDef, SymLook, SymResolver, TlsDesc,
    apply_object, elf_hash, expand_search, fetch_ventry, fini_order, gnu_hash, init_order,
    lookup_obj, plan_segments, verify_object_versions,
};

use crate::model;
use crate::{Cursor, alloc_probe};

struct Recording<'a> {
    inner: SliceMem<'a>,
    outside: Cell<usize>,
}

impl MemView for Recording<'_> {
    fn read(&self, vaddr: u64, len: usize) -> Option<&[u8]> {
        let result = self.inner.read(vaddr, len);
        if let Some(bytes) = result {
            let start = bytes.as_ptr() as usize;
            let end = start.saturating_add(bytes.len());
            let base = self.inner.bytes.as_ptr() as usize;
            let limit = base.saturating_add(self.inner.bytes.len());
            if start < base || end > limit {
                self.outside.set(self.outside.get().saturating_add(1));
            }
        }
        result
    }

    fn cstr(&self, vaddr: u64) -> Option<&[u8]> {
        self.inner.cstr(vaddr)
    }
}

struct EmptyTable;

impl ObjTable for EmptyTable {
    fn view(&self, _id: ObjId) -> Option<&ObjView> {
        None
    }
    fn deps(&self, _id: ObjId) -> &[ObjId] {
        &[]
    }
    fn dldags(&self, _id: ObjId) -> &[ObjId] {
        &[]
    }
    fn deepbind(&self, _id: ObjId) -> bool {
        false
    }
}

struct Resolver;

impl SymResolver for Resolver {
    fn resolve(
        &mut self,
        _referencing: ObjId,
        _sym_index: u32,
    ) -> Result<Option<SymDef>, RelocError> {
        Ok(None)
    }
}

struct Applier {
    bytes: Vec<u8>,
    outside: usize,
}

impl Applier {
    fn range(&self, addr: u64, len: u64) -> Option<(usize, usize)> {
        let end = addr.checked_add(len)?;
        let start = usize::try_from(addr).ok()?;
        let end = usize::try_from(end).ok()?;
        if end <= self.bytes.len() {
            Some((start, end))
        } else {
            None
        }
    }
}

impl RelocApplier for Applier {
    fn write64(&mut self, addr: u64, val: u64) -> Result<(), ApplyError> {
        let Some((start, end)) = self.range(addr, 8) else {
            self.outside = self.outside.saturating_add(1);
            return Err(ApplyError::OutOfRange);
        };
        if let Some(slot) = self.bytes.get_mut(start..end) {
            slot.copy_from_slice(&val.to_le_bytes());
        }
        Ok(())
    }

    fn write32(&mut self, addr: u64, val: u32) -> Result<(), ApplyError> {
        let Some((start, end)) = self.range(addr, 4) else {
            self.outside = self.outside.saturating_add(1);
            return Err(ApplyError::OutOfRange);
        };
        if let Some(slot) = self.bytes.get_mut(start..end) {
            slot.copy_from_slice(&val.to_le_bytes());
        }
        Ok(())
    }

    fn read64(&self, addr: u64) -> Option<u64> {
        let (start, end) = self.range(addr, 8)?;
        let bytes: [u8; 8] = self.bytes.get(start..end)?.try_into().ok()?;
        Some(u64::from_le_bytes(bytes))
    }

    fn call_ifunc(&mut self, resolver: u64, _arg: &IfuncArg) -> Result<u64, ApplyError> {
        Ok(resolver)
    }

    fn tls_module_id(&self, _obj: ObjId) -> u64 {
        1
    }

    fn tls_static_offset(&mut self, _obj: ObjId) -> Result<i64, ApplyError> {
        Ok(0)
    }

    fn tls_desc_alloc(
        &mut self,
        _obj: ObjId,
        _sym_value: u64,
        _addend: i64,
    ) -> Result<TlsDesc, ApplyError> {
        Ok(TlsDesc {
            resolver: 0,
            arg: 0,
        })
    }

    fn copy(&mut self, dst: u64, src: u64, len: u64) -> Result<(), ApplyError> {
        let Some((dst_start, dst_end)) = self.range(dst, len) else {
            self.outside = self.outside.saturating_add(1);
            return Err(ApplyError::BadCopy);
        };
        let Some((src_start, src_end)) = self.range(src, len) else {
            return Err(ApplyError::BadCopy);
        };
        if dst_end > self.bytes.len() || src_end > self.bytes.len() {
            return Err(ApplyError::BadCopy);
        }
        let source = self.bytes.get(src_start..src_end).unwrap_or(&[]).to_vec();
        if let Some(slot) = self.bytes.get_mut(dst_start..dst_end) {
            slot.copy_from_slice(&source);
        }
        Ok(())
    }
}

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

fn names<'a>(cursor: &mut Cursor<'a>) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    let mut rest = cursor.rest();
    while out.len() < 8 && !rest.is_empty() {
        let end = rest
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(rest.len());
        let name = rest.get(..end).unwrap_or(&[]);
        if !name.is_empty() {
            out.push(name);
        }
        if end >= rest.len() {
            break;
        }
        rest = rest.get(end.saturating_add(1)..).unwrap_or(&[]);
    }
    if out.is_empty() {
        out.push(b"main");
    }
    out
}

/// Run target 11.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let arch = match cursor.u8() % 3 {
        0 => Arch::X86_64,
        1 => Arch::Aarch64,
        _ => Arch::Riscv64,
    };
    let bias = if cursor.u8() & 1 == 0 { 0 } else { 0x1000 };
    let image_len = usize::from(cursor.u16()) % 4097;
    let image = cursor.take(image_len);
    let lookup_names = names(&mut cursor);
    let mem = Recording {
        inner: SliceMem::new(bias, image),
        outside: Cell::new(0),
    };

    let mut relocations = 0usize;
    let mut digest_ok = false;
    if let Ok(header) = Ehdr::parse(image) {
        let phdr_vaddr = bias.saturating_add(header.e_phoff);
        let phdrs = read_phdrs(&mem, phdr_vaddr, header.e_phnum);
        if let Ok(headers) = &phdrs {
            let _ = plan_segments(headers, 4096);
        }
        if let Ok(object) = ObjView::digest(&mem, header.e_phoff, header.e_phnum, bias) {
            digest_ok = true;
            let _ = fetch_ventry(&object, &mem, 0);
            let table = EmptyTable;
            let _ = verify_object_versions(&object, &mem, &|_name: &[u8]| None, &table);
            for name in &lookup_names {
                let look = SymLook::new(name);
                let _ = lookup_obj(ObjId(0), &object, &mem, &look);
                assert_eq!(
                    elf_hash(name),
                    model::elf_hash(name),
                    "P-elf-7: elf_hash differs from the model"
                );
                assert_eq!(
                    gnu_hash(name),
                    model::gnu_hash(name),
                    "P-elf-7: gnu_hash differs from the model"
                );
            }
            let entries = object.rela.1 / 24;
            for index in 0..entries.min(256) {
                let address = object.rela.0.saturating_add(index.saturating_mul(24));
                if let Some(bytes) = mem.read(address, 24)
                    && let Ok(rela) = Rela::parse(bytes)
                {
                    match classify(arch, rela.typ()) {
                        RelocAction::Reject(_) | RelocAction::None => {}
                        _ => relocations = relocations.saturating_add(1),
                    }
                }
            }
            let mut resolver = Resolver;
            let mut applier = Applier {
                bytes: vec![0u8; 4096],
                outside: 0,
            };
            let _ = apply_object(
                arch,
                ObjId(0),
                &object,
                true,
                &mem,
                &mut resolver,
                &mut applier,
                BindMode::Now,
                Phase::NonPlt,
                &IfuncArg {
                    hwcap: 0,
                    hwcap2: 0,
                },
            );
            assert!(
                applier.outside == 0,
                "P-elf-5: apply_object wrote outside the applier buffer"
            );
            let origin = |_id: ObjId| -> Option<Vec<u8>> { None };
            let search = raven_libc_elf::SearchCtx {
                trust: true,
                ld_library_path: None,
                library_path_rpath: false,
                main_rpath: None,
                default_dirs: &[b"/lib".as_slice()],
                origin_of: &origin,
                platform: b"x86_64",
            };
            let _ = expand_search(
                &search,
                lookup_names.first().copied().unwrap_or(b"main"),
                None,
            );
            let entries: Vec<&[u8]> = lookup_names.to_vec();
            let _ = EnvConfig::parse(&entries, true);
        }
    }

    let first = lookup_names.first().copied().unwrap_or(b"");
    if let Ok(header) = Ehdr::parse(image) {
        let _ = header;
    }
    let _ = (first, digest_ok);

    let node_count = 1 + u32::from(cursor.u8() % 4);
    let roots = [ObjId(node_count.saturating_sub(1))];
    let deps = move |id: ObjId| -> Vec<ObjId> {
        if id.0 > 0 {
            vec![ObjId(id.0.saturating_sub(1))]
        } else {
            Vec::new()
        }
    };
    let init = init_order(&roots, &deps);
    let fini = fini_order(&init);
    let order: Vec<u32> = init.iter().map(|id| id.0).collect();
    let finish: Vec<u32> = fini.iter().map(|id| id.0).collect();
    let model_deps = |id: u32| {
        if id > 0 {
            vec![id.saturating_sub(1)]
        } else {
            Vec::new()
        }
    };
    assert!(
        model::init_order_ok(&order, &finish, &model_deps),
        "P-elf-6: init_order is not dependency-first or fini is not its reverse"
    );

    assert!(
        mem.outside.get() == 0,
        "P-elf-2: a MemView request left the image"
    );

    alloc_probe::note(hash(&[
        u64::from(digest_ok),
        relocations as u64,
        lookup_names.len() as u64,
        node_count as u64,
    ]));
}

#[cfg(test)]
#[path = "elf_dynamic_ut.rs"]
mod tests;
