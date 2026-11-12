// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `rmdir` utility.

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
        let parsed = match raven_rmdir::parse(args.iter().skip(1)) {
            Ok(parsed) => parsed,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("rmdir", "directory ..."));
                return EXIT_USAGE;
            }
        };
        if parsed.operands.is_empty() {
            raven_rt::eprintln!("{}", usage("rmdir", "directory ..."));
            return EXIT_USAGE;
        }
        let mut status = EXIT_OK;
        for path in parsed.operands {
            if let Err(error) = fs::rmdir(&fs::resolve(path)) {
                raven_rt::eprintln!("{}", line("rmdir", path, error));
                status = EXIT_ERROR;
            }
        }
        status
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
