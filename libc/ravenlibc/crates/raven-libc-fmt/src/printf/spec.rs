// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Chris Torek.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/stdio/printflocal.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! Conversion flags, length modifiers and the argument-slot type table.
//!
//! `union arg` became [`ArgVal`]: floating-point values are stored as bits and never as
//! `f64`/`long double` (T5). `STATIC_ARG_TBL_SIZE` and the growable table of the donor
//! collapse into one fixed `[ArgVal; NL_ARGMAX + 1]` (`donor-map/04` 5.1.2).

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The donor's int arithmetic is mirrored statement for statement; every output-size
// addition has its explicit INT_MAX check at the call site.

use crate::types::NL_ARGMAX;

/// The default float precision (donor `DEFPREC`).
pub(crate) const DEFPREC: i32 = 6;

/// Space for a `uintmax_t` in base 2 (donor `BUF`).
pub(crate) const INT_BUF: usize = 64;

/// The donor's `STATIC_ARG_TBL_SIZE`; used only as a documentation anchor for the fixed
/// table size (the Rust table is always [`crate::types::NL_ARGMAX`] entries).
pub(crate) const STATIC_ARG_TBL_SIZE: usize = 8;

const _: () = assert!(STATIC_ARG_TBL_SIZE <= NL_ARGMAX);

/// `ALT` flag: alternate form.
pub(crate) const ALT: u32 = 0x001;
/// `LADJUST`: left adjustment.
pub(crate) const LADJUST: u32 = 0x004;
/// `LONGDBL`: `long double` argument.
pub(crate) const LONGDBL: u32 = 0x008;
/// `LONGINT`: `long` argument.
pub(crate) const LONGINT: u32 = 0x010;
/// `LLONGINT`: `long long` argument.
pub(crate) const LLONGINT: u32 = 0x020;
/// `SHORTINT`: `short` argument.
pub(crate) const SHORTINT: u32 = 0x040;
/// `ZEROPAD`: zero padding.
pub(crate) const ZEROPAD: u32 = 0x080;
/// `FPT`: the conversion is floating point.
pub(crate) const FPT: u32 = 0x100;
/// `GROUPING`: the `'` flag.
pub(crate) const GROUPING: u32 = 0x200;
/// `SIZET`: `size_t` argument.
pub(crate) const SIZET: u32 = 0x400;
/// `PTRDIFFT`: `ptrdiff_t` argument.
pub(crate) const PTRDIFFT: u32 = 0x800;
/// `INTMAXT`: `intmax_t` argument.
pub(crate) const INTMAXT: u32 = 0x1000;
/// `CHARINT`: `signed char` argument.
pub(crate) const CHARINT: u32 = 0x2000;
/// `FASTINT`: the C23 `%wfN` family selected an `int_fastN_t`.
pub(crate) const FASTINT: u32 = 0x4000;

/// `INTMAX_SIZE`: the conversions that read an `intmax_t`/`uintmax_t`.
pub(crate) const INTMAX_SIZE: u32 = INTMAXT | SIZET | PTRDIFFT | LLONGINT;

/// One argument slot of the positional table (the donor's `union arg`).
///
/// The tag lives in a parallel array so the 64-entry table stays 16 bytes per
/// slot: `long double` is split into two words.
#[derive(Clone, Copy)]
pub(crate) struct ArgVal {
    /// The low word (integer/pointer bits, or the low half of a `long double`).
    pub(crate) a: u64,
    /// The high half of a `long double`.
    pub(crate) b: u64,
}

/// The class of one [`ArgVal`].
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub(crate) enum ArgTag {
    /// An integer or pointer-sized value.
    Int,
    /// The bits of a `double`.
    F64,
    /// The image of a `long double` split over two words.
    Ld,
    /// A pointer value.
    Ptr,
    /// A `wint_t` argument.
    WInt,
}

impl ArgVal {
    /// A word slot.
    pub(crate) const fn word(v: u64) -> Self {
        Self { a: v, b: 0 }
    }

    /// A `long double` slot.
    pub(crate) const fn wide(v: u128) -> Self {
        Self {
            a: v as u64,
            b: (v >> 64) as u64,
        }
    }

