// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:arch/{x86_64,aarch64,riscv64}/reloc.h
// @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (donor files have no per-file header; covered by musl/COPYRIGHT)
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! The relocation numbers of the self-relocator, one set per architecture.
//!
//! `selfreloc.rs` compares `R_TYPE(info)` with [`REL_RELATIVE`] and [`REL_IRELATIVE`]. The
//! donor `reloc.h` declares a dozen `REL_*` names for its whole `dynlink.c`; the self-relocator
//! needs two of them. `REL_RELATIVE` is the donor constant for each architecture; the
//! `IRELATIVE` numbers are the ELF values of `donor-map/13` 3.6.1-3.6.4, not musl names.
//!
//! This file is included by the loader (`ld/src/lib.rs`) through `#[path]` exactly like
//! `selfreloc.rs`: no `crate::` paths, no external items, no allocation.

/// `R_TYPE(info)` of `musl:src/internal/dynlink.h:21`: the low 31 bits of `r_info`.
#[inline(always)]
pub const fn r_type(info: usize) -> usize {
    info & 0x7fff_ffff
}

/// `R_*_RELATIVE`: the load bias is added to the addend (`base + addend`).
#[cfg(target_arch = "x86_64")]
pub const REL_RELATIVE: usize = 8;
/// `R_*_RELATIVE`: the load bias is added to the addend (`base + addend`).
#[cfg(target_arch = "aarch64")]
pub const REL_RELATIVE: usize = 1027;
/// `R_*_RELATIVE`: the load bias is added to the addend (`base + addend`).
#[cfg(target_arch = "riscv64")]
pub const REL_RELATIVE: usize = 3;

/// `R_*_IRELATIVE`: the addend names the resolver (`base + addend`), its result is stored.
#[cfg(target_arch = "x86_64")]
pub const REL_IRELATIVE: usize = 37;
/// `R_*_IRELATIVE`: the addend names the resolver (`base + addend`), its result is stored.
#[cfg(target_arch = "aarch64")]
pub const REL_IRELATIVE: usize = 1032;
/// `R_*_IRELATIVE`: the addend names the resolver (`base + addend`), its result is stored.
#[cfg(target_arch = "riscv64")]
pub const REL_IRELATIVE: usize = 58;

/// x86-64 architectural constant, for the host tests and for `ld/`'s tables.
pub const REL_RELATIVE_X86_64: usize = 8;
/// x86-64 architectural constant.
pub const REL_IRELATIVE_X86_64: usize = 37;
/// AArch64 architectural constant.
pub const REL_RELATIVE_AARCH64: usize = 1027;
/// AArch64 architectural constant.
pub const REL_IRELATIVE_AARCH64: usize = 1032;
/// RISC-V 64 architectural constant.
pub const REL_RELATIVE_RISCV64: usize = 3;
/// RISC-V 64 architectural constant.
pub const REL_IRELATIVE_RISCV64: usize = 58;

#[cfg(test)]
#[path = "selfreloc_consts_ut.rs"]
mod tests;
