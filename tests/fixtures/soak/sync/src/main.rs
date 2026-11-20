// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K228 soak sync fixture: flush filesystem state through the product `file:` Sync call.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        match raven_rt::fs::sync() {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("soak-sync: {}", error.description());
                1
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
