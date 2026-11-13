// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// The vectors and addresses are hand-written constants.

//! The relocation tables and the per-architecture formulas.

use crate::testbuild as elfbuild;

use alloc::vec::Vec;

use super::*;
use crate::consts::*;
use crate::elf::Arch;
use crate::errors::{ApplyError, RelocError};
use crate::object::ObjView;
use crate::scope::ObjId;
use elfbuild::{Builder, Image};

const SYM: u32 = 1;
const DATA: u64 = elfbuild::DATA_BASE;
const SLOT: u64 = DATA + 0x400;

fn build(
    relas: &[(u64, u32, u32, i64)],
    plt: &[(u64, u32, u32, i64)],
    bias: u64,
) -> (Image, ObjView) {
    let mut builder = Builder::new().symbol(elfbuild::global_func(b"sym", 0x100));
    for (offset, sym, r_type, addend) in relas {
        builder = builder.rela(*offset, *sym, *r_type, *addend);
    }
    for (offset, sym, r_type, addend) in plt {
        builder = builder.plt_rela(*offset, *sym, *r_type, *addend);
    }
    let image = builder.build();
    let view = ObjView::digest(&image.memory(bias), image.phdr_vaddr, image.phnum, bias).unwrap();
    (image, view)
}

struct Resolver {
    defs: Vec<Option<SymDef>>,
}

impl Resolver {
    fn with(def: Option<SymDef>) -> Self {
        Self {
            defs: vec![None, def],
        }
    }
}

impl SymResolver for Resolver {
    fn resolve(&mut self, _: ObjId, index: u32) -> Result<Option<SymDef>, RelocError> {
        Ok(self.defs.get(index as usize).copied().flatten())
    }
}

struct Applier {
    memory: Vec<u8>,
    log: Vec<(u64, u64)>,
    writes32: Vec<(u64, u32)>,
    ifunc_result: u64,
    ifunc_calls: Vec<u64>,
    tls_offset: i64,
    module: u64,
    descriptor: TlsDesc,
    copies: Vec<(u64, u64, u64)>,
}

impl Applier {
    fn new(result: u64) -> Self {
        Self {
            memory: vec![0u8; 0x20_000],
            log: Vec::new(),
            writes32: Vec::new(),
            ifunc_result: result,
            ifunc_calls: Vec::new(),
            tls_offset: 0x40,
            module: 3,
            descriptor: TlsDesc {
                resolver: 0xaaaa,
                arg: 0xbbbb,
            },
            copies: Vec::new(),
        }
    }

