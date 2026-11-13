// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The pure logic of `ld-elf.so.1`: an ELF64 object view, `DT_HASH`/`DT_GNU_HASH` lookup,
//! symbol-version matching, scope walking, the relocation decision tables of x86_64, aarch64
//! and riscv64, `DT_RELR`, init/fini order, segment plans, library search paths and the
//! `LD_*` environment.
//!
//! Everything reads an image through [`MemView`] with checked addresses; nothing here holds a
//! pointer, opens a file, takes a lock or writes process memory. The loader binary
//! (`raven-libc-rtld`) owns the `unsafe` side of [`reloc::RelocApplier`] and the OS seam.
//! Behaviour and the donor translation key: `architecture/raven-libc/donor-map/13-dynamic-loader.md`.
//!
//! The crate is `no_std` + `alloc`, class P: no `unsafe` at all, `Result` from every parser.
//! Determinism is a contract: no clock, no environment and no randomness; the same bytes give
//! the same output, and all iteration is in file order.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

extern crate alloc;
// The test image builder is shared with the integration tests, where this crate is reached by
// its external name; the alias makes the same path work inside the crate.
extern crate self as raven_libc_elf;

pub mod consts;
pub mod debug;
pub mod elf;
pub mod env;
pub mod errors;
pub mod hash;
pub mod initorder;
pub mod limits;
pub mod lookup;
pub mod mem;
pub mod object;
pub mod phdr_info;
pub mod reloc;
pub mod relr;
pub mod scope;
pub mod search;
pub mod segments;
pub mod trace;
pub mod version;

/// The deterministic ELF image writer shared by the unit tests and the integration tests.
#[cfg(test)]
#[path = "../tests/common/elfbuild.rs"]
mod testbuild;

pub use debug::DebugWords;
pub use elf::{Arch, Dyn, Ehdr, Note, Phdr, Rela, Sym, Verdaux, Verdef, Vernaux, Verneed};
pub use env::EnvConfig;
pub use errors::{
    ApplyError, DigestError, ElfError, LookupError, RelocError, SearchError, VersionError,
};
pub use hash::{elf_hash, gnu_hash};
pub use initorder::{InitKind, fini_order, init_array_entries, init_order};
pub use lookup::{GnuHash, SymFlags, SymHit, SymLook, SysvHash, lookup_obj, matched_symbol};
pub use mem::{MemSet, MemView, SliceMem};
pub use object::{LoadSeg, ObjView, TlsTemplate};
pub use phdr_info::PhdrInfo;
pub use reloc::{
    BindMode, IfuncArg, Phase, RejectReason, RelocAction, RelocApplier, SymDef, SymResolver,
    TlsDesc, apply_object, apply_relr,
};
pub use relr::{RelrIter, relr_offsets};
pub use scope::{
    ObjId, ObjTable, Scope, symlook_default, symlook_global, symlook_list, symlook_needed,
};
pub use search::{RefObj, SearchCtx, expand_search, origin_subst};
pub use segments::{MapStep, SegmentPlan, StepKind, plan_segments};
pub use trace::{TraceEntry, format_trace};
pub use version::{VerEntry, fetch_ventry, locate_dependency, verify_object_versions};
