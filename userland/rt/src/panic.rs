// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The default `#[panic_handler]` for freestanding programs.

use core::panic::PanicInfo;

use crate::print::write_formatted;

/// Print the panic location and terminate with code 101.
#[cfg(not(test))]
#[panic_handler]
fn panic_handler(info: &PanicInfo<'_>) -> ! {
    let location = info.location();
    let (file, line, column) = match location {
        Some(location) => (location.file(), location.line(), location.column()),
        None => ("<unknown>", 0, 0),
    };
    write_formatted(
        core::format_args!(
            "panic: {} at {}:{}:{}\n",
            info.message(),
            file,
            line,
            column
        ),
        &mut |bytes| crate::sys::debug_write(bytes),
    );
    crate::sys::exit(101)
}