    fn seed64(&mut self, address: u64, value: u64) {
        let at = address as usize;
        self.memory[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn read(&self, address: u64) -> Option<u64> {
        let at = address as usize;
        let slice = self.memory.get(at..at + 8)?;
        Some(u64::from_le_bytes(slice.try_into().ok()?))
    }
}

impl RelocApplier for Applier {
    fn write64(&mut self, addr: u64, val: u64) -> Result<(), ApplyError> {
        let at = addr as usize;
        let slot = self
            .memory
            .get_mut(at..at + 8)
            .ok_or(ApplyError::OutOfRange)?;
        slot.copy_from_slice(&val.to_le_bytes());
        self.log.push((addr, val));
        Ok(())
    }

    fn write32(&mut self, addr: u64, val: u32) -> Result<(), ApplyError> {
        let at = addr as usize;
        let slot = self
            .memory
            .get_mut(at..at + 4)
            .ok_or(ApplyError::OutOfRange)?;
        slot.copy_from_slice(&val.to_le_bytes());
        self.writes32.push((addr, val));
        Ok(())
    }

    fn read64(&self, addr: u64) -> Option<u64> {
        self.read(addr)
    }

    fn call_ifunc(&mut self, resolver: u64, _: &IfuncArg) -> Result<u64, ApplyError> {
        self.ifunc_calls.push(resolver);
        Ok(self.ifunc_result)
    }

    fn tls_module_id(&self, _: ObjId) -> u64 {
        self.module
    }

    fn tls_static_offset(&mut self, _: ObjId) -> Result<i64, ApplyError> {
        Ok(self.tls_offset)
    }

    fn tls_desc_alloc(&mut self, _: ObjId, _: u64, _: i64) -> Result<TlsDesc, ApplyError> {
        Ok(self.descriptor)
    }

    fn copy(&mut self, dst: u64, src: u64, len: u64) -> Result<(), ApplyError> {
        self.copies.push((dst, src, len));
        Ok(())
    }
}

fn definition(value: u64, st_value: u64) -> SymDef {
    SymDef {
        obj: ObjId(1),
        value,
        st_value,
        size: 8,
        is_ifunc: false,
        is_tls: false,
        weak_undef: false,
    }
}

fn apply(
    arch: Arch,
    image: &Image,
    view: &ObjView,
    bias: u64,
    resolver: &mut dyn SymResolver,
    applier: &mut dyn RelocApplier,
    phase: Phase,
) -> Result<(), RelocError> {
    let pointer = image.memory(bias);
    apply_object(
        arch,
        ObjId(0),
        view,
        false,
        &pointer,
        resolver,
        applier,
        BindMode::Now,
        phase,
        &IfuncArg {
            hwcap: 0,
            hwcap2: 0,
        },
    )
}

#[test]
fn the_x86_64_table_is_the_documented_one() {
    assert!(matches!(x86_64::classify(R_X86_64_NONE), RelocAction::None));
    assert!(matches!(
        x86_64::classify(R_X86_64_64),
        RelocAction::Abs64 { sym: true }
    ));
    assert!(matches!(x86_64::classify(R_X86_64_PC32), RelocAction::Pc32));
    assert!(matches!(x86_64::classify(R_X86_64_COPY), RelocAction::Copy));
    assert!(matches!(
        x86_64::classify(R_X86_64_GLOB_DAT),
        RelocAction::GlobDat
    ));
    assert!(matches!(
        x86_64::classify(R_X86_64_JUMP_SLOT),
        RelocAction::JumpSlot
    ));
    assert!(matches!(
        x86_64::classify(R_X86_64_RELATIVE),
        RelocAction::Relative
    ));
    assert!(matches!(
        x86_64::classify(R_X86_64_DTPMOD64),
        RelocAction::DtpMod64
    ));
    assert!(matches!(
        x86_64::classify(R_X86_64_DTPOFF64),
        RelocAction::DtpOff64
    ));
    assert!(matches!(
        x86_64::classify(R_X86_64_TPOFF64),
        RelocAction::TpOff64
    ));
    assert!(matches!(
        x86_64::classify(R_X86_64_TLSDESC),
        RelocAction::TlsDesc
    ));
    assert!(matches!(
        x86_64::classify(R_X86_64_IRELATIVE),
        RelocAction::IRelative
    ));
    assert_eq!(
        x86_64::classify(R_X86_64_TPOFF32),
        RelocAction::Reject(RejectReason::NotInSharedObject)
    );
    assert_eq!(
        x86_64::classify(R_X86_64_DTPOFF32),
        RelocAction::Reject(RejectReason::NotInSharedObject)
    );
    assert_eq!(
        x86_64::classify(99),
        RelocAction::Reject(RejectReason::Unsupported)
    );
}

#[test]
fn the_aarch64_table_is_by_number() {
    assert!(matches!(
        aarch64::classify(R_AARCH64_ABS64),
        RelocAction::Abs64 { sym: true }
    ));
    assert!(matches!(
        aarch64::classify(R_AARCH64_GLOB_DAT),
        RelocAction::GlobDat
    ));
    assert!(matches!(
        aarch64::classify(R_AARCH64_JUMP_SLOT),
        RelocAction::JumpSlot
    ));
    assert!(matches!(
        aarch64::classify(R_AARCH64_RELATIVE),
        RelocAction::Relative
    ));
    assert!(matches!(
        aarch64::classify(R_AARCH64_TLS_DTPMOD64),
        RelocAction::DtpMod64
    ));
    assert!(matches!(
        aarch64::classify(R_AARCH64_TLS_DTPREL64),
        RelocAction::DtpOff64
    ));
    assert!(matches!(
        aarch64::classify(R_AARCH64_TLS_TPREL64),
        RelocAction::TpOff64
    ));
    assert!(matches!(
        aarch64::classify(R_AARCH64_TLSDESC),
        RelocAction::TlsDesc
    ));
    assert!(matches!(
        aarch64::classify(R_AARCH64_IRELATIVE),
        RelocAction::IRelative
    ));
    assert_eq!(
        aarch64::classify(1025),
        RelocAction::GlobDat,
        "1025 is GLOB_DAT"
    );
}

#[test]
fn the_riscv64_table_has_no_glob_dat() {
    assert!(matches!(
        riscv64::classify(R_RISCV_64),
        RelocAction::Abs64 { sym: true }
    ));
    assert!(matches!(
        riscv64::classify(R_RISCV_RELATIVE),
        RelocAction::Relative
    ));
    assert!(matches!(
        riscv64::classify(R_RISCV_JUMP_SLOT),
        RelocAction::JumpSlot
    ));
    assert_eq!(
        riscv64::classify(1025),
        RelocAction::Reject(RejectReason::Unsupported),
        "1025 must not be accepted as GLOB_DAT on riscv64"
    );
    assert_eq!(
        riscv64::classify(R_RISCV_TLSDESC),
        RelocAction::Reject(RejectReason::Unsupported)
    );
    assert!(matches!(
        riscv64::classify(R_RISCV_IRELATIVE),
        RelocAction::IRelative
    ));
}

#[test]
fn classify_rejects_unknown_numbers_on_every_arch() {
    for arch in [Arch::X86_64, Arch::Aarch64, Arch::Riscv64] {
        assert_eq!(
            classify(arch, 0xffff),
            RelocAction::Reject(RejectReason::Unsupported)
        );
    }
}

#[test]
fn an_absolute_relocation_writes_s_plus_a() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_64, 0x10)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x8010));
}

