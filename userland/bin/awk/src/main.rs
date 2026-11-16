// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `awk` utility (bounded POSIX core).

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_awk::{Options, run};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let mut options = Options::default();
        let mut scripts = Vec::new();
        let mut operands = Vec::new();
        let mut iter = args.iter().skip(1);
        while let Some(argument) = iter.next() {
            if argument == "-F" {
                let Some(value) = iter.next() else {
                    return usage_error();
                };
                options.fs = Some(value.to_string());
            } else if let Some(value) = argument.strip_prefix("-F") {
                if !value.is_empty() {
                    options.fs = Some(value.to_string());
                }
            } else if argument == "-v" {
                let Some(value) = iter.next() else {
                    return usage_error();
                };
                if !assign_option(&mut options, value) {
                    return usage_error();
                }
            } else if argument.starts_with("-v") && argument.len() > 2 {
                if !assign_option(&mut options, &argument[2..]) {
                    return usage_error();
                }
            } else if argument == "-f" {
                let Some(path) = iter.next() else {
                    return usage_error();
                };
                match read(path) {
                    Ok(text) => scripts.push(String::from_utf8_lossy(&text).into_owned()),
                    Err(error) => {
                        raven_rt::eprintln!("{}", line("awk", path, error));
                        return EXIT_ERROR;
                    }
                }
            } else if argument.starts_with('-') && argument.len() > 1 {
                return usage_error();
            } else {
                operands.push(argument);
            }
        }
        if scripts.is_empty() {
            let Some(script) = operands.first().copied() else {
                return usage_error();
            };
            scripts.push(script.to_string());
            operands.remove(0);
        }
        if operands.is_empty() {
            raven_rt::eprintln!("awk: standard input is not available in this image");
            return EXIT_USAGE;
        }
        let script = scripts.join("\n");
        let mut input = String::new();
        for path in operands {
            match read(path) {
                Ok(data) => input.push_str(&String::from_utf8_lossy(&data)),
                Err(error) => {
                    raven_rt::eprintln!("{}", line("awk", path, error));
                    return EXIT_ERROR;
                }
            }
            if !input.ends_with('\n') {
                input.push('\n');
            }
        }
        match run(&script, &input, &options) {
            Ok(output) => {
                raven_rt::print!("{output}");
                EXIT_OK
            }
            Err(_) => {
                raven_rt::eprintln!("awk: invalid or unsupported script");
                EXIT_USAGE
            }
        }
    }

    fn assign_option(options: &mut Options, value: &str) -> bool {
        let Some((name, value)) = value.split_once('=') else {
            return false;
        };
        options.vars.push((name.to_string(), value.to_string()));
        true
    }

    fn read(path: &str) -> Result<Vec<u8>, raven_tool_core::Errno> {
        fs::read_to_vec(&with_file_scheme(&fs::resolve(path)))
    }

    fn usage_error() -> i32 {
        raven_rt::eprintln!(
            "{}",
            usage("awk", "[-F fs] [-v name=value] [-f script] script file ...")
        );
        EXIT_USAGE
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
