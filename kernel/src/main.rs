// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD kernel binary entry point.

#![no_std]
#![no_main]

mod arch;
mod backtrace;
mod boot;
mod cmdline;
mod log;
mod memory;
mod selftest;

// Force the `raven-mem` object code (including the exported C ABI symbols) into the link.
use raven_mem as _;

/// Kernel entry point called by `_start` with the loader's `BootInfo` pointer in `rdi`.
///
/// # Safety
///
/// `boot` must be a valid HHDM virtual pointer to a loader-built `raven_bootinfo::BootInfo`.
#[unsafe(no_mangle)]
pub extern "C" fn kmain(boot: *const raven_bootinfo::BootInfo) -> ! {
    arch::serial::init();

    // S05 installs the CPU tables and runs the optional self-test before S04's banner path.
    // `boot::run` validates the handoff again; this early validation only supplies the command
    // line to the self-test runner.
    if let Ok(data) = boot::validate(boot) {
        let cmdline = cmdline::CmdLine::parse(data.cmdline);
        log::init(cmdline.loglevel());
        log::set_test_exit(cmdline.test_exit());
        arch::gdt::init();
        arch::idt::init();
        selftest::run(&cmdline);
    }

    boot::run(boot)
}
