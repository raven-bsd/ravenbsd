// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the exec start frame (CONTRACTS C8, S168b).
//!
//! C8 makes the argument register and the stack register carry the same `sp0`. Raven's loader
//! hands the kernel an already 16-byte-aligned C8 block, so the stack pointer is installed
//! unchanged; the donor alignment expressions below are documented as behaviour only and are not
//! applied to Raven's stack.

use super::*;

/// A real loader `sp0`: 16-byte aligned and inside the user range.
const C8_SP0: u64 = 0x0000_7fff_ffff_f000;

#[cfg(target_arch = "x86_64")]
#[test]
fn x86_exec_frame_carries_c8_stack_in_rdi_and_rsp() {
    let mut frame = crate::arch::x86_64::trapframe::TrapFrame::default();
    frame.rdi = 0xdead_beef;
    frame.rsp = 0xfeed_face;
    frame.rax = 0x1111;

    apply_exec_context(
        &mut frame,
        ExecContext {
            entry: 0x40_0000,
            stack: C8_SP0,
        },
    );

    assert_eq!(frame.rdi, C8_SP0, "C8 argument register carries sp0");
    assert_eq!(frame.rsp, C8_SP0, "C8 stack pointer stays at sp0");
    assert_eq!(frame.rip, 0x40_0000, "entry point");
    assert_eq!(frame.rax, 0, "other registers are cleared");
}

#[cfg(target_arch = "aarch64")]
#[test]
fn aarch64_exec_frame_carries_c8_stack_in_x0_and_sp() {
    let mut frame = crate::arch::aarch64::vectors::TrapFrame {
        x: [0xdead_beef; 31],
        sp: 0xfeed_face,
        elr: 0,
        spsr: 0,
        esr: 0,
        far: 0,
    };

    apply_exec_context(
        &mut frame,
        ExecContext {
            entry: 0x40_0000,
            stack: C8_SP0,
        },
    );

    assert_eq!(frame.x[0], C8_SP0, "C8 argument register carries sp0");
    assert_eq!(frame.sp, C8_SP0, "C8 stack pointer stays at sp0");
    assert_eq!(frame.elr, 0x40_0000, "entry point");
    assert_eq!(frame.x[1], 0, "other registers are cleared");
}

/// The donor amd64 and arm64 alignment expressions produce a pointer that is not the raw
/// argument. This documents the donor shape on an unaligned synthetic input; Raven installs the
/// aligned loader `sp0` unchanged and must not apply either expression.
#[test]
fn donor_exec_alignment_formulas_differ_from_the_raw_stack() {
    let unaligned = 0x0000_7fff_ffff_f001u64;
    let amd64 = ((unaligned - 8) & !0xF_u64) + 8;
    let arm64 = unaligned & !0xF_u64;

    assert_eq!(amd64, 0x0000_7fff_ffff_eff8);
    assert_eq!(arm64, 0x0000_7fff_ffff_f000);
    assert_ne!(amd64, unaligned);
    assert_ne!(arm64, unaligned);
    assert_ne!(amd64, arm64);
}
