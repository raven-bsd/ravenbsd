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
    extern crate alloc;

    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let arguments: Vec<String> = args.iter().skip(1).map(ToString::to_string).collect();
        let options = match localedef::parse_args(&arguments) {
            Ok(options) => options,
            Err(error) => {
                raven_rt::eprintln!("localedef: {error}");
                raven_rt::eprintln!("usage: {}", localedef::USAGE);
                return 2;
            }
        };
        let source = match read_text(&options.input) {
            Ok(source) => source,
            Err(message) => {
                raven_rt::eprintln!("localedef: {message}");
                return 1;
            }
        };
        let charmap = match read_text(&options.charmap) {
            Ok(charmap) => charmap,
            Err(message) => {
                raven_rt::eprintln!("localedef: {message}");
                return 1;
            }
        };
        let image = match localedef::compile(
            &source,
            &charmap,
            &options.charmap,
            options.name.as_deref(),
        ) {
            Ok(image) => image,
            Err(error) => {
                raven_rt::eprintln!("localedef: {error}");
                return 1;
            }
        };
        if let Some(output) = &options.output {
            if let Err(message) = write_file(output, &image) {
                raven_rt::eprintln!("localedef: {message}");
                return 1;
            }
        } else if let Err(error) = write_stdout(&image) {
            raven_rt::eprintln!("localedef: stdout: {error}");
            return 1;
        }
        0
    }

    fn read_text(path: &str) -> Result<String, String> {
        let scheme = scheme_path(path);
        let data =
            fs::read_to_vec(&scheme).map_err(|error| format!("{path}: {}", error.description()))?;
        core::str::from_utf8(&data)
            .map(String::from)
            .map_err(|_| format!("{path}: invalid UTF-8"))
    }

    fn write_file(path: &str, data: &[u8]) -> Result<(), String> {
        let scheme = scheme_path(path);
        let mut file = fs::open(&scheme, O_WRONLY | O_CREAT | O_TRUNC)
            .map_err(|error| format!("{path}: {}", error.description()))?;
        let mut offset = 0usize;
        while offset < data.len() {
            let written = file
                .write(data.get(offset..).unwrap_or(&[]))
                .map_err(|error| format!("{path}: {}", error.description()))?;
            if written == 0 {
                return Err(format!("{path}: short write"));
            }
            offset = offset.saturating_add(written);
        }
        file.close()
            .map_err(|error| format!("{path}: {}", error.description()))
    }

    /// Add the `file:` scheme to a host-style path; an explicit scheme is left unchanged.
    fn scheme_path(path: &str) -> String {
        match path.find(':') {
            Some(index) if !path[..index].contains('/') => path.to_string(),
            _ => format!("file:{path}"),
        }
    }

    fn write_stdout(data: &[u8]) -> Result<(), &'static str> {
        let mut offset = 0usize;
        while offset < data.len() {
            let written = raven_rt::sys::debug_write(data.get(offset..).unwrap_or(&[]))
                .map_err(|_| "write failed")?;
            if written == 0 {
                return Err("short write");
            }
            offset = offset.saturating_add(written);
        }
        Ok(())
    }
}
