// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `strip` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};

    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use raven_tool_core::path::with_file_scheme;
    use strip::{Mode, Options, StripError, parse_args, strip};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(error) => {
                usage_error(error);
                return EXIT_USAGE;
            }
        };
        run(&options)
    }

    fn run(options: &Options<'_>) -> i32 {
        let mut status = EXIT_OK;
        for path in &options.operands {
            if let Err(message) = strip_one(path, options.output, options.mode) {
                raven_rt::eprintln!("strip: {path}: {message}");
                status = EXIT_ERROR;
            }
        }
        status
    }

    fn strip_one(path: &str, output: Option<&str>, mode: Mode) -> Result<(), String> {
        let source = with_file_scheme(&fs::resolve(path));
        let mut data = fs::read_to_vec(&source).map_err(|error| error.description().to_string())?;
        strip(&mut data, mode).map_err(|error| message(error).to_string())?;
        let destination = output.unwrap_or(path);
        write_file(destination, &data).map_err(|error| error.description().to_string())
    }

    fn write_file(path: &str, data: &[u8]) -> Result<(), Errno> {
        let resolved = with_file_scheme(&fs::resolve(path));
        let mut file = fs::open(&resolved, O_WRONLY | O_CREAT | O_TRUNC)?;
        let mut offset = 0usize;
        while offset < data.len() {
            let written = file.write(data.get(offset..).unwrap_or(&[]))?;
            if written == 0 {
                return Err(Errno::EIO);
            }
            offset = offset.saturating_add(written);
        }
        file.close()
    }

    fn message(error: StripError) -> &'static str {
        match error {
            StripError::Malformed => "not a supported ELF32/ELF64 file",
            StripError::VerifyFailed => "section headers did not verify after stripping",
        }
    }

    fn usage_error(error: strip::UsageError) {
        match error {
            strip::UsageError::OutputWithMultipleFiles => {
                raven_rt::eprintln!("strip: -o requires a single input file");
            }
            _ => {
                raven_rt::eprintln!("{}", usage("strip", "[-g|-s] [-o outfile] file ..."));
            }
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
    let options = match strip::parse_args(args.iter().map(String::as_str)) {
        Ok(options) => options,
        Err(strip::UsageError::OutputWithMultipleFiles) => {
            eprintln!("strip: -o requires a single input file");
            return 2;
        }
        Err(_) => {
            eprintln!("usage: strip [-g|-s] [-o outfile] file ...");
            return 2;
        }
    };
    let mut status = 0;
    for path in &options.operands {
        let destination = options.output.unwrap_or(path);
        if options.output.is_some()
            && let Err(error) = std::fs::copy(path, destination)
        {
            eprintln!("strip: {path}: {error}");
            status = 1;
            continue;
        }
        if let Err(message) = strip_file(destination, options.mode) {
            eprintln!("strip: {path}: {message}");
            status = 1;
        }
    }
    status
}

#[cfg(not(target_os = "none"))]
fn strip_file(path: &str, mode: strip::Mode) -> Result<(), &'static str> {
    let mut data = std::fs::read(path).map_err(|_| "cannot read file")?;
    strip::strip(&mut data, mode).map_err(host_message)?;
    std::fs::write(path, data).map_err(|_| "cannot write file")
}

#[cfg(not(target_os = "none"))]
fn host_message(error: strip::StripError) -> &'static str {
    match error {
        strip::StripError::Malformed => "not a supported ELF32/ELF64 file",
        strip::StripError::VerifyFailed => "section headers did not verify after stripping",
    }
}
