// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `/sbin/shutdown`: stop services and request reboot, halt or poweroff.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::vec::Vec;
    use raven_rt::entry;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let argv: Vec<&str> = args.iter().collect();
        raven_shutdown::run_shutdown(&argv)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