#[test]
fn a_pc32_relocation_writes_s_plus_a_minus_p() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_PC32, 4)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(
        applier.writes32,
        vec![(bias + SLOT, (0x8000u64 + 4 - (bias + SLOT)) as u32)]
    );
}

#[test]
fn a_relative_relocation_writes_b_plus_a() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, 0, R_X86_64_RELATIVE, 0x123)], &[], bias);
    let mut resolver = Resolver::with(None);
    let mut applier = Applier::new(0);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(bias + 0x123));
}

#[test]
fn a_glob_dat_on_x86_64_ignores_the_addend() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_GLOB_DAT, 0x99)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x8000));
}

#[test]
fn a_glob_dat_on_aarch64_adds_the_addend() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_AARCH64_GLOB_DAT, 0x99)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    apply(
        Arch::Aarch64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x8099));
}

#[test]
fn a_jump_slot_in_the_plt_range_writes_s() {
    let bias = 0x2000;
    let (image, view) = build(&[], &[(SLOT, SYM, R_X86_64_JUMP_SLOT, 0)], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::Plt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x8000));
}

#[test]
fn the_plt_phase_rejects_a_non_plt_type() {
    let bias = 0x2000;
    let (image, view) = build(&[], &[(SLOT, SYM, R_X86_64_64, 0)], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    assert_eq!(
        apply(
            Arch::X86_64,
            &image,
            &view,
            bias,
            &mut resolver,
            &mut applier,
            Phase::Plt
        ),
        Err(RelocError::Unsupported {
            arch: Arch::X86_64,
            r_type: R_X86_64_64
        })
    );
}

#[test]
fn the_non_plt_phase_does_not_touch_the_plt_range() {
    let bias = 0x2000;
    let (image, view) = build(
        &[(SLOT, SYM, R_X86_64_64, 0)],
        &[(SLOT + 8, SYM, R_X86_64_JUMP_SLOT, 0)],
        bias,
    );
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.log.len(), 1);
    assert_eq!(applier.log[0].0, bias + SLOT);
}

