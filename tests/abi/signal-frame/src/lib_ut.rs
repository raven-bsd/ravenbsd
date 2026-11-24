// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S160 host checks: every donor C row must equal the canonical Rust model.

use core::mem::{align_of, size_of};

use super::*;

const DONOR: &str = include_str!("../donor-layout.txt");

fn hex(text: &str) -> usize {
    usize::from_str_radix(text.trim_start_matches("0x"), 16).expect("hex value")
}

fn size_align(name: &str) -> Option<(usize, usize)> {
    match name {
        "fbsd_mcontext_t" => Some((size_of::<Mcontext>(), align_of::<Mcontext>())),
        "fbsd_sigset_t" => Some((size_of::<Sigset>(), align_of::<Sigset>())),
        "fbsd_stack_t" => Some((size_of::<Stack>(), align_of::<Stack>())),
        "fbsd_siginfo_t" => Some((size_of::<Siginfo>(), align_of::<Siginfo>())),
        "fbsd_ucontext_t" => Some((size_of::<Ucontext>(), align_of::<Ucontext>())),
        "fbsd_sigframe_t" => Some((size_of::<Sigframe>(), align_of::<Sigframe>())),
        "nb_fpregset_t" => Some((size_of::<Fpregset>(), align_of::<Fpregset>())),
        "nb_mcontext_t" => Some((size_of::<NbMcontext>(), align_of::<NbMcontext>())),
        _ => None,
    }
}

fn offset(key: &str) -> Option<usize> {
    match key {
        "fbsd_mcontext_t.mc_onstack" => Some(core::mem::offset_of!(Mcontext, mc_onstack)),
        "fbsd_mcontext_t.mc_rdi" => Some(core::mem::offset_of!(Mcontext, mc_rdi)),
        "fbsd_mcontext_t.mc_rsi" => Some(core::mem::offset_of!(Mcontext, mc_rsi)),
        "fbsd_mcontext_t.mc_rdx" => Some(core::mem::offset_of!(Mcontext, mc_rdx)),
        "fbsd_mcontext_t.mc_rcx" => Some(core::mem::offset_of!(Mcontext, mc_rcx)),
        "fbsd_mcontext_t.mc_r8" => Some(core::mem::offset_of!(Mcontext, mc_r8)),
        "fbsd_mcontext_t.mc_rip" => Some(core::mem::offset_of!(Mcontext, mc_rip)),
        "fbsd_mcontext_t.mc_rsp" => Some(core::mem::offset_of!(Mcontext, mc_rsp)),
        "fbsd_mcontext_t.mc_rflags" => Some(core::mem::offset_of!(Mcontext, mc_rflags)),
        "fbsd_mcontext_t.mc_cs" => Some(core::mem::offset_of!(Mcontext, mc_cs)),
        "fbsd_mcontext_t.mc_ss" => Some(core::mem::offset_of!(Mcontext, mc_ss)),
        "fbsd_mcontext_t.mc_fpstate" => Some(core::mem::offset_of!(Mcontext, mc_fpstate)),
        "fbsd_mcontext_t.mc_fsbase" => Some(core::mem::offset_of!(Mcontext, mc_fsbase)),
        "fbsd_mcontext_t.mc_tlsbase" => Some(core::mem::offset_of!(Mcontext, mc_tlsbase)),
        "fbsd_mcontext_t.mc_spare" => Some(core::mem::offset_of!(Mcontext, mc_spare)),
        "fbsd_stack_t.ss_sp" => Some(core::mem::offset_of!(Stack, ss_sp)),
        "fbsd_stack_t.ss_size" => Some(core::mem::offset_of!(Stack, ss_size)),
        "fbsd_stack_t.ss_flags" => Some(core::mem::offset_of!(Stack, ss_flags)),
        "fbsd_siginfo_t.si_signo" => Some(core::mem::offset_of!(Siginfo, si_signo)),
        "fbsd_siginfo_t.si_errno" => Some(core::mem::offset_of!(Siginfo, si_errno)),
        "fbsd_siginfo_t.si_code" => Some(core::mem::offset_of!(Siginfo, si_code)),
        "fbsd_siginfo_t.si_pid" => Some(core::mem::offset_of!(Siginfo, si_pid)),
        "fbsd_siginfo_t.si_uid" => Some(core::mem::offset_of!(Siginfo, si_uid)),
        "fbsd_siginfo_t.si_status" => Some(core::mem::offset_of!(Siginfo, si_status)),
        "fbsd_siginfo_t.si_addr" => Some(core::mem::offset_of!(Siginfo, si_addr)),
        "fbsd_siginfo_t.si_value" => Some(core::mem::offset_of!(Siginfo, si_value)),
        "fbsd_siginfo_t._reason" => Some(core::mem::offset_of!(Siginfo, _reason)),
        "fbsd_ucontext_t.uc_sigmask" => Some(core::mem::offset_of!(Ucontext, uc_sigmask)),
        "fbsd_ucontext_t.uc_mcontext" => Some(core::mem::offset_of!(Ucontext, uc_mcontext)),
        "fbsd_ucontext_t.uc_link" => Some(core::mem::offset_of!(Ucontext, uc_link)),
        "fbsd_ucontext_t.uc_stack" => Some(core::mem::offset_of!(Ucontext, uc_stack)),
        "fbsd_ucontext_t.uc_flags" => Some(core::mem::offset_of!(Ucontext, uc_flags)),
        "fbsd_ucontext_t.__spare__" => Some(core::mem::offset_of!(Ucontext, __spare__)),
        "fbsd_sigframe_t.sf_ahu" => Some(core::mem::offset_of!(Sigframe, sf_ahu)),
        "fbsd_sigframe_t.sf_uc" => Some(core::mem::offset_of!(Sigframe, sf_uc)),
        "fbsd_sigframe_t.sf_si" => Some(core::mem::offset_of!(Sigframe, sf_si)),
        "nb_mcontext_t.__gregs" => Some(core::mem::offset_of!(NbMcontext, __gregs)),
        "nb_mcontext_t._mc_tlsbase" => Some(core::mem::offset_of!(NbMcontext, _mc_tlsbase)),
        "nb_mcontext_t.__fpregs" => Some(core::mem::offset_of!(NbMcontext, __fpregs)),
        _ => None,
    }
}

