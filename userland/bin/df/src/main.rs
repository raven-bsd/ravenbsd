// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `df` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let parsed = match raven_df::parse(args.iter().skip(1)) {
            Ok(parsed) => parsed,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("df", ""));
                return EXIT_USAGE;
            }
        };
        if !parsed.operands.is_empty() {
            raven_rt::eprintln!("{}", usage("df", ""));
            return EXIT_USAGE;
        }
        match fs::statfs("/") {
            Ok(statfs) => {
                raven_rt::println!("{}", raven_df::format_df(&statfs));
                EXIT_OK
            }
            Err(error) => {
                raven_rt::eprintln!("{}", line("df", "/", error));
                EXIT_ERROR
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
