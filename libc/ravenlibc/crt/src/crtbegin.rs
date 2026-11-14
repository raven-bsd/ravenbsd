// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `crtbegin.o`: the fallback of the compiler's `clang_rt.crtbegin`.
//!
//! Used only when the compiler runtime does not ship the object (`donor-map/14` 3.A.8).
//! CLEANROOM: it defines the hidden `__dso_handle` (one per DSO, the address
//! `__cxa_atexit`/`pthread_atfork` receive) with a zero word; a shared object uses
//! [`crate::crtbegin_s`] instead, whose handle is its own address. FreeBSD's `crtbegin.c` is
//! BSD-1 and was read for behaviour only (DR-40).

// The one data word of the handle and the marker for the extractor.
#[cfg(target_os = "none")]
core::arch::global_asm!(
    ".hidden __dso_handle",
    ".globl __dso_handle",
    ".type __dso_handle,%object",
    ".section .data.__dso_handle,\"aw\",%progbits",
    ".balign 8",
    "__dso_handle:",
    ".quad 0",
    ".size __dso_handle,8",
    ".globl __raven_crt_kind_crtbegin",
    ".set __raven_crt_kind_crtbegin, 0",
    ".section .note.GNU-stack,\"\",%progbits",
);
