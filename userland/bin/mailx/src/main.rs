// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `mailx` command.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(not(target_os = "none"))]
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(mailx::run(&arguments));
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[cfg(target_os = "none")]
#[allow(dead_code)]
fn main() {}
