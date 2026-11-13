// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Relocation decisions and application.
//!
//! [`classify`] is a pure table: `(arch, r_type) -> RelocAction`. [`apply_object`] walks one
//! object's relocation ranges in the phase order the loader uses (`NonPlt`, `Relr`, `Plt`,
//! `Ifunc`) and asks a [`SymResolver`] for definitions and a [`RelocApplier`] for every memory
//! access. IFUNC work is strictly last (A-ld-5): an IFUNC definition referenced by a normal
//! relocation is skipped in the earlier phases and resolved in [`Phase::Ifunc`].

pub mod aarch64;
pub mod riscv64;
pub mod x86_64;

use alloc::vec::Vec;

use crate::consts::{RELA_SIZE, TLS_DTV_OFFSET_RISCV, TLS_TCB_SIZE, TLS_TP_OFFSET_RISCV};
use crate::elf::{Arch, Rela};
use crate::errors::{ApplyError, RelocError};
use crate::limits::MAX_RELOCS_PER_OBJECT;
use crate::mem::MemView;
use crate::object::ObjView;
use crate::relr::relr_offsets;
use crate::scope::ObjId;

/// Eager or lazy binding. Lazy exists so the table is complete; C62 owns the trampoline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindMode {
    /// Resolve every slot now (the default).
    Now,
    /// Keep `JUMP_SLOT` for a PLT trampoline (behaviour is C62's feature).
    Lazy,
}

/// The phase of one object's relocation pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// `.rela.dyn` without the PLT slots.
    NonPlt,
    /// `DT_RELR` (after `.rela.dyn`).
    Relr,
    /// `.rela.plt`.
    Plt,
    /// The IFUNC pass, after every other phase of every initial object.
    Ifunc,
}

/// Why a relocation number is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectReason {
    /// Not valid in a shared object built with Raven's flags.
    NotInSharedObject,
    /// Not part of the table for this architecture.
    Unsupported,
    /// Text relocations are forbidden.
    TextRel,
}

/// What to do with one relocation number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelocAction {
    /// `R_*_NONE`: nothing.
    None,
    /// `S + A`.
    Abs64 {
        /// Whether the relocation references a symbol.
        sym: bool,
    },
    /// `S + A - P`, written as 32 bits.
    Pc32,
    /// `B + A`.
    Relative,
    /// `S` (x86_64) or `S + A` (aarch64).
    GlobDat,
    /// `S`.
    JumpSlot,
    /// `copy(dst, src, size)`, main program only.
    Copy,
    /// `module id`, added to the word at `P`.
    DtpMod64,
    /// `st_value + A`, added to the word at `P`.
    DtpOff64,
    /// The static TLS offset formula.
    TpOff64,
    /// Allocate a TLS descriptor.
    TlsDesc,
    /// `resolver(B + A)`, IFUNC phase only.
    IRelative,
    /// Refused (the reason is reported).
    Reject(RejectReason),
}

/// The table for one architecture.
pub fn classify(arch: Arch, r_type: u32) -> RelocAction {
    match arch {
        Arch::X86_64 => x86_64::classify(r_type),
        Arch::Aarch64 => aarch64::classify(r_type),
        Arch::Riscv64 => riscv64::classify(r_type),
    }
}

/// One resolved definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymDef {
    /// The defining object.
    pub obj: ObjId,
    /// Absolute value (`bias + st_value`, or `st_value` for TLS).
    pub value: u64,
    /// The raw `st_value` (the TLS formulas use it).
    pub st_value: u64,
    /// `st_size` (for `COPY`).
    pub size: u64,
    /// `STT_GNU_IFUNC`.
    pub is_ifunc: bool,
    /// `STT_TLS`.
    pub is_tls: bool,
    /// A weak undefined symbol resolved to zero.
    pub weak_undef: bool,
}

/// The symbol resolver; C62 implements it with `symlook_default` and a cache.
pub trait SymResolver {
    /// Resolve `sym_index` for `referencing`; `None` means a weak undefined symbol.
    fn resolve(&mut self, referencing: ObjId, sym_index: u32)
    -> Result<Option<SymDef>, RelocError>;
}

