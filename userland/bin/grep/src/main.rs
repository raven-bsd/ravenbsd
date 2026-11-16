// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `grep` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_grep::{compile, parse, process};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!(
                    "{}",
                    usage("grep", "[-i -v -n -l -q -c -E -F] pattern [file ...]")
                );
                return EXIT_USAGE;
            }
        };
        if options.files.is_empty() {
            raven_rt::eprintln!("grep: standard input is not available in this image");
            return EXIT_USAGE;
        }
        let regex = match compile(&options) {
            Ok(regex) => regex,
            Err(_) => {
                raven_rt::eprintln!("grep: invalid regular expression");
                return EXIT_USAGE;
            }
        };
        let show_name = options.files.len() > 1;
        let mut matched = false;
        for path in &options.files {
            let data = match read(path) {
                Ok(data) => data,
                Err(error) => {
                    raven_rt::eprintln!("{}", line("grep", path, error));
                    return EXIT_ERROR;
                }
            };
            match process(&regex, Some(path), show_name, &data, &options) {
                Ok(report) => {
                    matched |= report.matched;
                    for text in report.output {
                        raven_rt::println!("{text}");
                    }
                }
                Err(_) => {
                    raven_rt::eprintln!("grep: invalid regular expression");
                    return EXIT_USAGE;
                }
            }
        }
        if matched { EXIT_OK } else { 1 }
    }

    fn read(path: &str) -> Result<alloc::vec::Vec<u8>, raven_tool_core::Errno> {
        fs::read_to_vec(&with_file_scheme(&fs::resolve(path)))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
