// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Explicit self-tests selected by `selftest=<name>` on the kernel command line.

use core::arch::asm;

use crate::cmdline::CmdLine;
use crate::kprintln;

/// Run the self-test named by `selftest=`, if any.
pub fn run(cmdline: &CmdLine) {
    match cmdline.get("selftest") {
        Some("int3") => {
            // SAFETY: INT3 is a one-byte software breakpoint; the handler returns to the next
            // instruction.
            unsafe {
                asm!("int3");
            }
            kprintln!("selftest: int3 returned");
        }
        Some("div0") => divide_error(),
        Some("pagefault") => page_fault(),
        Some("doublefault") => double_fault(),
        Some(name @ ("heap" | "paging")) => memory(name),
        Some("timer") => timer_selftest(),
        Some("sched") => crate::sched::selftest::run(),
        _ => {}
    }
}

fn memory(name: &str) {
    match name {
        "heap" => crate::memory::heap::selftest(),
        "paging" => crate::memory::paging::selftest(),
        _ => {}
    }
}

fn timer_selftest() {
    crate::time::selftest();
}

#[inline(never)]
fn divide_error() -> ! {
    // SAFETY: dividing by zero raises #DE, which is fatal in S05. No Rust value is read or written
    // after the fault.
    unsafe {
        asm!("xor edx, edx", "xor eax, eax", "div edx", options(noreturn),);
    }
}

#[inline(never)]
fn page_fault() -> ! {
    // SAFETY: 0xDEAD_0000 is a canonical user-range address that the loader does not map; the
    // read raises #PF with that CR2. The loop is unreachable when the handler is fatal.
    unsafe {
        let _ = core::ptr::read_volatile(0xDEAD_0000 as *const u8);
    }
    loop {
        core::hint::spin_loop();
    }
}

#[inline(never)]
fn double_fault() -> ! {
    // SAFETY: installing an invalid RSP and then raising #UD forces the CPU to double-fault while
    // pushing the exception frame; the #DF handler runs on IST1.
    unsafe {
        asm!("mov rsp, 0xdead0000", "ud2", options(noreturn),);
    }
}