/// The IFUNC resolver argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IfuncArg {
    /// `AT_HWCAP`.
    pub hwcap: u64,
    /// `AT_HWCAP2`.
    pub hwcap2: u64,
}

/// The pair written into a `TLSDESC` slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TlsDesc {
    /// The resolver entry point.
    pub resolver: u64,
    /// The resolver argument.
    pub arg: u64,
}

/// The memory side of a relocation; the loader supplies the `unsafe` implementation.
pub trait RelocApplier {
    /// Write one 64-bit word.
    fn write64(&mut self, addr: u64, val: u64) -> Result<(), ApplyError>;
    /// Write one 32-bit word.
    fn write32(&mut self, addr: u64, val: u32) -> Result<(), ApplyError>;
    /// Read one 64-bit word (for the `*where += x` forms).
    fn read64(&self, addr: u64) -> Option<u64>;
    /// Call an IFUNC resolver.
    fn call_ifunc(&mut self, resolver: u64, arg: &IfuncArg) -> Result<u64, ApplyError>;
    /// The TLS module id of an object.
    fn tls_module_id(&self, obj: ObjId) -> u64;
    /// The static TLS offset of an object, allocated lazily.
    fn tls_static_offset(&mut self, obj: ObjId) -> Result<i64, ApplyError>;
    /// Allocate the descriptor for a `TLSDESC` relocation.
    fn tls_desc_alloc(
        &mut self,
        obj: ObjId,
        sym_value: u64,
        addend: i64,
    ) -> Result<TlsDesc, ApplyError>;
    /// Copy `len` bytes.
    fn copy(&mut self, dst: u64, src: u64, len: u64) -> Result<(), ApplyError>;
}

fn check_offset(obj: &ObjView, addr: u64, len: u64) -> Result<(), RelocError> {
    let end = addr.checked_add(len).ok_or(RelocError::BadOffset)?;
    let inside = obj.segments.iter().any(|segment| {
        let segment_end = segment.vaddr.saturating_add(segment.memsz);
        addr >= segment.vaddr && end <= segment_end
    });
    if inside {
        Ok(())
    } else {
        Err(RelocError::BadOffset)
    }
}

fn reject(arch: Arch, r_type: u32, reason: RejectReason) -> RelocError {
    match reason {
        RejectReason::NotInSharedObject => RelocError::NotInSharedObject,
        RejectReason::Unsupported | RejectReason::TextRel => {
            RelocError::Unsupported { arch, r_type }
        }
    }
}

struct Context<'a> {
    arch: Arch,
    id: ObjId,
    obj: &'a ObjView,
    mainprog: bool,
    res: &'a mut dyn SymResolver,
    app: &'a mut dyn RelocApplier,
    ifunc: IfuncArg,
}

fn resolve(ctx: &mut Context<'_>, sym: u32) -> Result<SymDef, RelocError> {
    match ctx.res.resolve(ctx.id, sym)? {
        Some(definition) => Ok(definition),
        None => Ok(SymDef {
            obj: ctx.id,
            value: 0,
            st_value: 0,
            size: 0,
            is_ifunc: false,
            is_tls: false,
            weak_undef: true,
        }),
    }
}

enum Outcome {
    Written,
    Deferred,
}

