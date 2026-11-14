// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The target panic handler: one line on the debug console, then `EXIT(127)`.

use crate::raw::sys;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    sys::debug_write(b"ld-elf.so.1: panic\n");
    sys::exit(127)
}
