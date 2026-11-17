// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `ulimit` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::vec::Vec;

    use raven_rt::entry;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use ulimit::parse_args;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let arguments: Vec<&str> = args.iter().skip(1).collect();
        if arguments.first() == Some(&"--help") {
            raven_rt::println!("{}", usage("ulimit", "[-HS] [-a] [-cdfnstv] [limit]"));
            return EXIT_OK;
        }
        if parse_args(&arguments).is_err() {
            raven_rt::eprintln!("{}", usage("ulimit", "[-HS] [-a] [-cdfnstv] [limit]"));
            return EXIT_USAGE;
        }
        // The ABI has no rlimit syscall: report the gap instead of inventing a limit.
        raven_rt::eprintln!("ulimit: resource limits are not available in this kernel");
        EXIT_ERROR
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