fn apply_one(
    ctx: &mut Context<'_>,
    rela: &Rela,
    action: RelocAction,
) -> Result<Outcome, RelocError> {
    let arch = ctx.arch;
    let r_type = rela.typ();
    let bias = ctx.obj.bias;
    let place = bias.checked_add(rela.offset).ok_or(RelocError::BadOffset)?;
    let addend = rela.addend;
    match action {
        RelocAction::None => Ok(Outcome::Deferred),
        RelocAction::Reject(reason) => Err(reject(arch, r_type, reason)),
        RelocAction::Relative => {
            check_offset(ctx.obj, place, 8)?;
            let value = bias.wrapping_add_signed(addend);
            ctx.app
                .write64(place, value)
                .map_err(|_| RelocError::BadOffset)?;
            Ok(Outcome::Written)
        }
        RelocAction::IRelative => Ok(Outcome::Deferred),
        RelocAction::Copy => {
            if !ctx.mainprog {
                return Err(RelocError::UnexpectedCopy);
            }
            let definition = resolve(ctx, rela.sym())?;
            check_offset(ctx.obj, place, definition.size)?;
            ctx.app
                .copy(place, definition.value, definition.size)
                .map_err(|_| RelocError::BadOffset)?;
            Ok(Outcome::Written)
        }
        RelocAction::TlsDesc => {
            let definition = resolve(ctx, rela.sym())?;
            if definition.is_ifunc {
                return Err(RelocError::IfuncForTls);
            }
            check_offset(ctx.obj, place, 16)?;
            let descriptor = ctx
                .app
                .tls_desc_alloc(definition.obj, definition.st_value, addend)
                .map_err(|_| RelocError::BadOffset)?;
            ctx.app
                .write64(place, descriptor.resolver)
                .map_err(|_| RelocError::BadOffset)?;
            let arg = place.checked_add(8).ok_or(RelocError::BadOffset)?;
            ctx.app
                .write64(arg, descriptor.arg)
                .map_err(|_| RelocError::BadOffset)?;
            Ok(Outcome::Written)
        }
        RelocAction::Pc32 => {
            let definition = resolve(ctx, rela.sym())?;
            check_offset(ctx.obj, place, 4)?;
            let value = definition
                .value
                .wrapping_add_signed(addend)
                .wrapping_sub(place);
            ctx.app
                .write32(place, u32::try_from(value).unwrap_or(0))
                .map_err(|_| RelocError::BadOffset)?;
            Ok(Outcome::Written)
        }
        RelocAction::Abs64 { sym } => {
            let definition = resolve(ctx, rela.sym())?;
            if definition.is_ifunc {
                // A resolver may call other objects; it runs in the IFUNC pass (A-ld-5).
                return Ok(Outcome::Deferred);
            }
            check_offset(ctx.obj, place, 8)?;
            let base = if definition.weak_undef {
                0
            } else {
                definition.value
            };
            let value = if sym {
                base.wrapping_add_signed(addend)
            } else {
                addend as u64
            };
            ctx.app
                .write64(place, value)
                .map_err(|_| RelocError::BadOffset)?;
            Ok(Outcome::Written)
        }
        RelocAction::GlobDat | RelocAction::JumpSlot => {
            let definition = resolve(ctx, rela.sym())?;
            if definition.is_ifunc {
                return Ok(Outcome::Deferred);
            }
            check_offset(ctx.obj, place, 8)?;
            let base = if definition.weak_undef {
                0
            } else {
                definition.value
            };
            let value = match action {
                RelocAction::GlobDat => {
                    if arch == Arch::X86_64 {
                        base
                    } else {
                        base.wrapping_add_signed(addend)
                    }
                }
                _ => base,
            };
            ctx.app
                .write64(place, value)
                .map_err(|_| RelocError::BadOffset)?;
            Ok(Outcome::Written)
        }
        RelocAction::DtpMod64 => {
            let definition = resolve(ctx, rela.sym())?;
            if definition.is_ifunc {
                return Err(RelocError::IfuncForTls);
            }
            check_offset(ctx.obj, place, 8)?;
            let previous = ctx.app.read64(place).ok_or(RelocError::BadOffset)?;
            let module = if definition.weak_undef {
                0
            } else {
                ctx.app.tls_module_id(definition.obj)
            };
            ctx.app
                .write64(place, previous.wrapping_add(module))
                .map_err(|_| RelocError::BadOffset)?;
            Ok(Outcome::Written)
        }
        RelocAction::DtpOff64 => {
            let definition = resolve(ctx, rela.sym())?;
            if definition.is_ifunc {
                return Err(RelocError::IfuncForTls);
            }
            check_offset(ctx.obj, place, 8)?;
            let previous = ctx.app.read64(place).ok_or(RelocError::BadOffset)?;
            let mut value = definition
                .st_value
                .wrapping_add_signed(addend)
                .wrapping_add(previous);
            if arch == Arch::Riscv64 {
                value = value.wrapping_sub(TLS_DTV_OFFSET_RISCV);
            }
            ctx.app
                .write64(place, value)
                .map_err(|_| RelocError::BadOffset)?;
            Ok(Outcome::Written)
        }
        RelocAction::TpOff64 => {
            let definition = resolve(ctx, rela.sym())?;
            if definition.is_ifunc {
                return Err(RelocError::IfuncForTls);
            }
            let offset = ctx
                .app
                .tls_static_offset(definition.obj)
                .map_err(|_| RelocError::BadOffset)?;
            check_offset(ctx.obj, place, 8)?;
            let value = match arch {
                Arch::X86_64 => definition
                    .st_value
                    .wrapping_add_signed(addend)
                    .wrapping_sub(offset as u64),
                Arch::Aarch64 => definition
                    .st_value
                    .wrapping_add_signed(addend)
                    .wrapping_add(offset as u64),
                Arch::Riscv64 => definition
                    .st_value
                    .wrapping_add_signed(addend)
                    .wrapping_add(offset as u64)
                    .wrapping_sub(TLS_TP_OFFSET_RISCV)
                    .wrapping_sub(TLS_TCB_SIZE),
            };
            ctx.app
                .write64(place, value)
                .map_err(|_| RelocError::BadOffset)?;
            Ok(Outcome::Written)
        }
    }
}

