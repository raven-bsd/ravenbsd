// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD kernel binary entry point.

#![no_std]
#![no_main]

mod arch;
mod panic;

// Force the `raven-mem` object code (including the exported C ABI symbols) into the link.
use raven_mem as _;

/// Kernel entry point, called by `_start` from `arch::x86_64::start`.
///
/// # Safety
///
/// `boot` must be a valid, aligned pointer to a `raven_bootinfo::BootInfo` whose memory map and
/// physical ranges the loader has already made readable in the current address space.
#[unsafe(no_mangle)]
pub extern "C" fn kmain(boot: *const raven_bootinfo::BootInfo) -> ! {
    let _ = boot;
    arch::halt()
}
