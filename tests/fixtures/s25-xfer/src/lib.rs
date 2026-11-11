// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Common process support for the S25 fixture binaries.

#![no_std]

/// Terminate fixture binaries deterministically when Rust reports a panic.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    raven_rt::exit(2)
}