fn walk_table(
    ctx: &mut Context<'_>,
    address: u64,
    size: u64,
    mem: &dyn MemView,
    phase: Phase,
) -> Result<(), RelocError> {
    if !size.is_multiple_of(RELA_SIZE as u64) {
        return Err(RelocError::BadOffset);
    }
    let count = usize::try_from(size.checked_div(RELA_SIZE as u64).unwrap_or(u64::MAX))
        .unwrap_or(usize::MAX);
    if count > MAX_RELOCS_PER_OBJECT {
        return Err(RelocError::TooManyRelocs);
    }
    for index in 0..count {
        let at = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_mul(RELA_SIZE as u64))
            .and_then(|value| address.checked_add(value))
            .ok_or(RelocError::BadOffset)?;
        let bytes = mem.read(at, RELA_SIZE).ok_or(RelocError::BadOffset)?;
        let rela = Rela::parse(bytes).map_err(|_| RelocError::BadOffset)?;
        let action = classify(ctx.arch, rela.typ());
        let is_plt = matches!(action, RelocAction::JumpSlot | RelocAction::IRelative);
        if phase == Phase::Plt {
            if !is_plt {
                return Err(reject(ctx.arch, rela.typ(), RejectReason::Unsupported));
            }
            if matches!(action, RelocAction::JumpSlot) {
                apply_one(ctx, &rela, action)?;
            }
            continue;
        }
        if is_plt {
            // A PLT-shaped relocation outside `.rela.plt` is not a normal non-PLT entry.
            if matches!(action, RelocAction::JumpSlot) {
                return Err(reject(ctx.arch, rela.typ(), RejectReason::Unsupported));
            }
            continue;
        }
        apply_one(ctx, &rela, action)?;
    }
    Ok(())
}

