// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `crti.o`: the empty first start object.
//!
//! The modern toolchain emits no legacy `.init`/`.fini` prologue (musl's `crt/crti.c` is a
//! zero-line file, `A-abi-21`; `donor-map/10` 3.12.1 decides the same). The object exists so
//! that unmodified link lines that name `crti.o` keep working, and it carries only the
//! non-executable stack note and the archive marker that `libc-pack` uses to find this member.

// The marker `libc_pack_crt` looks for; stripped from the produced object.
#[cfg(target_os = "none")]
core::arch::global_asm!(
    ".globl __raven_crt_kind_crti",
    ".set __raven_crt_kind_crti, 0",
    ".section .note.GNU-stack,\"\",%progbits",
);
