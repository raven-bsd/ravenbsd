// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `iconv` command-line wrapper.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(not(target_os = "none"))]
fn main() {
    std::process::exit(run());
}

#[cfg(not(target_os = "none"))]
fn run() -> i32 {
    use std::io::{Read, Write};

    use iconv::{convert, parse_encoding};

    fn usage() -> i32 {
        eprintln!("usage: iconv -f from -t to [-o file] [file...]");
        2
    }

    let mut from = None;
    let mut to = None;
    let mut output = None;
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-f" => match args.next() {
                Some(value) => from = Some(value),
                None => return usage(),
            },
            "-t" => match args.next() {
                Some(value) => to = Some(value),
                None => return usage(),
            },
            "-o" => match args.next() {
                Some(value) => output = Some(value),
                None => return usage(),
            },
            "--" => {
                files.extend(args);
                break;
            }
            value if value.starts_with('-') && value != "-" => {
                eprintln!("iconv: unsupported option {value}");
                return 2;
            }
            value => files.push(value.to_string()),
        }
    }
    let Some(from) = from else {
        return usage();
    };
    let Some(to) = to else {
        return usage();
    };
    let from = match parse_encoding(&from) {
        Some(encoding) => encoding,
        None => {
            eprintln!("iconv: unsupported encoding {from}");
            return 1;
        }
    };
    let to = match parse_encoding(&to) {
        Some(encoding) => encoding,
        None => {
            eprintln!("iconv: unsupported encoding {to}");
            return 1;
        }
    };

    let mut input = Vec::new();
    if files.is_empty() {
        if let Err(error) = std::io::stdin().read_to_end(&mut input) {
            eprintln!("iconv: stdin: {error}");
            return 1;
        }
    } else {
        for file in &files {
            if file == "-" {
                if let Err(error) = std::io::stdin().read_to_end(&mut input) {
                    eprintln!("iconv: stdin: {error}");
                    return 1;
                }
                continue;
            }
            match std::fs::read(file) {
                Ok(mut data) => input.append(&mut data),
                Err(error) => {
                    eprintln!("iconv: {file}: {error}");
                    return 1;
                }
            }
        }
    }

    let converted = match convert(&input, from, to) {
        Ok(converted) => converted,
        Err(error) => {
            eprintln!(
                "iconv: conversion failed at offset {}: {}",
                error.offset, error.message
            );
            return 1;
        }
    };
    match output.as_deref() {
        Some(path) if path != "-" => {
            if let Err(error) = std::fs::write(path, converted) {
                eprintln!("iconv: {path}: {error}");
                return 1;
            }
        }
        _ => {
            if let Err(error) = std::io::stdout().write_all(&converted) {
                eprintln!("iconv: stdout: {error}");
                return 1;
            }
        }
    }
    0
}

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::eprintln!("iconv: target build is host-only");
        1
    }
}
