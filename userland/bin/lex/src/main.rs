// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `lex` command.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(not(target_os = "none"))]
use std::io::Read;

#[cfg(not(target_os = "none"))]
fn main() {
    let mut arguments = std::env::args().skip(1);
    let mut stdout = false;
    let mut output: Option<String> = None;
    let mut input: Option<String> = None;
    let mut verbose = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-t" => stdout = true,
            "-v" => verbose = true,
            "-o" => match arguments.next() {
                Some(path) => output = Some(path),
                None => usage(),
            },
            "-h" | "--help" => usage(),
            other if other.starts_with('-') && other != "-" => {
                eprintln!("lex: unknown option `{other}`");
                std::process::exit(2);
            }
            other => {
                if input.is_some() {
                    eprintln!("lex: more than one input file");
                    std::process::exit(2);
                }
                input = Some(other.to_string());
            }
        }
    }
    let source = match input.as_deref() {
        Some(path) if path != "-" => match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("lex: {path}: {error}");
                std::process::exit(1);
            }
        },
        _ => {
            let mut text = String::new();
            if let Err(error) = std::io::stdin().read_to_string(&mut text) {
                eprintln!("lex: stdin: {error}");
                std::process::exit(1);
            }
            text
        }
    };
    let generated = match lex::compile(&source) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("lex: {error}");
            std::process::exit(1);
        }
    };
    let path = output.unwrap_or_else(|| {
        if stdout {
            "-".to_string()
        } else {
            "lex.yy.c".to_string()
        }
    });
    if path == "-" {
        print!("{generated}");
    } else if let Err(error) = std::fs::write(&path, generated) {
        eprintln!("lex: {path}: {error}");
        std::process::exit(1);
    }
    if verbose {
        eprintln!("lex: wrote {path}");
    }
}

#[cfg(not(target_os = "none"))]
fn usage() -> ! {
    eprintln!("usage: lex [-t] [-v] [-o output] [file]");
    std::process::exit(2);
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[cfg(target_os = "none")]
#[allow(dead_code)]
fn main() {}
