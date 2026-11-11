// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 early-boot `kmain`: validate the handoff, build the kernel's own page tables and heap,
//! bring up the GICv2 and the generic timer, run the requested self-test and exit with the test
//! status.
//!
//! The device tree is parsed before paging reclaims loader memory; the resulting platform
//! addresses are plain values, so the later GIC/timer bring-up does not touch loader frames.
//! There is no scheduler or user mode until a self-test asks for it (S14b/S14c).

use raven_bootinfo::BootInfo;

use crate::arch::aarch64::{fdt, gic, irq, timer};
use crate::cmdline::CmdLine;
use crate::{boot, kprintln, log, memory, sync, time};

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
    // The DTB lives in loader-owned memory that paging reclaims, so read it now.
    let platform = fdt::platform(data.info.dtb.base, data.info.dtb.len);
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

    // GICv2 and the generic timer. The frames are mapped through the uncached MMIO window.
    if let Err(error) = gic::init(platform.gicd, platform.gicc) {
        kprintln!("kernel: error: {error}");
        boot::exit_qemu(0x11);
    }
    let frequency = match timer::init(platform.timer_ppi) {
        Ok(frequency) => frequency,
        Err(error) => {
            kprintln!("kernel: error: {error}");
            boot::exit_qemu(0x11);
        }
    };
    irq::set_timer(platform.timer_ppi);
    time::init(frequency, timer::read_counter());
    if let Err(error) = gic::enable(platform.timer_ppi) {
        kprintln!("kernel: error: {error}");
        boot::exit_qemu(0x11);
    }
    kprintln!("time: generic timer {} MHz", frequency / 1_000_000);
    kprintln!("timer: {} Hz", timer::HZ);

    // SAFETY: the vector table was installed in `_start`, and the GIC and timer are configured.
    unsafe {
        sync::enable_interrupts();
    }

    // `data` still points at loader memory that paging just reclaimed. Re-parse the command
    // line from the kernel-owned copy.
    let cmdline = CmdLine::parse(memory::paging::cmdline());
    crate::arch::aarch64::selftest::run(&cmdline);

    // `boot::run` re-validates the copied handoff, finds the frame allocator already
    // initialised, prints the C4 banner and exits through the test-exit protocol.
    boot::run(boot_info)
}
