// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `yacc` command.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(not(target_os = "none"))]
use std::io::Read;

#[cfg(not(target_os = "none"))]
fn main() {
    let mut arguments = std::env::args().skip(1);
    let mut definitions = false;
    let mut verbose = false;
    let mut output: Option<String> = None;
    let mut input: Option<String> = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-d" => definitions = true,
            "-v" => verbose = true,
            "-o" => match arguments.next() {
                Some(path) => output = Some(path),
                None => usage(),
            },
            "-h" | "--help" => usage(),
            other if other.starts_with('-') && other != "-" => {
                eprintln!("yacc: unknown option `{other}`");
                std::process::exit(2);
            }
            other => {
                if input.is_some() {
                    eprintln!("yacc: more than one input file");
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
                eprintln!("yacc: {path}: {error}");
                std::process::exit(1);
            }
        },
        _ => {
            let mut text = String::new();
            if let Err(error) = std::io::stdin().read_to_string(&mut text) {
                eprintln!("yacc: stdin: {error}");
                std::process::exit(1);
            }
            text
        }
    };
    let grammar = match yacc::Grammar::parse(&source) {
        Ok(grammar) => grammar,
        Err(error) => {
            eprintln!("yacc: {error}");
            std::process::exit(1);
        }
    };
    let generated = match yacc::compile_grammar(&grammar) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("yacc: {error}");
            std::process::exit(1);
        }
    };
    let output_path = output.unwrap_or_else(|| "y.tab.c".to_string());
    if let Err(error) = std::fs::write(&output_path, generated) {
        eprintln!("yacc: {output_path}: {error}");
        std::process::exit(1);
    }
    if definitions {
        let header_path = header_path(&output_path);
        let header = match yacc::header(&grammar) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("yacc: {error}");
                std::process::exit(1);
            }
        };
        if let Err(error) = std::fs::write(&header_path, header) {
            eprintln!("yacc: {header_path}: {error}");
            std::process::exit(1);
        }
    }
    if verbose {
        match yacc::statistics(&grammar) {
            Ok((rules, shift_reduce, reduce_reduce)) => {
                eprintln!(
                    "yacc: {rules} rules, {shift_reduce} shift/reduce, {reduce_reduce} reduce/reduce conflicts"
                );
            }
            Err(error) => eprintln!("yacc: {error}"),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn header_path(output: &str) -> String {
    if let Some(stem) = output.strip_suffix(".c") {
        format!("{stem}.h")
    } else if output == "y.tab.c" {
        "y.tab.h".to_string()
    } else {
        format!("{output}.h")
    }
}

#[cfg(not(target_os = "none"))]
fn usage() -> ! {
    eprintln!("usage: yacc [-d] [-v] [-o output] grammar");
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
