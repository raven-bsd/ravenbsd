// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the signal context layout.

use super::*;

#[test]
fn zero_context_is_empty() {
    let context = MContext::zero();
    assert_eq!(context.pc(), 0);
    assert_eq!(context.sp(), 0);
    assert_eq!(context.reg(0), Some(0));
    assert_eq!(context.reg(usize::MAX), None);
}

#[test]
fn pc_and_sp_roundtrip() {
    let mut context = MContext::zero();
    context.set_pc(0x1234_5678_9abc_def0);
    context.set_sp(0x0fff_ffff_ffff_0000);
    assert_eq!(context.pc(), 0x1234_5678_9abc_def0);
    assert_eq!(context.sp(), 0x0fff_ffff_ffff_0000);
}

#[test]
fn context_size_matches_c36() {
    assert_eq!(core::mem::size_of::<MContext>(), MCONTEXT_SIZE);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn x86_64_register_indices_match_the_c_header() {
    let mut context = MContext::zero();
    context.gregs[REG_RDI] = 7;
    context.gregs[REG_RAX] = 11;
    assert_eq!(context.reg(REG_RDI), Some(7));
    assert_eq!(context.reg(REG_RAX), Some(11));
    assert_eq!(context.gregs.len(), 23);
}

#[cfg(target_arch = "aarch64")]
#[test]
fn aarch64_fields_match_the_c_header() {
    let mut context = MContext::zero();
    context.regs[0] = 7;
    context.regs[30] = 9;
    context.fault_address = 0x1000;
    assert_eq!(context.reg(0), Some(7));
    assert_eq!(context.reg(30), Some(9));
    assert_eq!(context.fault_address, 0x1000);
    assert_eq!(context.regs.len(), 31);
}
