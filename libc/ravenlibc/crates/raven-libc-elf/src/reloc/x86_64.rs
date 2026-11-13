// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright 1996, 1997, 1998, 1999 John D. Polstra.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/amd64/reloc.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS OR
// IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES
// OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED.
// IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY DIRECT, INDIRECT,
// INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
// NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
// THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
//! The x86_64 relocation table.
//!
//! `TPOFF32` and `DTPOFF32` are not valid in a shared object built with Raven's flags; the donor
//! handles them, the area document rejects them (`RD-new`, 13 4.6), so this table follows the
//! document.

use alloc::vec::Vec;

use crate::consts::{
    R_X86_64_64, R_X86_64_COPY, R_X86_64_DTPMOD64, R_X86_64_DTPOFF32, R_X86_64_DTPOFF64,
    R_X86_64_GLOB_DAT, R_X86_64_IRELATIVE, R_X86_64_JUMP_SLOT, R_X86_64_NONE, R_X86_64_PC32,
    R_X86_64_RELATIVE, R_X86_64_TLSDESC, R_X86_64_TPOFF32, R_X86_64_TPOFF64,
};

use super::{RejectReason, RelocAction};

/// Classify one x86_64 relocation number.
pub fn classify(r_type: u32) -> RelocAction {
    match r_type {
        R_X86_64_NONE => RelocAction::None,
        R_X86_64_64 => RelocAction::Abs64 { sym: true },
        R_X86_64_PC32 => RelocAction::Pc32,
        R_X86_64_COPY => RelocAction::Copy,
        R_X86_64_GLOB_DAT => RelocAction::GlobDat,
        R_X86_64_JUMP_SLOT => RelocAction::JumpSlot,
        R_X86_64_RELATIVE => RelocAction::Relative,
        R_X86_64_DTPMOD64 => RelocAction::DtpMod64,
        R_X86_64_DTPOFF64 => RelocAction::DtpOff64,
        R_X86_64_TPOFF64 => RelocAction::TpOff64,
        R_X86_64_TLSDESC => RelocAction::TlsDesc,
        R_X86_64_IRELATIVE => RelocAction::IRelative,
        R_X86_64_DTPOFF32 | R_X86_64_TPOFF32 => {
            RelocAction::Reject(RejectReason::NotInSharedObject)
        }
        _ => RelocAction::Reject(RejectReason::Unsupported),
    }
}

/// The documented table, for tests and diagnostics.
pub fn table() -> Vec<(&'static str, u32, RelocAction)> {
    let mut rows = Vec::new();
    for (name, number) in [
        ("R_X86_64_NONE", R_X86_64_NONE),
        ("R_X86_64_64", R_X86_64_64),
        ("R_X86_64_PC32", R_X86_64_PC32),
        ("R_X86_64_COPY", R_X86_64_COPY),
        ("R_X86_64_GLOB_DAT", R_X86_64_GLOB_DAT),
        ("R_X86_64_JUMP_SLOT", R_X86_64_JUMP_SLOT),
        ("R_X86_64_RELATIVE", R_X86_64_RELATIVE),
        ("R_X86_64_DTPMOD64", R_X86_64_DTPMOD64),
        ("R_X86_64_DTPOFF64", R_X86_64_DTPOFF64),
        ("R_X86_64_TPOFF64", R_X86_64_TPOFF64),
        ("R_X86_64_DTPOFF32", R_X86_64_DTPOFF32),
        ("R_X86_64_TPOFF32", R_X86_64_TPOFF32),
        ("R_X86_64_TLSDESC", R_X86_64_TLSDESC),
        ("R_X86_64_IRELATIVE", R_X86_64_IRELATIVE),
    ] {
        rows.push((name, number, classify(number)));
    }
    rows
}