    /// The `long double` image.
    pub(crate) const fn ld(self) -> u128 {
        (self.a as u128) | ((self.b as u128) << 64)
    }
}

/// Type ids of the positional type table, in the donor's order (`printf-pos.c:46-55`).
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub(crate) enum TypeId {
    /// Unused slot; a gap.
    Unused,
    /// `short *`.
    PShort,
    /// `int`.
    Int,
    /// `unsigned int`.
    UInt,
    /// `int *`.
    PInt,
    /// `long`.
    Long,
    /// `unsigned long`.
    ULong,
    /// `long *`.
    PLong,
    /// `long long`.
    LLong,
    /// `unsigned long long`.
    ULLong,
    /// `long long *`.
    PLLong,
    /// `ptrdiff_t`.
    PtrDiffT,
    /// `ptrdiff_t *`.
    PPtrDiffT,
    /// `ssize_t`.
    SSizeT,
    /// `size_t`.
    SizeT,
    /// `ssize_t *`.
    PSSizeT,
    /// `intmax_t`.
    IntMaxT,
    /// `uintmax_t`.
    UIntMaxT,
    /// `intmax_t *`.
    PIntMaxT,
    /// `void *`.
    PVoid,
    /// `char *`.
    PChar,
    /// `signed char *`.
    PSChar,
    /// `double`.
    Double,
    /// `long double`.
    LongDouble,
    /// `wint_t`.
    WInt,
    /// `wchar_t *`.
    PWChar,
}

impl TypeId {
    /// The argument class of this id.
    pub(crate) fn arg(self) -> ArgClass {
        match self {
            Self::PShort
            | Self::PInt
            | Self::PLong
            | Self::PLLong
            | Self::PPtrDiffT
            | Self::PSSizeT
            | Self::PIntMaxT
            | Self::PVoid
            | Self::PChar
            | Self::PSChar
            | Self::PWChar => ArgClass::Pointer,
            Self::Double => ArgClass::Double,
            Self::LongDouble => ArgClass::LongDouble,
            Self::WInt => ArgClass::WInt,
            Self::Unused => ArgClass::Unused,
            _ => ArgClass::Integer,
        }
    }
}

/// The reader call needed to fetch an argument class.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub(crate) enum ArgClass {
    /// No argument.
    Unused,
    /// `next_u64`.
    Integer,
    /// `next_f64_bits`.
    Double,
    /// `next_ld_bits`.
    LongDouble,
    /// `next_u64` as an address.
    Pointer,
    /// `next_u64` low 32 bits.
    WInt,
}

/// The fixed positional type table (`donor-map/04` 5.1.2).
pub(crate) struct TypeTable {
    /// One entry per 1-based argument number.
    pub(crate) types: [TypeId; NL_ARGMAX + 1],
    /// The largest used index.
    pub(crate) tablemax: u32,
    /// The next 1-based argument number while scanning.
    pub(crate) nextarg: u32,
}

impl TypeTable {
    /// A table with every slot unused.
    pub(crate) const fn new() -> Self {
        Self {
            types: [TypeId::Unused; NL_ARGMAX + 1],
            tablemax: 0,
            nextarg: 1,
        }
    }

    /// The donor's `_ensurespace` + `addtype`, with the fixed limit.
    pub(crate) fn add(&mut self, id: TypeId) -> Result<(), crate::types::FmtErr> {
        let index = self.nextarg as usize;
        if index == 0 || index > NL_ARGMAX {
            return Err(crate::types::FmtErr::Invalid);
        }
        let slot = self
            .types
            .get_mut(index)
            .ok_or(crate::types::FmtErr::Invalid)?;
        if *slot == TypeId::Unused {
            *slot = id;
        } else if *slot != id {
            // Two different ids for one argument number: EINVAL (T18).
            return Err(crate::types::FmtErr::Invalid);
        }
        if self.nextarg > self.tablemax {
            self.tablemax = self.nextarg;
        }
        self.nextarg = self.nextarg.saturating_add(1);
        Ok(())
    }
}

/// The result of the positional pre-scan.
pub(crate) struct Analysis {
    /// The type table when the format used positional arguments.
    pub(crate) table: Option<TypeTable>,
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "spec_ut.rs"]
mod tests;
