// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The kernel's PG-7 signal entry body.
//!
//! The kernel saves the interrupted machine context at `tp + SIG_CTX_AREA_OFFSET`, sets
//! `rdi = kick bits`, `rsi = optional FaultRecord`, `rdx = user mask`, and resumes at the
//! registered entry. This function is that entry: it runs the libc handler machinery and then
//! `SIGRETURN`s to the saved context.

use raven_libc_signal::kernel::{FaultRecord, SigKernel, SysKernel};
use raven_libc_signal::raw::SIG_CTX_AREA_OFFSET;

use super::DISP;

/// The kernel enters with `sp - 128`; the Rust entry body has a large `ucontext_t` frame and
/// must not reuse the interrupted stack that the handler will occupy. A dedicated 16 KiB stack
/// keeps the entry frame and the application handler frame disjoint.
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .text
    .global __raven_sigentry_body_asm
    .hidden __raven_sigentry_body_asm
    .type __raven_sigentry_body_asm,@function
__raven_sigentry_body_asm:
    lea __raven_sigentry_stack(%rip),%rsp
    add $16384,%rsp
    and $-16,%rsp
    jmp __raven_sigentry_body

    .bss
    .global __raven_sigentry_stack
    .hidden __raven_sigentry_stack
    .align 16
__raven_sigentry_stack:
    .zero 16384
"#,
    options(att_syntax)
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .text
    .global __raven_sigentry_body_asm
    .hidden __raven_sigentry_body_asm
    .type __raven_sigentry_body_asm,%function
__raven_sigentry_body_asm:
    adrp x9, __raven_sigentry_stack
    add x9, x9, :lo12:__raven_sigentry_stack
    add x9, x9, #16384
    and x9, x9, #0xfffffffffffffff0
    mov sp, x9
    b __raven_sigentry_body

    .bss
    .global __raven_sigentry_stack
    .hidden __raven_sigentry_stack
    .align 4
__raven_sigentry_stack:
    .zero 16384
"#
);

unsafe extern "C" {
    fn __raven_sigentry_body_asm();
}

/// The registered entry body. Never returns: `signal_entry` ends in `SIGRETURN`.
#[unsafe(no_mangle)]
pub extern "C" fn __raven_sigentry_body(bits: u64, fault: u64, _mask: u64) -> ! {
    let record = if fault == 0 {
        None
    } else {
        // SAFETY: the kernel wrote a `FaultRecord` at this user address immediately after the
        // saved `MContext`; the address is non-zero and valid for the duration of the entry.
        Some(unsafe { core::ptr::read(fault as *const FaultRecord) })
    };
    raven_libc_signal::entry::signal_entry(&SysKernel::new(), &DISP, bits as u32, record)
}

/// Register the entry once, before the first application handler is installed.
pub(crate) fn ensure_signal_entry() {
    let kernel = SysKernel::new();
    if kernel.caps().entry {
        let entry = __raven_sigentry_body_asm as *const () as usize;
        let _ = kernel.sig_setup(entry, SIG_CTX_AREA_OFFSET, 0);
    }
}
