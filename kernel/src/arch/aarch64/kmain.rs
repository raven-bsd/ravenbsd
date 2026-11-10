// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 early-boot `kmain`: validate the handoff, bring up the frame allocator, build the
//! kernel's own page tables and heap, run the requested self-test and exit with the test status.
//!
//! After `memory::paging::init` the kernel runs on its own TTBR1 tables and the loader's frames
//! are reclaimed, exactly as the x86-64 `kmain` does. There is no scheduler or user mode here
//! yet (S14b/S14c).

use raven_bootinfo::BootInfo;

use crate::cmdline::CmdLine;
use crate::{boot, kprintln, log, memory};

/// Kernel entry point called by `_start` with the loader's `BootInfo` pointer in `x0`.
///
/// # Safety
///
/// `boot` must be a valid HHDM virtual pointer to a loader-built `raven_bootinfo::BootInfo`.
#[unsafe(no_mangle)]
pub extern "C" fn kmain(boot: *const BootInfo) -> ! {
    crate::arch::serial::init();

    let data = match boot::validate(boot) {
        Ok(data) => data,
        Err(error) => {
            kprintln!("kernel: error: {error}");
            boot::exit_qemu(0x11);
        }
    };
    {
        let initial_cmdline = CmdLine::parse(data.cmdline);
        log::init(initial_cmdline.loglevel());
        log::set_test_exit(initial_cmdline.test_exit());
    }

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

    // `data` still points at loader memory that paging just reclaimed. Re-parse the command
    // line from the kernel-owned copy.
    let cmdline = CmdLine::parse(memory::paging::cmdline());
    crate::arch::aarch64::selftest::run(&cmdline);

    // `boot::run` re-validates the copied handoff, finds the frame allocator already
    // initialised, prints the C4 banner and exits through the test-exit protocol.
    boot::run(boot_info)
}
