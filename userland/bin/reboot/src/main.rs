// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `/sbin/reboot`: stop services and request a normal reboot.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_shutdown::run_reboot()
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
