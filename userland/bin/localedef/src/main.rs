// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `localedef` utility.

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

    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let options = match localedef::parse_args(&arguments) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("localedef: {error}");
            eprintln!("usage: {}", localedef::USAGE);
            return 2;
        }
    };
    let source = match std::fs::read_to_string(&options.input) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("localedef: {}: {error}", options.input);
            return 1;
        }
    };
    let charmap = match std::fs::read_to_string(&options.charmap) {
        Ok(charmap) => charmap,
        Err(error) => {
            eprintln!("localedef: {}: {error}", options.charmap);
            return 1;
        }
    };
    let image =
        match localedef::compile(&source, &charmap, &options.charmap, options.name.as_deref()) {
            Ok(image) => image,
            Err(error) => {
                eprintln!("localedef: {error}");
                return 1;
            }
        };
    if let Some(output) = &options.output {
        if let Err(error) = std::fs::write(output, image) {
            eprintln!("localedef: {output}: {error}");
            return 1;
        }
    } else if let Err(error) = std::io::stdout().write_all(&image) {
        eprintln!("localedef: stdout: {error}");
        return 1;
    }
    0
}

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::eprintln!("localedef: target build is host-only");
        1
    }
}
