// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2014-2015 The FreeBSD Foundation
//
// Portions of this software were developed by Andrew Turner
// under sponsorship from the FreeBSD Foundation.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/aarch64/reloc.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
//! The aarch64 relocation table.
//!
//! The numbers are the linkers' numbers, not the psABI labels: 1028 is the module id and 1029 is
//! the offset inside the module (A-ld-21).

use alloc::vec::Vec;

use crate::consts::{
    R_AARCH64_ABS64, R_AARCH64_COPY, R_AARCH64_GLOB_DAT, R_AARCH64_IRELATIVE, R_AARCH64_JUMP_SLOT,
    R_AARCH64_NONE, R_AARCH64_RELATIVE, R_AARCH64_TLS_DTPMOD64, R_AARCH64_TLS_DTPREL64,
    R_AARCH64_TLS_TPREL64, R_AARCH64_TLSDESC,
};

use super::{RejectReason, RelocAction};

/// Classify one aarch64 relocation number.
pub fn classify(r_type: u32) -> RelocAction {
    match r_type {
        R_AARCH64_NONE => RelocAction::None,
        R_AARCH64_ABS64 => RelocAction::Abs64 { sym: true },
        R_AARCH64_COPY => RelocAction::Copy,
        R_AARCH64_GLOB_DAT => RelocAction::GlobDat,
        R_AARCH64_JUMP_SLOT => RelocAction::JumpSlot,
        R_AARCH64_RELATIVE => RelocAction::Relative,
        R_AARCH64_TLS_DTPMOD64 => RelocAction::DtpMod64,
        R_AARCH64_TLS_DTPREL64 => RelocAction::DtpOff64,
        R_AARCH64_TLS_TPREL64 => RelocAction::TpOff64,
        R_AARCH64_TLSDESC => RelocAction::TlsDesc,
        R_AARCH64_IRELATIVE => RelocAction::IRelative,
        _ => RelocAction::Reject(RejectReason::Unsupported),
    }
}

/// The documented table, for tests and diagnostics.
pub fn table() -> Vec<(&'static str, u32, RelocAction)> {
    let mut rows = Vec::new();
    for (name, number) in [
        ("R_AARCH64_NONE", R_AARCH64_NONE),
        ("R_AARCH64_ABS64", R_AARCH64_ABS64),
        ("R_AARCH64_COPY", R_AARCH64_COPY),
        ("R_AARCH64_GLOB_DAT", R_AARCH64_GLOB_DAT),
        ("R_AARCH64_JUMP_SLOT", R_AARCH64_JUMP_SLOT),
        ("R_AARCH64_RELATIVE", R_AARCH64_RELATIVE),
        ("R_AARCH64_TLS_DTPMOD64", R_AARCH64_TLS_DTPMOD64),
        ("R_AARCH64_TLS_DTPREL64", R_AARCH64_TLS_DTPREL64),
        ("R_AARCH64_TLS_TPREL64", R_AARCH64_TLS_TPREL64),
        ("R_AARCH64_TLSDESC", R_AARCH64_TLSDESC),
        ("R_AARCH64_IRELATIVE", R_AARCH64_IRELATIVE),
    ] {
        rows.push((name, number, classify(number)));
    }
    rows
}