#[test]
fn a_jump_slot_in_the_non_plt_range_is_rejected() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_JUMP_SLOT, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    assert!(matches!(
        apply(
            Arch::X86_64,
            &image,
            &view,
            bias,
            &mut resolver,
            &mut applier,
            Phase::NonPlt
        ),
        Err(RelocError::Unsupported { .. })
    ));
}

#[test]
fn a_copy_relocation_is_allowed_in_the_main_program() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_COPY, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    let pointer = image.memory(bias);
    apply_object(
        Arch::X86_64,
        ObjId(0),
        &view,
        true,
        &pointer,
        &mut resolver,
        &mut applier,
        BindMode::Now,
        Phase::NonPlt,
        &IfuncArg {
            hwcap: 0,
            hwcap2: 0,
        },
    )
    .unwrap();
    assert_eq!(applier.copies, vec![(bias + SLOT, 0x8000, 8)]);
}

#[test]
fn a_copy_relocation_in_a_shared_object_is_rejected() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_COPY, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    assert_eq!(
        apply(
            Arch::X86_64,
            &image,
            &view,
            bias,
            &mut resolver,
            &mut applier,
            Phase::NonPlt
        ),
        Err(RelocError::UnexpectedCopy)
    );
}

#[test]
fn dtpmod_adds_the_module_id_to_the_existing_word() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_DTPMOD64, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    applier.seed64(bias + SLOT, 0x100);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x103));
}

#[test]
fn dtpoff_adds_st_value_and_the_addend_to_the_existing_word() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_DTPOFF64, 0x20)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    applier.seed64(bias + SLOT, 0x100);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x130));
}

#[test]
fn tpoff64_on_x86_64_subtracts_the_static_offset() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_TPOFF64, 0x8)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    applier.tls_offset = 0x40;
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(
        applier.read(bias + SLOT),
        Some(0x10u64.wrapping_add(0x8).wrapping_sub(0x40))
    );
}

#[test]
fn tprel64_on_aarch64_adds_the_static_offset() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_AARCH64_TLS_TPREL64, 0x8)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    applier.tls_offset = 0x40;
    apply(
        Arch::Aarch64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x10 + 0x8 + 0x40));
}

#[test]
fn tprel64_on_riscv64_subtracts_the_tp_and_tcb_terms() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_RISCV_TLS_TPREL64, 0x8)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    applier.tls_offset = 0x40;
    apply(
        Arch::Riscv64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x10 + 0x8 + 0x40 - 16));
}

#[test]
fn dtprel64_on_riscv64_subtracts_the_dtv_offset() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_RISCV_TLS_DTPREL64, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    applier.seed64(bias + SLOT, 0x1000);
    apply(
        Arch::Riscv64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x1000 + 0x10 - 0x800));
}

#[test]
fn aarch64_1028_is_the_module_id() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, 1028, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    applier.seed64(bias + SLOT, 0x100);
    apply(
        Arch::Aarch64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x103));
}

#[test]
fn aarch64_1029_is_the_offset_inside_the_module() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, 1029, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    applier.seed64(bias + SLOT, 0x100);
    apply(
        Arch::Aarch64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x110));
}

#[test]
fn a_tls_desc_writes_the_pair() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_TLSDESC, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0xaaaa));
    assert_eq!(applier.read(bias + SLOT + 8), Some(0xbbbb));
}

#[test]
fn a_tls_desc_on_riscv64_is_rejected() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_RISCV_TLSDESC, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    assert!(matches!(
        apply(
            Arch::Riscv64,
            &image,
            &view,
            bias,
            &mut resolver,
            &mut applier,
            Phase::NonPlt
        ),
        Err(RelocError::Unsupported { .. })
    ));
}

