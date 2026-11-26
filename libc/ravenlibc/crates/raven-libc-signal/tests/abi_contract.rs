// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PG-7 ABI contract guard.
//!
//! The libc signal seams must take their syscall numbers from `raven-abi`, never from a local
//! literal (HARD-RULES 7.3). This is the guard for the rev 320 defect: the sources below carried
//! local `0x80`/`0x81`/`0x82`/`0x85`/`0x86` values before the refactor.

const RAW: &str = include_str!("../src/raw.rs");

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
    assert!(
        RAW.contains("raven_abi::syscall::"),
        "raw.rs must use raven_abi::syscall::"
    );
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
