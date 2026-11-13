// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the register-convention table and the copied stubs.

#![allow(clippy::indexing_slicing)]

use super::*;

fn source(name: &str) -> host::string::String {
    let path = host::format!("{}/src/arch/{name}.rs", env!("CARGO_MANIFEST_DIR"));
    host::fs::read_to_string(path).unwrap()
}

#[test]
fn table_matches_the_kernel_contract() {
    let x = convention::X86_64;
    assert_eq!(x.instruction, "syscall");
    assert_eq!(x.nr_reg, "rax");
    assert_eq!(x.arg_regs, ["rdi", "rsi", "rdx", "r10", "r8", "r9"]);
    assert_eq!(x.ret_reg, "rax");

    let a = convention::AARCH64;
    assert_eq!(a.instruction, "svc #0");
    assert_eq!(a.nr_reg, "x8");
    assert_eq!(a.arg_regs, ["x0", "x1", "x2", "x3", "x4", "x5"]);
    assert_eq!(a.ret_reg, "x0");

    let r = convention::RISCV64;
    assert_eq!(r.instruction, "ecall");
    assert_eq!(r.nr_reg, "a7");
    assert_eq!(r.arg_regs, ["a0", "a1", "a2", "a3", "a4", "a5"]);
    assert_eq!(r.ret_reg, "a0");
}

#[test]
fn table_lists_every_supported_architecture() {
    assert_eq!(convention::ALL.len(), 3);
    assert_eq!(
        convention::ALL.map(|c| c.arch),
        ["x86_64", "aarch64", "riscv64"]
    );
}

#[test]
fn x86_64_stub_uses_the_contract_registers() {
    let text = source("x86_64");
    assert!(text.contains("inlateout(\"rax\")"));
    for register in ["rdi", "rsi", "rdx", "r10", "r8", "r9"] {
        assert!(text.contains(&host::format!("in(\"{register}\")")));
    }
    assert_eq!(text.matches("clobber_abi(\"C\")").count(), 7);
}

#[test]
fn aarch64_stub_uses_the_contract_registers() {
    let text = source("aarch64");
    assert!(text.contains("in(\"x8\")"));
    for register in ["x0", "x1", "x2", "x3", "x4", "x5"] {
        assert!(text.contains(&host::format!("in(\"{register}\")")));
    }
    assert!(text.contains("lateout(\"x0\")"));
}

#[test]
fn riscv64_stub_uses_the_contract_registers() {
    let text = source("riscv64");
    assert!(text.contains("in(\"a7\")"));
    for register in ["a0", "a1", "a2", "a3", "a4", "a5"] {
        assert!(text.contains(&host::format!("in(\"{register}\")")));
    }
    assert!(text.contains("lateout(\"a0\")"));
}

#[test]
fn stubs_never_narrow_the_memory_clobber() {
    for arch in ["x86_64", "aarch64", "riscv64"] {
        let text = source(arch);
        for forbidden in ["nomem", "readonly", "pure"] {
            assert!(
                !text.contains(forbidden),
                "{arch} must not use options({forbidden})"
            );
        }
        assert!(text.contains("options(nostack)"));
    }
}

#[test]
fn stubs_carry_the_first_party_provenance_line() {
    for arch in ["x86_64", "aarch64", "riscv64"] {
        assert!(source(arch).contains(
            "// RavenBSD: copied from userland/rt/src/sys.rs (same project, BSD-3-Clause)"
        ));
    }
}

#[test]
fn kernel_and_abi_still_document_the_same_convention() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../..");
    let abi =
        host::fs::read_to_string(host::format!("{root}/crates/raven-abi/src/syscall.rs")).unwrap();
    for text in [
        "number `rax`, args `rdi rsi rdx r10 r8 r9`, result `rax`",
        "number `x8`, args `x0..x5`, result `x0`",
        "number `a7`, args `a0..a5`, result `a0`",
    ] {
        assert!(
            abi.contains(text),
            "raven-abi convention line changed: {text}"
        );
    }

    let aarch64 =
        host::fs::read_to_string(host::format!("{root}/kernel/src/arch/aarch64/syscall.rs"))
            .unwrap();
    assert!(aarch64.contains("svc"));

    let dispatch =
        host::fs::read_to_string(host::format!("{root}/kernel/src/syscall/mod.rs")).unwrap();
    assert!(dispatch.contains("`x8` number"));
    assert!(dispatch.contains("`x0..x5` arguments"));
}

#[test]
fn current_architecture_has_a_stub_module() {
    #[cfg(target_arch = "x86_64")]
    assert_eq!(crate::arch::convention::X86_64.arch, "x86_64");
    #[cfg(target_arch = "aarch64")]
    assert_eq!(crate::arch::convention::AARCH64.arch, "aarch64");
    #[cfg(target_arch = "riscv64")]
    assert_eq!(crate::arch::convention::RISCV64.arch, "riscv64");
}
