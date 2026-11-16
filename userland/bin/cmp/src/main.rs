// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `cmp` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_cmp::{differences, first_difference, format_difference, parse};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("cmp", "[-l|-s] file1 file2"));
                return EXIT_USAGE;
            }
        };
        let [left, right] = options.operands.as_slice() else {
            raven_rt::eprintln!("{}", usage("cmp", "[-l|-s] file1 file2"));
            return EXIT_USAGE;
        };
        let left_data = match read(left) {
            Ok(data) => data,
            Err(error) => {
                raven_rt::eprintln!("cmp: {left}: {}", error.description());
                return EXIT_ERROR;
            }
        };
        let right_data = match read(right) {
            Ok(data) => data,
            Err(error) => {
                raven_rt::eprintln!("cmp: {right}: {}", error.description());
                return EXIT_ERROR;
            }
        };
        let same = first_difference(&left_data, &right_data).is_none();
        if options.silent {
            return if same { EXIT_OK } else { 1 };
        }
        if options.list {
            for difference in differences(&left_data, &right_data) {
                raven_rt::println!("{}", format_difference(&difference));
            }
        } else if !same {
            let offset = first_difference(&left_data, &right_data).unwrap_or(0);
            raven_rt::println!("{left} {right} differ: byte {}", offset.saturating_add(1));
        }
        if same { EXIT_OK } else { 1 }
    }

    fn read(path: &str) -> Result<alloc::vec::Vec<u8>, raven_tool_core::Errno> {
        let full = with_file_scheme(&fs::resolve(path));
        fs::read_to_vec(&full)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
