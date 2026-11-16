// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `m4` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_m4::{FileProvider, M4Error, Processor, parse_args};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    struct Files;

    impl FileProvider for Files {
        fn read_file(&mut self, path: &str) -> Option<String> {
            let resolved = with_file_scheme(&fs::resolve(path));
            fs::read_to_vec(&resolved)
                .ok()
                .map(|data| String::from_utf8_lossy(&data).into_owned())
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let arguments: Vec<String> = args.iter().skip(1).map(|value| value.to_string()).collect();
        let invocation = match parse_args(&arguments) {
            Ok(invocation) => invocation,
            Err(error) => return fail(error),
        };
        let mut sources = Vec::new();
        if invocation.operands.is_empty() {
            match read_console() {
                Ok(text) => sources.push(text),
                Err(error) => {
                    raven_rt::eprintln!("m4: tty:console: {}", error.description());
                    return EXIT_ERROR;
                }
            }
        } else {
            for operand in &invocation.operands {
                if operand == "-" {
                    match read_console() {
                        Ok(text) => sources.push(text),
                        Err(error) => {
                            raven_rt::eprintln!("m4: tty:console: {}", error.description());
                            return EXIT_ERROR;
                        }
                    }
                } else {
                    match Files.read_file(operand) {
                        Some(text) => sources.push(text),
                        None => {
                            raven_rt::eprintln!("m4: {operand}: cannot open");
                            return EXIT_ERROR;
                        }
                    }
                }
            }
        }
        let mut provider = Files;
        let mut processor = Processor::new(&invocation.options, &mut provider);
        for source in sources {
            if let Err(error) = processor.process(&source) {
                return fail(error);
            }
        }
        raven_rt::print!("{}", processor.finish());
        EXIT_OK
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

    fn fail(error: M4Error) -> i32 {
        match error {
            M4Error::Usage => {
                raven_rt::eprintln!(
                    "{}",
                    usage("m4", "[-D name[=value]] [-U name] [-I dir] [file ...]")
                );
                EXIT_USAGE
            }
            M4Error::Unsupported(name) => {
                raven_rt::eprintln!("m4: {name}: unsupported macro");
                EXIT_USAGE
            }
            M4Error::Include(name) => {
                raven_rt::eprintln!("m4: {name}: cannot open include file");
                EXIT_ERROR
            }
            M4Error::Recursion => {
                raven_rt::eprintln!("m4: macro recursion limit exceeded");
                EXIT_ERROR
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
use std::io::Read as _;

#[cfg(not(target_os = "none"))]
struct HostFiles;

#[cfg(not(target_os = "none"))]
impl raven_m4::FileProvider for HostFiles {
    fn read_file(&mut self, path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(host_run(&arguments));
}

#[cfg(not(target_os = "none"))]
fn host_run(arguments: &[String]) -> i32 {
    use raven_m4::{Processor, parse_args};

    let invocation = match parse_args(arguments) {
        Ok(invocation) => invocation,
        Err(error) => return host_fail(error),
    };
    let mut sources = Vec::new();
    if invocation.operands.is_empty() {
        match read_stdin() {
            Ok(text) => sources.push(text),
            Err(error) => {
                eprintln!("m4: standard input: {error}");
                return 1;
            }
        }
    } else {
        for operand in &invocation.operands {
            if operand == "-" {
                match read_stdin() {
                    Ok(text) => sources.push(text),
                    Err(error) => {
                        eprintln!("m4: standard input: {error}");
                        return 1;
                    }
                }
            } else {
                match std::fs::read_to_string(operand) {
                    Ok(text) => sources.push(text),
                    Err(error) => {
                        eprintln!("m4: {operand}: {error}");
                        return 1;
                    }
                }
            }
        }
    }
    let mut provider = HostFiles;
    let mut processor = Processor::new(&invocation.options, &mut provider);
    for source in sources {
        if let Err(error) = processor.process(&source) {
            return host_fail(error);
        }
    }
    print!("{}", processor.finish());
    0
}

#[cfg(not(target_os = "none"))]
fn read_stdin() -> std::io::Result<String> {
    let mut text = String::new();
    std::io::stdin().read_to_string(&mut text)?;
    Ok(text)
}

#[cfg(not(target_os = "none"))]
fn host_fail(error: raven_m4::M4Error) -> i32 {
    use raven_m4::M4Error;

    match error {
        M4Error::Usage => {
            eprintln!("usage: m4 [-D name[=value]] [-U name] [-I dir] [file ...]");
            2
        }
        M4Error::Unsupported(name) => {
            eprintln!("m4: {name}: unsupported macro");
            2
        }
        M4Error::Include(name) => {
            eprintln!("m4: {name}: cannot open include file");
            1
        }
        M4Error::Recursion => {
            eprintln!("m4: macro recursion limit exceeded");
            1
        }
    }
}
