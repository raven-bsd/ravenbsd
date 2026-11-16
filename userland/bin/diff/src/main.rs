// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `diff` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_diff::{diff, parse};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("diff", "[-u] file1 file2"));
                return EXIT_USAGE;
            }
        };
        let [left, right] = options.operands.as_slice() else {
            raven_rt::eprintln!("{}", usage("diff", "[-u] file1 file2"));
            return EXIT_USAGE;
        };
        let left_text = match read(left) {
            Ok(text) => text,
            Err(error) => {
                raven_rt::eprintln!("{}", line("diff", left, error));
                return EXIT_ERROR;
            }
        };
        let right_text = match read(right) {
            Ok(text) => text,
            Err(error) => {
                raven_rt::eprintln!("{}", line("diff", right, error));
                return EXIT_ERROR;
            }
        };
        let (changed, output) = diff(&left_text, &right_text, options.unified);
        if !output.is_empty() {
            raven_rt::print!("{output}");
        }
        if changed { 1 } else { EXIT_OK }
    }

    fn read(path: &str) -> Result<alloc::string::String, raven_tool_core::Errno> {
        let data = fs::read_to_vec(&with_file_scheme(&fs::resolve(path)))?;
        alloc::string::String::from_utf8(data).map_err(|_| raven_tool_core::Errno::EILSEQ)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
