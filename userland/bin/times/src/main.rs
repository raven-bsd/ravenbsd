// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `times` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_tool_core::error::{EXIT_OK, EXIT_USAGE, usage};
    use times::{parse_args, render};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        if parse_args(args.iter().skip(1)).is_err() {
            raven_rt::eprintln!("{}", usage("times", ""));
            return EXIT_USAGE;
        }
        // The kernel has no process CPU-time clock; report the POSIX shape with zero ticks.
        raven_rt::print!("{}", render(0, 0, 0, 0, 100));
        EXIT_OK
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
