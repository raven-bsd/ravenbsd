// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel panic handler. S04 replaces the bare halt with logging and a backtrace.

use core::panic::PanicInfo;

use crate::arch;

/// Panic handler: stop the machine until a later ticket can report the panic.
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    arch::halt()
}
