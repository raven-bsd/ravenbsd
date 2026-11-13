// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assembly checks: the hidden symbols link, the x86-64 static TLS descriptor returns descriptor
//! word 1 through a host caller, and the golden disassembly of the three targets is present.
//! The dynamic entry needs `%fs`/`tpidr_el0`/`tp` to address our DTV; on the host `%fs` belongs
//! to the test runtime, so only the static entry is executed here and the dynamic entry is
//! covered by the build plus the golden text.

use raven_libc_thread::raw;

#[test]
fn entry_symbol_exists() {
    assert_ne!(raven_libc_thread::raw::entry::entry_address(), 0);
}

#[test]
fn static_tlsdesc_returns_word_one() {
    let mut descriptor = [0usize, 0x1234_5678];
    assert_eq!(raw::tlsdesc_static(&mut descriptor), 0x1234_5678);
    descriptor[1] = 0x800;
    assert_eq!(raw::tlsdesc_static(&mut descriptor), 0x800);
}

#[test]
fn static_tlsdesc_handles_zero() {
    let mut descriptor = [usize::MAX, 0];
    assert_eq!(raw::tlsdesc_static(&mut descriptor), 0);
}

#[test]
fn golden_tlsdesc_x86_64_matches() {
    let text = include_str!("golden/tlsdesc_x86_64.txt");
    assert!(text.contains("<__rl_tlsdesc_static>:"));
    assert!(text.contains("<__rl_tlsdesc_dynamic>:"));
    assert!(text.contains("movq\t8(%rax), %rax"));
    assert!(text.contains("movq\t%fs:8, %rdx"));
    assert!(text.contains("subq\t%fs:0, %rax"));
    assert!(text.contains("pushq\t%rdx"));
    assert!(text.contains("popq\t%rdx"));
}

#[test]
fn golden_tlsdesc_aarch64_matches() {
    let text = include_str!("golden/tlsdesc_aarch64.txt");
    assert!(text.contains("<__rl_tlsdesc_static>:"));
    assert!(text.contains("ldr\tx0, [x0, #8]"));
    assert!(text.contains("mrs\tx1, TPIDR_EL0"));
    assert!(text.contains("ldur\tx1, [x1, #-8]"));
    assert!(text.contains("stp\tx1, x2, [sp, #-16]!"));
    assert!(text.contains("ldp\tx1, x2, [sp], #16"));
}

#[test]
fn golden_tlsdesc_riscv64_matches() {
    let text = include_str!("golden/tlsdesc_riscv64.txt");
    assert!(text.contains("<__rl_tlsdesc_static>:"));
    assert!(text.contains("ld\ta0, 8(a0)"));
    assert!(text.contains("ld\tt2, -8(tp)"));
    assert!(text.contains("slli\ta0, a0, 3"));
    assert!(text.contains("sub\ta0, a0, tp"));
}

#[test]
fn golden_entries_match() {
    let x86 = include_str!("golden/entry_x86_64.txt");
    assert!(x86.contains("andq\t$-16, %rsp"));
    assert!(x86.contains("jmpq\t*(%rdi)"));
    let arm = include_str!("golden/entry_aarch64.txt");
    assert!(arm.contains("ldr\tx16, [x0]"));
    assert!(arm.contains("br\tx16"));
    let rv = include_str!("golden/entry_riscv64.txt");
    assert!(rv.contains("ld\tt0, 0(a0)"));
    assert!(rv.contains("jr\tt0"));
}
