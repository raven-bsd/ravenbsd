// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `crtn.o`: the empty last start object, the tail of the `crti`/`crtn` pair.
//!
//! Like [`crate::crti`], the object is empty by decision (`donor-map/10` 3.12.1, `A-abi-21`):
//! it exists for compatibility with link lines and carries only the stack note and the marker.

// The marker `libc_pack_crt` looks for; stripped from the produced object.
#[cfg(target_os = "none")]
core::arch::global_asm!(
    ".globl __raven_crt_kind_crtn",
    ".set __raven_crt_kind_crtn, 0",
    ".section .note.GNU-stack,\"\",%progbits",
);
