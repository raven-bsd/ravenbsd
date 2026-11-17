// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `unget` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use unget::{parse_args, pending_path};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("unget", "[-ns] [-r SID] file ..."));
                return EXIT_USAGE;
            }
        };
        let mut status = EXIT_OK;
        for file in &options.files {
            let pending = pending_path(file);
            if let Err(error) = raven_rt::fs::rename(&pending, file) {
                raven_rt::eprintln!("unget: {pending}: {}", error.description());
                status = EXIT_ERROR;
            }
        }
        status
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
