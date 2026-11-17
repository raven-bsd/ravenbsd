// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `tabs` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use tabs::{parse_args, render};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("tabs", "[-c] [-T type] [stop ...]"));
                return EXIT_USAGE;
            }
        };
        match raven_rt::debug_write(render(&options).as_bytes()) {
            Ok(_) => EXIT_OK,
            Err(_) => EXIT_ERROR,
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
