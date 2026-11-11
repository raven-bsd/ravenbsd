// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One binary used as several test services.
//!
//! `/sbin/testsvc a`, `b` and `c` print `testsvc: <name> started` and sleep forever. `/sbin/testsvc
//! x` exits immediately with status 1 so that the crash-loop policy can be observed.
//!
//! The package is built by `build.sh` with `linker.ld`, which page-aligns every segment: the
//! production user linker script leaves unaligned file tails that `raven_rt::spawn` currently
//! rejects in strict mode.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let Some(name) = args.get(0) else {
            return 2;
        };
        if name == "x" {
            return 1;
        }
        raven_rt::println!("testsvc: {} started", name);
        loop {
            let _ = raven_rt::nanosleep(1_000_000_000);
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
