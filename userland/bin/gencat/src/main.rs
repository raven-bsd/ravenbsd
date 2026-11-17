// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `gencat` command-line wrapper.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(not(target_os = "none"))]
fn main() {
    std::process::exit(run());
}

#[cfg(not(target_os = "none"))]
fn run() -> i32 {
    use std::io::Write;

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 2 {
        eprintln!("usage: gencat catalog source...");
        return 2;
    }
    let catalog = &args[0];
    let mut sources = Vec::new();
    for source in &args[1..] {
        match std::fs::read_to_string(source) {
            Ok(text) => sources.push(text),
            Err(error) => {
                eprintln!("gencat: {source}: {error}");
                return 1;
            }
        }
    }
    let source_refs = sources.iter().map(String::as_str).collect::<Vec<_>>();
    let image = match gencat::compile(&source_refs) {
        Ok(image) => image,
        Err(error) => {
            eprintln!("gencat: {error}");
            return 1;
        }
    };
    if catalog == "-" {
        if let Err(error) = std::io::stdout().write_all(&image) {
            eprintln!("gencat: stdout: {error}");
            return 1;
        }
    } else if let Err(error) = std::fs::write(catalog, image) {
        eprintln!("gencat: {catalog}: {error}");
        return 1;
    }
    0
}

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::eprintln!("gencat: target build is host-only");
        1
    }
}
