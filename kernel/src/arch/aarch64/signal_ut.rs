// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the aarch64 signal-frame layout (C19, S160 table, NetBSD donor).

use super::*;
use core::mem::{align_of, offset_of, size_of};

#[test]
fn sigcode_is_the_c19_trampoline() {
    assert_eq!(SIGRETURN, 0x82);
    assert_eq!(SIGCODE_WORDS, [0xD280_1048, 0xD400_0001]);
    assert_eq!(SIGCODE, [0x48, 0x10, 0x80, 0xd2, 0x01, 0x00, 0x00, 0xd4]);
}

#[test]
fn mcontext_is_the_netbsd_aarch64_layout() {
    assert_eq!(size_of::<MContextLayout>(), 0x370);
    assert_eq!(align_of::<MContextLayout>(), 16);
    assert_eq!(offset_of!(MContextLayout, gregs), 0x000);
    assert_eq!(offset_of!(MContextLayout, fregs), 0x120);
    assert_eq!(offset_of!(MContextLayout, spare), 0x330);
    assert_eq!(size_of::<FpregsetLayout>(), 0x210);
    assert_eq!(offset_of!(FpregsetLayout, q), 0x000);
    assert_eq!(offset_of!(FpregsetLayout, fpcr), 0x200);
    assert_eq!(offset_of!(FpregsetLayout, fpsr), 0x204);
}

#[test]
fn ucontext_is_the_c19_order_with_the_aarch64_mcontext() {
    assert_eq!(size_of::<UContextLayout>(), 0x3c0);
    assert_eq!(align_of::<UContextLayout>(), 16);
    assert_eq!(offset_of!(UContextLayout, uc_sigmask), 0x000);
    assert_eq!(offset_of!(UContextLayout, uc_mcontext), 0x010);
    assert_eq!(offset_of!(UContextLayout, uc_link), 0x380);
    assert_eq!(offset_of!(UContextLayout, uc_stack), 0x388);
    assert_eq!(offset_of!(UContextLayout, uc_flags), 0x3a0);
    assert_eq!(offset_of!(UContextLayout, uc_spare), 0x3a4);
    assert_eq!(size_of::<StackLayout>(), 24);
}

#[test]
fn sendsig_frame_is_aligned_and_packs_siginfo_below_ucontext() {
    let frame = frame(0x1000).expect("frame");
    assert_eq!(frame.ucontext, 0x1000 - 0x3c0);
    assert_eq!(frame.siginfo, 0xbf0);
    assert_eq!(frame.sp, frame.siginfo);
    assert_eq!(frame.siginfo + SIGINFO_SIZE as u64, frame.ucontext);
    assert_eq!(frame.ucontext & (FRAME_ALIGN - 1), 0);
}

#[test]
fn sendsig_frame_rejects_underflow() {
    assert_eq!(frame(0x100), None);
    assert_eq!(frame(0x3bf), None);
    assert_eq!(frame(0x400), None);
}

#[test]
fn greg_indices_match_the_netbsd_donor() {
    assert_eq!(REG_X0, 0);
    assert_eq!(REG_X28, 28);
    assert_eq!(REG_X29, 29);
    assert_eq!(REG_X30, 30);
    assert_eq!(REG_SP, 31);
    assert_eq!(REG_ELR, 32);
    assert_eq!(REG_SPSR, 33);
    assert_eq!(REG_TPIDR, 34);
    assert_eq!(NGREG, 35);
    assert_eq!(NFREG, 32);
    assert_eq!(ARG_SIG, 0);
    assert_eq!(ARG_SIGINFO, 1);
    assert_eq!(ARG_UCONTEXT, 2);
    assert_eq!(ARG_UCONTEXT_CALLEE, 28);
    assert_eq!(REG_LR, 30);
}
