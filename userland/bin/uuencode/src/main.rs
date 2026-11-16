// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `uuencode` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::String;

    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;
    use uuencode::{encode, parse};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("uuencode", "[input] output_name"));
                return EXIT_USAGE;
            }
        };
        let Some(input) = options.input else {
            raven_rt::eprintln!("uuencode: standard input is not available in this image");
            return EXIT_USAGE;
        };
        if input == "-" {
            raven_rt::eprintln!("uuencode: standard input is not available in this image");
            return EXIT_USAGE;
        }
        let path = with_file_scheme(&fs::resolve(input));
        let data = match fs::read_to_vec(&path) {
            Ok(data) => data,
            Err(error) => {
                raven_rt::eprintln!("{}", line("uuencode", input, error));
                return EXIT_ERROR;
            }
        };
        let mode = match fs::stat(&path) {
            Ok(stat) => stat.mode & 0o7777,
            Err(error) => {
                raven_rt::eprintln!("{}", line("uuencode", input, error));
                return EXIT_ERROR;
            }
        };
        let encoded = encode(&data, options.name, mode);
        let text = String::from_utf8_lossy(&encoded);
        raven_rt::print!("{text}");
        EXIT_OK
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
