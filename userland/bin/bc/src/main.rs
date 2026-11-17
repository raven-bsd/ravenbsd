// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `bc` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_bc::{Options, Session};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let arguments: Vec<String> = args.iter().skip(1).map(|value| value.to_string()).collect();
        let (options, files) = match parse(&arguments) {
            Ok(parsed) => parsed,
            Err(()) => {
                raven_rt::eprintln!("{}", usage("bc", "[-l] [file ...]"));
                return EXIT_USAGE;
            }
        };
        let mut session = Session::new(&options);
        if files.is_empty() {
            let text = match read_console() {
                Ok(text) => text,
                Err(error) => {
                    raven_rt::eprintln!("bc: tty:console: {}", error.description());
                    return EXIT_ERROR;
                }
            };
            if let Err(error) = session.process(&text) {
                raven_rt::print!("{}", session.finish());
                raven_rt::eprintln!("bc: {error}");
                return EXIT_ERROR;
            }
        } else {
            for file in files {
                let text = if file == "-" {
                    match read_console() {
                        Ok(text) => text,
                        Err(error) => {
                            raven_rt::eprintln!("bc: tty:console: {}", error.description());
                            return EXIT_ERROR;
                        }
                    }
                } else {
                    match read(&file) {
                        Ok(text) => text,
                        Err(error) => {
                            raven_rt::print!("{}", session.finish());
                            raven_rt::eprintln!("{}", line("bc", &file, error));
                            return EXIT_ERROR;
                        }
                    }
                };
                if let Err(error) = session.process(&text) {
                    raven_rt::print!("{}", session.finish());
                    raven_rt::eprintln!("bc: {error}");
                    return EXIT_ERROR;
                }
                if session.quit() {
                    break;
                }
            }
        }
        raven_rt::print!("{}", session.finish());
        EXIT_OK
    }

    fn parse(arguments: &[String]) -> Result<(Options, Vec<String>), ()> {
        let mut options = Options::default();
        let mut files = Vec::new();
        let mut end_of_options = false;
        for argument in arguments {
            if end_of_options {
                files.push(argument.clone());
                continue;
            }
            match argument.as_str() {
                "--" => end_of_options = true,
                "-l" => options.math_lib = true,
                "-" => files.push(argument.clone()),
                value if value.starts_with('-') && value.len() > 1 => return Err(()),
                value => files.push(value.to_string()),
            }
        }
        Ok((options, files))
    }

    fn read(path: &str) -> Result<String, raven_tool_core::Errno> {
        let data = fs::read_to_vec(&with_file_scheme(&fs::resolve(path)))?;
        Ok(String::from_utf8_lossy(&data).into_owned())
    }

    fn read_console() -> Result<String, raven_tool_core::Errno> {
        let mut file = fs::open("tty:console", 0)?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            data.extend_from_slice(buffer.get(..count).unwrap_or(&[]));
        }
        Ok(String::from_utf8_lossy(&data).into_owned())
    }
}

#[cfg(not(target_os = "none"))]
use std::io::Read as _;

#[cfg(not(target_os = "none"))]
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(host_run(&arguments));
}

#[cfg(not(target_os = "none"))]
fn host_run(arguments: &[String]) -> i32 {
    use raven_bc::{Options, Session};

    let mut options = Options::default();
    let mut files = Vec::new();
    let mut end_of_options = false;
    for argument in arguments {
        if end_of_options {
            files.push(argument.clone());
            continue;
        }
        match argument.as_str() {
            "--" => end_of_options = true,
            "-l" => options.math_lib = true,
            "-" => files.push(argument.clone()),
            value if value.starts_with('-') && value.len() > 1 => {
                eprintln!("usage: bc [-l] [file ...]");
                return 2;
            }
            value => files.push(value.to_string()),
        }
    }
    let mut session = Session::new(&options);
    if files.is_empty() {
        match read_stdin() {
            Ok(text) => {
                if let Err(error) = session.process(&text) {
                    print!("{}", session.finish());
                    eprintln!("bc: {error}");
                    return 1;
                }
            }
            Err(error) => {
                eprintln!("bc: standard input: {error}");
                return 1;
            }
        }
    } else {
        for file in files {
            let text = if file == "-" {
                match read_stdin() {
                    Ok(text) => text,
                    Err(error) => {
                        eprintln!("bc: standard input: {error}");
                        return 1;
                    }
                }
            } else {
                match std::fs::read_to_string(&file) {
                    Ok(text) => text,
                    Err(error) => {
                        print!("{}", session.finish());
                        eprintln!("bc: {file}: {error}");
                        return 1;
                    }
                }
            };
            if let Err(error) = session.process(&text) {
                print!("{}", session.finish());
                eprintln!("bc: {error}");
                return 1;
            }
            if session.quit() {
                break;
            }
        }
    }
    print!("{}", session.finish());
    0
}

#[cfg(not(target_os = "none"))]
fn read_stdin() -> std::io::Result<String> {
    let mut data = Vec::new();
    std::io::stdin().read_to_end(&mut data)?;
    Ok(String::from_utf8_lossy(&data).into_owned())
}
