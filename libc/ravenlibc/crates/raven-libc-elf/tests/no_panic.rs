// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]

//! The single-byte-flip sweep and the determinism check.
//!
//! Every byte of three small builder images is flipped once; every entry point the fuzz target
//! (C70) will use must return or error, never panic. A second run over the unmodified image must
//! give the identical view, lookup result and write log.

#[path = "common/elfbuild.rs"]
mod elfbuild;

use raven_libc_elf::elf::Arch;
use raven_libc_elf::errors::{ApplyError, RelocError};
use raven_libc_elf::lookup::{SymFlags, SymLook, lookup_obj};
use raven_libc_elf::mem::SliceMem;
use raven_libc_elf::object::ObjView;
use raven_libc_elf::reloc::{
    BindMode, IfuncArg, Phase, RelocApplier, SymDef, SymResolver, TlsDesc, apply_object, apply_relr,
};
use raven_libc_elf::relr::relr_offsets;
use raven_libc_elf::scope::ObjId;
use raven_libc_elf::version::fetch_ventry;

struct NullResolver;

impl SymResolver for NullResolver {
    fn resolve(&mut self, _: ObjId, _: u32) -> Result<Option<SymDef>, RelocError> {
        Ok(None)
    }
}

struct LogApplier {
    memory: Vec<u8>,
    log: Vec<(u64, u64)>,
}

impl LogApplier {
    fn new() -> Self {
        Self {
            memory: vec![0u8; 0x40_000],
            log: Vec::new(),
        }
    }
}

impl RelocApplier for LogApplier {
    fn write64(&mut self, addr: u64, val: u64) -> Result<(), ApplyError> {
        let at = usize::try_from(addr).map_err(|_| ApplyError::OutOfRange)?;
        let slot = self
            .memory
            .get_mut(at..at + 8)
            .ok_or(ApplyError::OutOfRange)?;
        slot.copy_from_slice(&val.to_le_bytes());
        self.log.push((addr, val));
        Ok(())
    }
    fn write32(&mut self, addr: u64, val: u32) -> Result<(), ApplyError> {
        let at = usize::try_from(addr).map_err(|_| ApplyError::OutOfRange)?;
        let slot = self
            .memory
            .get_mut(at..at + 4)
            .ok_or(ApplyError::OutOfRange)?;
        slot.copy_from_slice(&val.to_le_bytes());
        Ok(())
    }
    fn read64(&self, addr: u64) -> Option<u64> {
        let at = usize::try_from(addr).ok()?;
        let slice = self.memory.get(at..at + 8)?;
        Some(u64::from_le_bytes(slice.try_into().ok()?))
    }
    fn call_ifunc(&mut self, _: u64, _: &IfuncArg) -> Result<u64, ApplyError> {
        Ok(0)
    }
    fn tls_module_id(&self, _: ObjId) -> u64 {
        1
    }
    fn tls_static_offset(&mut self, _: ObjId) -> Result<i64, ApplyError> {
        Ok(0)
    }
    fn tls_desc_alloc(&mut self, _: ObjId, _: u64, _: i64) -> Result<TlsDesc, ApplyError> {
        Ok(TlsDesc {
            resolver: 0,
            arg: 0,
        })
    }
    fn copy(&mut self, _: u64, _: u64, _: u64) -> Result<(), ApplyError> {
        Ok(())
    }
}

fn images() -> Vec<elfbuild::Image> {
    let plain = elfbuild::Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .symbol(elfbuild::global_object(b"data", 0x200))
        .needed(b"libc.so.1")
        .soname(b"libplain.so.1")
        .rela(elfbuild::DATA_BASE + 0x200, 1, 8, 0)
        .build();
    let gnu = elfbuild::Builder::new()
        .with_sysv_hash(false)
        .with_gnu_hash(true)
        .symbol(elfbuild::global_func(b"alpha", 0x100))
        .symbol(elfbuild::global_func(b"beta", 0x200))
        .build();
    let versioned = elfbuild::Builder::new()
        .symbol(elfbuild::SymbolSpec {
            name: b"f".to_vec(),
            info: (1 << 4) | 2,
            other: 0,
            shndx: 1,
            value: 0x100,
            size: 16,
            version: 2,
            hidden: false,
        })
        .version_def(elfbuild::VersionDef {
            name: b"A".to_vec(),
            index: 2,
            flags: 0,
        })
        .relr(elfbuild::DATA_BASE + 0x200)
        .build();
    vec![plain, gnu, versioned]
}

