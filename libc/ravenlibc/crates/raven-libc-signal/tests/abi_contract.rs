// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PG-7 ABI contract guard.
//!
//! The libc entry and the assembly return stubs must take their syscall numbers from
//! `raven-abi`, never from a local literal (HARD-RULES 7.3). This is the failing test for
//! the rev 320 defect: the sources below still carry local `0x80`/`0x81`/`0x82`/`0x85`/`0x86`
//! values before the refactor.

const RAW: &str = include_str!("../src/raw.rs");
const ENTRY_X86_64: &str = include_str!("../src/raw/entry_x86_64.rs");
const ENTRY_AARCH64: &str = include_str!("../src/raw/entry_aarch64.rs");
const ENTRY_RISCV64: &str = include_str!("../src/raw/entry_riscv64.rs");

#[test]
fn libc_uses_the_abi_signal_numbers() {
    for literal in [
        "const SIG_SETUP: usize = 0x80",
        "const THREAD_KICK: usize = 0x81",
        "const PROC_SIGRECV_EX: usize = 0x85",
        "const PROC_SIGQUEUE: usize = 0x86",
        "const SIGRECV_DELIVERABLE: u32 = 2",
    ] {
        assert!(
            !RAW.contains(literal),
            "raw.rs still defines the ABI value locally: {literal}"
        );
    }
    for abi in ["raven_abi::signal::SIG_SETUP", "raven_abi::syscall::"] {
        assert!(RAW.contains(abi), "raw.rs must use {abi}");
    }
    for symbol in [
        "THREAD_KICK",
        "PROC_SIGRECV_EX",
        "PROC_SIGQUEUE",
        "PROC_SIGRECV_DELIVERABLE",
    ] {
        assert!(
            RAW.contains(symbol),
            "raw.rs must use the ABI symbol {symbol}"
        );
    }
}

#[test]
fn asm_sigreturn_uses_the_abi_constant() {
    for (name, text) in [
        ("entry_x86_64.rs", ENTRY_X86_64),
        ("entry_aarch64.rs", ENTRY_AARCH64),
        ("entry_riscv64.rs", ENTRY_RISCV64),
    ] {
        assert!(!text.contains("0x82"), "{name} still hardcodes 0x82");
        assert!(
            text.contains("raven_abi::signal::SIGRETURN"),
            "{name} must take SIGRETURN from raven-abi"
        );
    }
}