fn greg(name: &str) -> Option<usize> {
    match name {
        "RDI" => Some(NB_GREG_RDI),
        "RSI" => Some(NB_GREG_RSI),
        "RDX" => Some(NB_GREG_RDX),
        "RCX" => Some(NB_GREG_RCX),
        "R8" => Some(NB_GREG_R8),
        "R9" => Some(NB_GREG_R9),
        "R10" => Some(NB_GREG_R10),
        "R11" => Some(NB_GREG_R11),
        "R12" => Some(NB_GREG_R12),
        "R13" => Some(NB_GREG_R13),
        "R14" => Some(NB_GREG_R14),
        "R15" => Some(NB_GREG_R15),
        "RBP" => Some(NB_GREG_RBP),
        "RBX" => Some(NB_GREG_RBX),
        "RAX" => Some(NB_GREG_RAX),
        "GS" => Some(NB_GREG_GS),
        "FS" => Some(NB_GREG_FS),
        "ES" => Some(NB_GREG_ES),
        "DS" => Some(NB_GREG_DS),
        "TRAPNO" => Some(NB_GREG_TRAPNO),
        "ERR" => Some(NB_GREG_ERR),
        "RIP" => Some(NB_GREG_RIP),
        "CS" => Some(NB_GREG_CS),
        "RFLAGS" => Some(NB_GREG_RFLAGS),
        "RSP" => Some(NB_GREG_RSP),
        "SS" => Some(NB_GREG_SS),
        _ => None,
    }
}

#[test]
fn donor_table_matches_rust_model() {
    let mut rows = 0usize;
    for line in DONOR.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(rest) = line.strip_prefix("sizeof(") {
            let (name, rest) = rest.split_once(")=").unwrap_or_else(|| panic!("{line}"));
            let (size, align) = rest
                .split_once(" align=")
                .unwrap_or_else(|| panic!("{line}"));
            let (want_size, want_align) =
                size_align(name).unwrap_or_else(|| panic!("unknown size {line}"));
            assert_eq!(want_size, hex(size), "{line}");
            assert_eq!(want_align, align.parse::<usize>().expect("align"), "{line}");
        } else if let Some(rest) = line.strip_prefix("nb_greg_") {
            let (name, value) = rest.split_once('=').unwrap_or_else(|| panic!("{line}"));
            assert_eq!(
                greg(name).unwrap_or_else(|| panic!("unknown greg {line}")),
                value.parse::<usize>().expect("greg index"),
                "{line}"
            );
        } else {
            let (key, value) = line.split_once('=').unwrap_or_else(|| panic!("{line}"));
            assert_eq!(
                offset(key).unwrap_or_else(|| panic!("unknown offset {line}")),
                hex(value),
                "{line}"
            );
        }
        rows += 1;
    }
    assert!(rows >= 70, "only {rows} donor rows");
}

#[test]
fn union_and_greg_shapes_are_pinned() {
    assert_eq!(size_of::<Reason>(), 0x28);
    assert_eq!(size_of::<Sigval>(), 0x8);
    assert_eq!(size_of::<Fpregset>(), 0x200);
    assert_eq!(align_of::<Fpregset>(), 8);
    assert_eq!(size_of::<[u64; 26]>(), 0xd0);
}
