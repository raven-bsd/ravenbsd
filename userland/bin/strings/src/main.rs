// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `strings` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_strings::{extract, parse};
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("strings", "[-a] [-n number] file ..."));
                return EXIT_USAGE;
            }
        };
        if options.operands.is_empty() {
            raven_rt::eprintln!("{}", usage("strings", "[-a] [-n number] file ..."));
            return EXIT_USAGE;
        }
        let mut status = EXIT_OK;
        for path in options.operands {
            match read(path) {
                Ok(data) => {
                    for text in extract(&data, options.min) {
                        raven_rt::println!("{text}");
                    }
                }
                Err(error) => {
                    raven_rt::eprintln!("{}", line("strings", path, error));
                    status = EXIT_ERROR;
                }
            }
        }
        status
    }

    fn read(path: &str) -> Result<alloc::vec::Vec<u8>, raven_tool_core::Errno> {
        fs::read_to_vec(&with_file_scheme(&fs::resolve(path)))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
