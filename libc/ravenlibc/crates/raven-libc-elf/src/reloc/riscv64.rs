// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright (c) 2015-2017 Ruslan Bukin <br@bsdpad.com>
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// This software was developed by SRI International and the University of
// Cambridge Computer Laboratory under DARPA/AFRL contract FA8750-10-C-0237
// ("CTSRD"), as part of the DARPA CRASH research programme.
//
// This software was developed by the University of Cambridge Computer
// Laboratory as part of the CTSRD Project, with support from the UK Higher
// Education Innovation Fund (HEIF).
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/riscv/reloc.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
//! The riscv64 relocation table.
//!
//! There is no `GLOB_DAT` on riscv64 (T-8); global data references use `R_RISCV_64` against the
//! symbol. The donor has no `TLSDESC`; the number is rejected (O-3).

use alloc::vec::Vec;

use crate::consts::{
    R_RISCV_64, R_RISCV_COPY, R_RISCV_IRELATIVE, R_RISCV_JUMP_SLOT, R_RISCV_NONE, R_RISCV_RELATIVE,
    R_RISCV_TLS_DTPMOD64, R_RISCV_TLS_DTPREL64, R_RISCV_TLS_TPREL64, R_RISCV_TLSDESC,
};

use super::{RejectReason, RelocAction};

/// Classify one riscv64 relocation number.
pub fn classify(r_type: u32) -> RelocAction {
    match r_type {
        R_RISCV_NONE => RelocAction::None,
        R_RISCV_64 => RelocAction::Abs64 { sym: true },
        R_RISCV_RELATIVE => RelocAction::Relative,
        R_RISCV_COPY => RelocAction::Copy,
        R_RISCV_JUMP_SLOT => RelocAction::JumpSlot,
        R_RISCV_TLS_DTPMOD64 => RelocAction::DtpMod64,
        R_RISCV_TLS_DTPREL64 => RelocAction::DtpOff64,
        R_RISCV_TLS_TPREL64 => RelocAction::TpOff64,
        R_RISCV_IRELATIVE => RelocAction::IRelative,
        R_RISCV_TLSDESC => RelocAction::Reject(RejectReason::Unsupported),
        _ => RelocAction::Reject(RejectReason::Unsupported),
    }
}

/// The documented table, for tests and diagnostics.
pub fn table() -> Vec<(&'static str, u32, RelocAction)> {
    let mut rows = Vec::new();
    for (name, number) in [
        ("R_RISCV_NONE", R_RISCV_NONE),
        ("R_RISCV_64", R_RISCV_64),
        ("R_RISCV_RELATIVE", R_RISCV_RELATIVE),
        ("R_RISCV_COPY", R_RISCV_COPY),
        ("R_RISCV_JUMP_SLOT", R_RISCV_JUMP_SLOT),
        ("R_RISCV_TLS_DTPMOD64", R_RISCV_TLS_DTPMOD64),
        ("R_RISCV_TLS_DTPREL64", R_RISCV_TLS_DTPREL64),
        ("R_RISCV_TLS_TPREL64", R_RISCV_TLS_TPREL64),
        ("R_RISCV_TLSDESC", R_RISCV_TLSDESC),
        ("R_RISCV_IRELATIVE", R_RISCV_IRELATIVE),
    ] {
        rows.push((name, number, classify(number)));
    }
    rows
}
