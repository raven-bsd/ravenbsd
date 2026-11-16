// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `sed` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_rt::entry;
    use raven_rt::fs;
    use raven_sed::run;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let mut quiet = false;
        let mut scripts: Vec<String> = Vec::new();
        let mut files: Vec<&str> = Vec::new();
        let mut args = args.iter().skip(1);
        while let Some(argument) = args.next() {
            match argument {
                "-n" => quiet = true,
                "-e" => {
                    let Some(script) = args.next() else {
                        return usage_error();
                    };
                    scripts.push(script.to_string());
                }
                "-f" => {
                    let Some(path) = args.next() else {
                        return usage_error();
                    };
                    match read(path) {
                        Ok(text) => scripts.push(String::from_utf8_lossy(&text).into_owned()),
                        Err(error) => {
                            raven_rt::eprintln!("{}", line("sed", path, error));
                            return EXIT_ERROR;
                        }
                    }
                }
                "--" => {
                    for file in args.by_ref() {
                        files.push(file);
                    }
                }
                value if value.starts_with('-') && value.len() > 1 => return usage_error(),
                value => files.push(value),
            }
        }
        if scripts.is_empty() {
            let Some(script) = files.first().copied() else {
                return usage_error();
            };
            scripts.push(script.to_string());
            files.remove(0);
        }
        if files.is_empty() {
            raven_rt::eprintln!("sed: standard input is not available in this image");
            return EXIT_USAGE;
        }
        let script = scripts.join("\n");
        let mut input = String::new();
        for path in files {
            match read(path) {
                Ok(data) => input.push_str(&String::from_utf8_lossy(&data)),
                Err(error) => {
                    raven_rt::eprintln!("{}", line("sed", path, error));
                    return EXIT_ERROR;
                }
            }
            if !input.ends_with('\n') {
                input.push('\n');
            }
        }
        match run(&script, &input, quiet) {
            Ok(output) => {
                raven_rt::print!("{output}");
                EXIT_OK
            }
            Err(_) => {
                raven_rt::eprintln!("sed: invalid script");
                EXIT_USAGE
            }
        }
    }

    fn read(path: &str) -> Result<Vec<u8>, raven_tool_core::Errno> {
        fs::read_to_vec(&with_file_scheme(&fs::resolve(path)))
    }

    fn usage_error() -> i32 {
        raven_rt::eprintln!("{}", usage("sed", "[-n] [-e script] [-f script] file ..."));
        EXIT_USAGE
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
