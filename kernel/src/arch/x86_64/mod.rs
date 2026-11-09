// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 architecture support.

pub mod serial;
mod start;

/// Halts the current CPU forever.
///
/// Interrupts are disabled first so that a stray interrupt cannot leave the CPU spinning in the
/// halt loop; `hlt` still wakes on NMI and machine checks, after which the loop halts again.
pub fn halt() -> ! {
    loop {
        // SAFETY: `cli` and `hlt` are privileged instructions, valid in ring 0, and touch no
        // memory. They do not change the stack and `cli`/`hlt` leave the flags alone.
        unsafe {
            core::arch::asm!("cli", "hlt", options(nomem, nostack, preserves_flags),);
        }
    }
}
