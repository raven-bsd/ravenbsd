// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 self-tests selected by `selftest=` on the kernel command line.

use crate::cmdline::CmdLine;
use crate::kprintln;

/// Run the self-test named by `selftest=`, if any.
pub fn run(cmdline: &CmdLine) {
    match cmdline.get("selftest") {
        Some("brk") => breakpoint(),
        // Behaves as on x86-64: allocate, free and grow, then report the heap statistics.
        Some("heap") => crate::memory::heap::selftest(),
        _ => {}
    }
}

#[inline(never)]
fn breakpoint() {
    // SAFETY: BRK #0x13 raises a synchronous BRK exception (EC 0x3c). The vector handler prints
    // the trap frame and exits the test with failure; this instruction does not return in that
    // path. If a future handler returns, execution continues on the next line.
    unsafe {
        core::arch::asm!("brk #0x13");
    }
    kprintln!("selftest: brk returned");
}
