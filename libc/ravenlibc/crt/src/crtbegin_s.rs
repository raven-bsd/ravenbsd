// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `crtbeginS.o`: the fallback of the compiler's `clang_rt.crtbegin` for a shared object.
//!
//! CLEANROOM (`donor-map/14` 3.A.8, `donor-map/13` 3.8). The handle is its own address and
//! lives in `.data.rel.ro`, so the linker turns the word into a `RELATIVE` relocation. A
//! `.fini_array` entry calls `__cxa_finalize(&__dso_handle)`: that is how each DSO runs its
//! destructors when it is unloaded, the loader itself never calls `__cxa_finalize`.
//!
//! The three architecture arms differ only in how `&__dso_handle` is materialised and how the
//! tail call to `__cxa_finalize` is written.

// The handle, its `.data.rel.ro` word, the `.fini_array` entry and the marker.
#[cfg(target_os = "none")]
core::arch::global_asm!(
    ".hidden __dso_handle",
    ".globl __dso_handle",
    ".type __dso_handle,%object",
    ".section .data.rel.ro.__dso_handle,\"aw\",%progbits",
    ".balign 8",
    "__dso_handle:",
    ".quad __dso_handle",
    ".size __dso_handle,8",
    ".section .fini_array,\"aw\",%fini_array",
    ".balign 8",
    ".quad __do_fini",
    ".globl __raven_crt_kind_crtbegin_s",
    ".set __raven_crt_kind_crtbegin_s, 0",
    ".section .note.GNU-stack,\"\",%progbits",
);

#[cfg(all(target_os = "none", target_arch = "x86_64"))]
core::arch::global_asm!(
    ".text",
    ".globl __do_fini",
    ".type __do_fini,@function",
    "__do_fini:",
    "lea __dso_handle(%rip),%rdi",
    "jmp __cxa_finalize",
    ".size __do_fini,.-__do_fini",
    options(att_syntax),
);

#[cfg(all(target_os = "none", target_arch = "aarch64"))]
core::arch::global_asm!(
    ".text",
    ".globl __do_fini",
    ".type __do_fini,%function",
    "__do_fini:",
    "adrp x0, __dso_handle",
    "add x0, x0, :lo12:__dso_handle",
    "b __cxa_finalize",
    ".size __do_fini,.-__do_fini",
);

#[cfg(all(target_os = "none", target_arch = "riscv64"))]
core::arch::global_asm!(
    ".text",
    ".globl __do_fini",
    ".type __do_fini,%function",
    "__do_fini:",
    "lla a0, __dso_handle",
    "tail __cxa_finalize",
    ".size __do_fini,.-__do_fini",
);