#[test]
fn an_ifunc_definition_is_deferred_and_then_resolved() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_64, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(SymDef {
        obj: ObjId(1),
        value: 0x8000,
        st_value: 0x10,
        size: 8,
        is_ifunc: true,
        is_tls: false,
        weak_undef: false,
    }));
    let mut applier = Applier::new(0x9999);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert!(
        applier.log.is_empty(),
        "an IFUNC is skipped before its phase"
    );
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::Ifunc,
    )
    .unwrap();
    assert_eq!(applier.ifunc_calls, vec![0x8000]);
    assert_eq!(applier.read(bias + SLOT), Some(0x9999));
}

#[test]
fn an_irelative_is_resolved_only_in_the_ifunc_phase() {
    let bias = 0x2000;
    let (image, view) = build(&[], &[(SLOT, 0, R_X86_64_IRELATIVE, 0x40)], bias);
    let mut resolver = Resolver::with(None);
    let mut applier = Applier::new(0x1234);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::Plt,
    )
    .unwrap();
    assert!(applier.log.is_empty());
    assert!(applier.ifunc_calls.is_empty());
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::Ifunc,
    )
    .unwrap();
    assert_eq!(applier.ifunc_calls, vec![bias + 0x40]);
    assert_eq!(applier.read(bias + SLOT), Some(0x1234));
}

#[test]
fn the_ifunc_phase_skips_every_other_relocation() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, 0, R_X86_64_RELATIVE, 0x100)], &[], bias);
    let mut resolver = Resolver::with(None);
    let mut applier = Applier::new(0);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::Ifunc,
    )
    .unwrap();
    assert!(applier.log.is_empty());
}

#[test]
fn an_ifunc_for_a_tls_relocation_is_an_error() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_DTPMOD64, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(SymDef {
        obj: ObjId(1),
        value: 0x8000,
        st_value: 0x10,
        size: 8,
        is_ifunc: true,
        is_tls: false,
        weak_undef: false,
    }));
    let mut applier = Applier::new(0);
    assert_eq!(
        apply(
            Arch::X86_64,
            &image,
            &view,
            bias,
            &mut resolver,
            &mut applier,
            Phase::NonPlt
        ),
        Err(RelocError::IfuncForTls)
    );
}

#[test]
fn a_weak_undefined_absolute_writes_the_addend() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_64, 0x55)], &[], bias);
    let mut resolver = Resolver::with(None);
    let mut applier = Applier::new(0);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0x55));
}

#[test]
fn a_weak_undefined_glob_dat_writes_zero() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_GLOB_DAT, 0x55)], &[], bias);
    let mut resolver = Resolver::with(None);
    let mut applier = Applier::new(0);
    applier.seed64(bias + SLOT, 0xff);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert_eq!(applier.read(bias + SLOT), Some(0));
}

#[test]
fn an_offset_outside_a_load_segment_fails_before_any_write() {
    let bias = 0x2000;
    let (image, view) = build(&[(0x0090_0000, SYM, R_X86_64_64, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    assert_eq!(
        apply(
            Arch::X86_64,
            &image,
            &view,
            bias,
            &mut resolver,
            &mut applier,
            Phase::NonPlt
        ),
        Err(RelocError::BadOffset)
    );
    assert!(applier.log.is_empty());
    assert!(applier.writes32.is_empty());
}

#[test]
fn apply_relr_adds_the_bias_to_each_word() {
    let bias = 0x2000;
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .relr(DATA + 0x200)
        .build();
    let view = ObjView::digest(&image.memory(bias), image.phdr_vaddr, image.phnum, bias).unwrap();
    let mut applier = Applier::new(0);
    applier.seed64(bias + DATA + 0x200, 0x100);
    apply_relr(&view, &image.memory(bias), &mut applier).unwrap();
    assert_eq!(applier.read(bias + DATA + 0x200), Some(0x100 + bias));
}

#[test]
fn a_none_relocation_writes_nothing() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, 0, R_X86_64_NONE, 0)], &[], bias);
    let mut resolver = Resolver::with(None);
    let mut applier = Applier::new(0);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::NonPlt,
    )
    .unwrap();
    assert!(applier.log.is_empty());
}

