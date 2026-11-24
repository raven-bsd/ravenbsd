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

#[test]
fn donor_table_is_byte_for_byte() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("donor-layout-aarch64.txt");
    let table = std::fs::read_to_string(path).expect("aarch64 donor table");
    let mut seen = 0usize;
    for line in table.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("sizeof(") {
            let (name, rest) = rest.split_once(")=").expect("sizeof row");
            let (size, align) = rest.split_once(" align=").expect("sizeof align");
            let size = parse_hex(size);
            let align: usize = align.parse().expect("decimal align");
            match name {
                "aarch64_sigset_t" => {
                    assert_eq!(size_of::<[u32; 4]>(), size);
                    assert_eq!(align_of::<[u32; 4]>(), align);
                }
                "aarch64_fregset_t" => {
                    assert_eq!(size_of::<FpregsetLayout>(), size);
                    assert_eq!(align_of::<FpregsetLayout>(), align);
                }
                "aarch64_mcontext_t" => {
                    assert_eq!(size_of::<MContextLayout>(), size);
                    assert_eq!(align_of::<MContextLayout>(), align);
                }
                "aarch64_stack_t" => {
                    assert_eq!(size_of::<StackLayout>(), size);
                    assert_eq!(align_of::<StackLayout>(), align);
                }
                "aarch64_siginfo_t" => {
                    assert_eq!(SIGINFO_SIZE, size);
                    assert_eq!(SIGINFO_ALIGN, align);
                }
                "aarch64_ucontext_t" => {
                    assert_eq!(size_of::<UContextLayout>(), size);
                    assert_eq!(align_of::<UContextLayout>(), align);
                }
                other => panic!("unknown sizeof row {other}"),
            }
            seen += 1;
            continue;
        }
        let (name, value) = line.split_once('=').expect("field row");
        let value = parse_hex(value);
        match name {
            "aarch64_fregset_t.__qregs" => assert_eq!(offset_of!(FpregsetLayout, q), value),
            "aarch64_fregset_t.__fpcr" => assert_eq!(offset_of!(FpregsetLayout, fpcr), value),
            "aarch64_fregset_t.__fpsr" => assert_eq!(offset_of!(FpregsetLayout, fpsr), value),
            "aarch64_mcontext_t.__gregs" => assert_eq!(offset_of!(MContextLayout, gregs), value),
            "aarch64_mcontext_t.__fregs" => assert_eq!(offset_of!(MContextLayout, fregs), value),
            "aarch64_mcontext_t.__spare" => assert_eq!(offset_of!(MContextLayout, spare), value),
            "aarch64_stack_t.ss_sp" => assert_eq!(offset_of!(StackLayout, ss_sp), value),
            "aarch64_stack_t.ss_size" => assert_eq!(offset_of!(StackLayout, ss_size), value),
            "aarch64_stack_t.ss_flags" => assert_eq!(offset_of!(StackLayout, ss_flags), value),
            "aarch64_ucontext_t.uc_sigmask" => {
                assert_eq!(offset_of!(UContextLayout, uc_sigmask), value);
            }
            "aarch64_ucontext_t.uc_mcontext" => {
                assert_eq!(offset_of!(UContextLayout, uc_mcontext), value);
            }
            "aarch64_ucontext_t.uc_link" => assert_eq!(offset_of!(UContextLayout, uc_link), value),
            "aarch64_ucontext_t.uc_stack" => {
                assert_eq!(offset_of!(UContextLayout, uc_stack), value)
            }
            "aarch64_ucontext_t.uc_flags" => {
                assert_eq!(offset_of!(UContextLayout, uc_flags), value)
            }
            "aarch64_ucontext_t.__spare__" => {
                assert_eq!(offset_of!(UContextLayout, uc_spare), value);
            }
            other => panic!("unknown field row {other}"),
        }
        seen += 1;
    }
    assert_eq!(seen, 21, "aarch64 donor table row count");
}

fn parse_hex(text: &str) -> usize {
    usize::from_str_radix(text.trim_start_matches("0x"), 16).expect("hex")
}
