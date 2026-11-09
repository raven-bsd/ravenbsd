// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD kernel binary entry point.

#![no_std]
#![no_main]

extern crate alloc;

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

    let data = match boot::validate(boot) {
        Ok(data) => data,
        Err(error) => {
            kprintln!("kernel: error: {error}");
            boot::exit_qemu(0x11);
        }
    };
    {
        let initial_cmdline = cmdline::CmdLine::parse(data.cmdline);
        log::init(initial_cmdline.loglevel());
        log::set_test_exit(initial_cmdline.test_exit());
    }

    // S05 installs the CPU tables before any exception can be reported.
    arch::gdt::init();
    arch::idt::init();

    if let Err(error) = memory::frame::init(data.info.hhdm_offset, data.memory_map) {
        kprintln!("kernel: error: {error}");
        boot::exit_qemu(0x11);
    }
    let boot_info = match memory::paging::init(&data) {
        Ok(boot_info) => boot_info,
        Err(error) => {
            kprintln!("kernel: error: {error}");
            boot::exit_qemu(0x11);
        }
    };
    if let Err(error) = memory::heap::init() {
        kprintln!("kernel: error: {error}");
        boot::exit_qemu(0x11);
    }

    // `data` still points at loader memory that paging just reclaimed. Re-parse the command line
    // from the kernel-owned copy before running the self-tests.
    let cmdline = cmdline::CmdLine::parse(memory::paging::cmdline());
    selftest::run(&cmdline);

    // `boot::run` re-validates the copied handoff, finds the frame allocator already initialised,
    // prints the S04 banner and exits through the test-exit port.
    boot::run(boot_info)
}
