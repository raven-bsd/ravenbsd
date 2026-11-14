// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `crtend.o`: the fallback of the compiler's `clang_rt.crtend`.
//!
//! CLEANROOM. The one thing a terminator object must contribute is the zero-length `.eh_frame`
//! record that ends the unwinder's table walk (`donor-map/14` 3.A.8). The object is otherwise
//! empty.

// The `.eh_frame` terminator and the marker for the extractor.
#[cfg(target_os = "none")]
core::arch::global_asm!(
    ".section .eh_frame,\"a\",%progbits",
    ".balign 8",
    ".long 0",
    ".globl __raven_crt_kind_crtend",
    ".set __raven_crt_kind_crtend, 0",
    ".section .note.GNU-stack,\"\",%progbits",
);
