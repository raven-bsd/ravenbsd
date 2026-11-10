// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 early-boot `kmain`: validate the handoff, bring up the frame allocator, print the C4
//! banner and either run the requested self-test or exit the QEMU test.
//!
//! There is no heap, no scheduler and no MMU setup here yet (S14a): the kernel runs on the
//! loader's page tables and only uses statically sized state.

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

    boot::print_banner(&data);

    let cmdline = CmdLine::parse(data.cmdline);
    crate::arch::aarch64::selftest::run(&cmdline);

    if log::test_exit() {
        boot::exit_qemu(0x10);
    }
    kprintln!("boot: early init complete, halting");
    crate::arch::halt()
}