#[test]
fn every_table_row_is_reachable_through_known_types() {
    for arch in [Arch::X86_64, Arch::Aarch64, Arch::Riscv64] {
        let rows = known_types(arch);
        assert!(!rows.is_empty());
        for (name, number, action) in rows {
            assert_eq!(classify(arch, number), action, "{name}");
        }
    }
}

#[test]
fn an_unknown_type_in_a_relocation_is_rejected_by_the_walker() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, 0xffff, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    assert!(matches!(
        apply(
            Arch::X86_64,
            &image,
            &view,
            bias,
            &mut resolver,
            &mut applier,
            Phase::NonPlt
        ),
        Err(RelocError::Unsupported { .. })
    ));
}

#[test]
fn a_rela_size_that_is_not_a_multiple_of_the_record_is_bad_offset() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_64, 0)], &[], bias);
    let mut view = view;
    view.rela.1 = 25;
    let mut resolver = Resolver::with(Some(definition(0x8000, 0x10)));
    let mut applier = Applier::new(0);
    assert_eq!(
        apply(
            Arch::X86_64,
            &image,
            &view,
            bias,
            &mut resolver,
            &mut applier,
            Phase::NonPlt
        ),
        Err(RelocError::BadOffset)
    );
}

#[test]
fn an_ifunc_jump_slot_is_resolved_in_the_ifunc_phase() {
    let bias = 0x2000;
    let (image, view) = build(&[], &[(SLOT, SYM, R_X86_64_JUMP_SLOT, 0)], bias);
    let mut resolver = Resolver::with(Some(SymDef {
        obj: ObjId(1),
        value: 0x8000,
        st_value: 0x10,
        size: 8,
        is_ifunc: true,
        is_tls: false,
        weak_undef: false,
    }));
    let mut applier = Applier::new(0x7777);
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::Plt,
    )
    .unwrap();
    assert!(applier.log.is_empty(), "the IFUNC slot waits for its phase");
    apply(
        Arch::X86_64,
        &image,
        &view,
        bias,
        &mut resolver,
        &mut applier,
        Phase::Ifunc,
    )
    .unwrap();
    assert_eq!(applier.ifunc_calls, vec![0x8000]);
    assert_eq!(applier.read(bias + SLOT), Some(0x7777));
}

#[test]
fn apply_relr_handles_a_bitmap_entry() {
    let bias = 0x2000;
    let bitmap = 1u64 | (1 << 1) | (1 << 2);
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .relr(DATA + 0x200)
        .relr(bitmap)
        .build();
    let view = ObjView::digest(&image.memory(bias), image.phdr_vaddr, image.phnum, bias).unwrap();
    let mut applier = Applier::new(0);
    for index in 0..3u64 {
        applier.seed64(bias + DATA + 0x200 + index * 8, 0x10);
    }
    apply_relr(&view, &image.memory(bias), &mut applier).unwrap();
    for index in 0..3u64 {
        assert_eq!(
            applier.read(bias + DATA + 0x200 + index * 8),
            Some(0x10 + bias)
        );
    }
}

#[test]
fn an_ifunc_tls_desc_is_an_error() {
    let bias = 0x2000;
    let (image, view) = build(&[(SLOT, SYM, R_X86_64_TLSDESC, 0)], &[], bias);
    let mut resolver = Resolver::with(Some(SymDef {
        obj: ObjId(1),
        value: 0x8000,
        st_value: 0x10,
        size: 8,
        is_ifunc: true,
        is_tls: false,
        weak_undef: false,
    }));
    let mut applier = Applier::new(0);
    assert_eq!(
        apply(
            Arch::X86_64,
            &image,
            &view,
            bias,
            &mut resolver,
            &mut applier,
            Phase::NonPlt
        ),
        Err(RelocError::IfuncForTls)
    );
}