/// Apply the IFUNC pass: `IRELATIVE` entries and every normal relocation whose definition is an
/// IFUNC.
fn walk_ifunc(
    ctx: &mut Context<'_>,
    address: u64,
    size: u64,
    mem: &dyn MemView,
) -> Result<(), RelocError> {
    if !size.is_multiple_of(RELA_SIZE as u64) {
        return Err(RelocError::BadOffset);
    }
    let count = usize::try_from(size.checked_div(RELA_SIZE as u64).unwrap_or(u64::MAX))
        .unwrap_or(usize::MAX);
    for index in 0..count {
        let at = u64::try_from(index)
            .ok()
            .and_then(|value| value.checked_mul(RELA_SIZE as u64))
            .and_then(|value| address.checked_add(value))
            .ok_or(RelocError::BadOffset)?;
        let bytes = mem.read(at, RELA_SIZE).ok_or(RelocError::BadOffset)?;
        let rela = Rela::parse(bytes).map_err(|_| RelocError::BadOffset)?;
        let action = classify(ctx.arch, rela.typ());
        let place = ctx
            .obj
            .bias
            .checked_add(rela.offset)
            .ok_or(RelocError::BadOffset)?;
        match action {
            RelocAction::IRelative => {
                check_offset(ctx.obj, place, 8)?;
                let resolver = ctx.obj.bias.wrapping_add_signed(rela.addend);
                let target = ctx
                    .app
                    .call_ifunc(resolver, &ctx.ifunc)
                    .map_err(|_| RelocError::BadOffset)?;
                ctx.app
                    .write64(place, target)
                    .map_err(|_| RelocError::BadOffset)?;
            }
            RelocAction::Abs64 { .. } | RelocAction::GlobDat | RelocAction::JumpSlot => {
                let definition = resolve(ctx, rela.sym())?;
                if !definition.is_ifunc {
                    continue;
                }
                check_offset(ctx.obj, place, 8)?;
                let target = ctx
                    .app
                    .call_ifunc(definition.value, &ctx.ifunc)
                    .map_err(|_| RelocError::BadOffset)?;
                let value = match action {
                    RelocAction::JumpSlot => target,
                    _ => target.wrapping_add_signed(rela.addend),
                };
                ctx.app
                    .write64(place, value)
                    .map_err(|_| RelocError::BadOffset)?;
            }
            RelocAction::Reject(reason) => return Err(reject(ctx.arch, rela.typ(), reason)),
            _ => {}
        }
    }
    Ok(())
}

/// Apply one object's relocations for one phase.
///
/// `mainprog` allows `COPY` relocations; `mode` is advisory (lazy binding is C62's feature).
#[allow(clippy::too_many_arguments)] // the loader's per-object context
pub fn apply_object(
    arch: Arch,
    id: ObjId,
    obj: &ObjView,
    mainprog: bool,
    mem: &dyn MemView,
    res: &mut dyn SymResolver,
    app: &mut dyn RelocApplier,
    mode: BindMode,
    phase: Phase,
    ifunc: &IfuncArg,
) -> Result<(), RelocError> {
    let _ = mode;
    let mut context = Context {
        arch,
        id,
        obj,
        mainprog,
        res,
        app,
        ifunc: *ifunc,
    };
    match phase {
        Phase::NonPlt => walk_table(&mut context, obj.rela.0, obj.rela.1, mem, Phase::NonPlt),
        Phase::Plt => walk_table(
            &mut context,
            obj.plt_rela.0,
            obj.plt_rela.1,
            mem,
            Phase::Plt,
        ),
        Phase::Relr => apply_relr(obj, mem, app),
        Phase::Ifunc => {
            walk_ifunc(&mut context, obj.rela.0, obj.rela.1, mem)?;
            walk_ifunc(&mut context, obj.plt_rela.0, obj.plt_rela.1, mem)
        }
    }
}

/// Apply `DT_RELR`: `*(bias + offset) += bias`.
pub fn apply_relr(
    obj: &ObjView,
    mem: &dyn MemView,
    app: &mut dyn RelocApplier,
) -> Result<(), RelocError> {
    for offset in relr_offsets(obj, mem)? {
        let offset = offset?;
        let place = obj.bias.checked_add(offset).ok_or(RelocError::BadOffset)?;
        let previous = app.read64(place).ok_or(RelocError::BadOffset)?;
        app.write64(place, previous.wrapping_add(obj.bias))
            .map_err(|_| RelocError::BadOffset)?;
    }
    Ok(())
}

/// Every relocation type this crate knows, for the documentation table.
pub fn known_types(arch: Arch) -> Vec<(&'static str, u32, RelocAction)> {
    match arch {
        Arch::X86_64 => x86_64::table(),
        Arch::Aarch64 => aarch64::table(),
        Arch::Riscv64 => riscv64::table(),
    }
}

#[cfg(test)]
#[path = "../reloc_ut.rs"]
mod tests;