fn probe(bytes: &[u8]) {
    use raven_libc_elf::elf::Ehdr;
    let phnum = Ehdr::parse(bytes).map(|header| header.e_phnum).unwrap_or(5);
    let memory = SliceMem::new(0, bytes);
    let Ok(view) = ObjView::digest(&memory, 64, phnum, 0) else {
        return;
    };
    for name in [b"f".as_slice(), b"data", b"alpha", b"beta", b"absent"] {
        for flags in [SymFlags::default(), SymFlags::IN_PLT, SymFlags::NO_TLS] {
            let _ = lookup_obj(
                ObjId(0),
                &view,
                &memory,
                &SymLook::new(name).with_flags(flags),
            );
        }
    }
    for index in 0..4u32 {
        let _ = fetch_ventry(&view, &memory, index);
    }
    for phase in [Phase::NonPlt, Phase::Plt, Phase::Ifunc] {
        let mut resolver = NullResolver;
        let mut applier = LogApplier::new();
        let _ = apply_object(
            Arch::X86_64,
            ObjId(0),
            &view,
            false,
            &memory,
            &mut resolver,
            &mut applier,
            BindMode::Now,
            phase,
            &IfuncArg {
                hwcap: 0,
                hwcap2: 0,
            },
        );
    }
    let mut applier = LogApplier::new();
    let _ = apply_relr(&view, &memory, &mut applier);
    let _ = relr_offsets(&view, &memory).map(|iterator| iterator.take(16).collect::<Vec<_>>());
}

#[test]
fn flipping_every_byte_never_panics() {
    for image in images() {
        let original = image.bytes.clone();
        for index in 0..2048.min(original.len()) {
            let mut bytes = original.clone();
            if let Some(byte) = bytes.get_mut(index) {
                *byte ^= 0xff;
            }
            probe(&bytes);
        }
    }
}

#[test]
fn the_same_image_gives_the_same_view_and_writes() {
    for image in images() {
        let memory = SliceMem::new(0, &image.bytes);
        let first = ObjView::digest(&memory, 64, 5, 0).unwrap();
        let second = ObjView::digest(&memory, 64, 5, 0).unwrap();
        assert_eq!(first, second);
        for name in [b"f".as_slice(), b"alpha", b"beta", b"data"] {
            let request = SymLook::new(name);
            let left = lookup_obj(ObjId(0), &first, &memory, &request).unwrap();
            let right = lookup_obj(ObjId(0), &second, &memory, &request).unwrap();
            assert_eq!(left.map(|hit| hit.value), right.map(|hit| hit.value));
        }
        let mut left = LogApplier::new();
        let mut right = LogApplier::new();
        let mut resolver = NullResolver;
        apply_object(
            Arch::X86_64,
            ObjId(0),
            &first,
            false,
            &memory,
            &mut resolver,
            &mut left,
            BindMode::Now,
            Phase::NonPlt,
            &IfuncArg {
                hwcap: 0,
                hwcap2: 0,
            },
        )
        .unwrap();
        let mut resolver = NullResolver;
        apply_object(
            Arch::X86_64,
            ObjId(0),
            &second,
            false,
            &memory,
            &mut resolver,
            &mut right,
            BindMode::Now,
            Phase::NonPlt,
            &IfuncArg {
                hwcap: 0,
                hwcap2: 0,
            },
        )
        .unwrap();
        assert_eq!(left.log, right.log);
    }
}
