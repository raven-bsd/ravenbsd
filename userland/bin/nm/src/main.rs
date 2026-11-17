// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `nm` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use nm::{NmError, Options, inspect, parse_args, render};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("nm", "[-agunpr] file ..."));
                return EXIT_USAGE;
            }
        };
        run(&options)
    }

    fn run(options: &Options<'_>) -> i32 {
        let multiple = options.operands.len() > 1;
        let mut first = true;
        let mut status = EXIT_OK;
        for path in &options.operands {
            let resolved = fs::resolve(path);
            let data = match fs::read_to_vec(&with_file_scheme(&resolved)) {
                Ok(data) => data,
                Err(error) => {
                    raven_rt::eprintln!("nm: {path}: {}", error.description());
                    status = EXIT_ERROR;
                    continue;
                }
            };
            match inspect(&data, options) {
                Ok((class, entries)) => {
                    if multiple {
                        if !first {
                            raven_rt::println!();
                        }
                        raven_rt::println!("{path}:");
                    }
                    first = false;
                    if entries.is_empty() {
                        raven_rt::eprintln!("nm: {path}: no symbols");
                    }
                    for line in render(&entries, class) {
                        raven_rt::println!("{line}");
                    }
                }
                Err(error) => {
                    raven_rt::eprintln!("nm: {path}: {}", message(error));
                    status = EXIT_ERROR;
                }
            }
        }
        status
    }

    fn message(error: NmError) -> &'static str {
        match error {
            NmError::Malformed => "not a supported ELF32/ELF64 object",
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(host_run(&args));
}

#[cfg(not(target_os = "none"))]
fn host_run(args: &[String]) -> i32 {
    use nm::{parse_args, render};

    let options = match parse_args(args.iter().map(String::as_str)) {
        Ok(options) => options,
        Err(_) => {
            eprintln!("usage: nm [-agunpr] file ...");
            return 2;
        }
    };
    let multiple = options.operands.len() > 1;
    let mut first = true;
    let mut status = 0;
    for path in &options.operands {
        let data = match std::fs::read(path) {
            Ok(data) => data,
            Err(error) => {
                eprintln!("nm: {path}: {error}");
                status = 1;
                continue;
            }
        };
        match nm::inspect(&data, &options) {
            Ok((class, entries)) => {
                if multiple {
                    if !first {
                        println!();
                    }
                    println!("{path}:");
                }
                first = false;
                if entries.is_empty() {
                    eprintln!("nm: {path}: no symbols");
                }
                for line in render(&entries, class) {
                    println!("{line}");
                }
            }
            Err(error) => {
                eprintln!("nm: {path}: {}", host_message(error));
                status = 1;
            }
        }
    }
    status
}

#[cfg(not(target_os = "none"))]
fn host_message(error: nm::NmError) -> &'static str {
    match error {
        nm::NmError::Malformed => "not a supported ELF32/ELF64 object",
    }
}
