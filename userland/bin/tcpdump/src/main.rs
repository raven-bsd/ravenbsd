// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `tcpdump` command.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(not(target_os = "none"))]
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let options = match raven_tcpdump::cli::parse(arguments) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("tcpdump: {error}");
            std::process::exit(1);
        }
    };
    let input = match &options.read {
        Some(path) => match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("tcpdump: {path}: {error}");
                std::process::exit(1);
            }
        },
        None => Vec::new(),
    };
    match raven_tcpdump::execute(&input, &options) {
        Ok(output) => {
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            if let Err(error) = stdout.write_all(output.text.as_bytes()) {
                eprintln!("tcpdump: {error}");
                std::process::exit(1);
            }
            if let (Some(path), Some(dump)) = (&options.write, output.dump)
                && let Err(error) = std::fs::write(path, dump)
            {
                eprintln!("tcpdump: {path}: {error}");
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("tcpdump: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(target_os = "none")]
mod program {
    use alloc::string::String;
    use alloc::vec::Vec;

    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let values: Vec<String> = args.iter().skip(1).map(String::from).collect();
        let options = match raven_tcpdump::cli::parse(values) {
            Ok(options) => options,
            Err(error) => {
                raven_rt::eprintln!("tcpdump: {error}");
                return 1;
            }
        };
        let input = match &options.read {
            Some(path) => match read_file(path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    raven_rt::eprintln!("tcpdump: {path}: {}", error.description());
                    return 1;
                }
            },
            None => Vec::new(),
        };
        match raven_tcpdump::execute(&input, &options) {
            Ok(output) => {
                for line in output.text.lines() {
                    raven_rt::println!("{line}");
                }
                if let (Some(path), Some(dump)) = (&options.write, output.dump)
                    && let Err(error) = write_file(path, &dump)
                {
                    raven_rt::eprintln!("tcpdump: {path}: {}", error.description());
                    return 1;
                }
                0
            }
            Err(error) => {
                raven_rt::eprintln!("tcpdump: {error}");
                1
            }
        }
    }

    fn read_file(path: &str) -> Result<Vec<u8>, Errno> {
        let full = with_file_scheme(&fs::resolve(path));
        fs::read_to_vec(&full)
    }

    fn write_file(path: &str, data: &[u8]) -> Result<(), Errno> {
        let full = with_file_scheme(&fs::resolve(path));
        let mut file = fs::open(&full, O_WRONLY | O_CREAT | O_TRUNC)?;
        let mut offset = 0usize;
        while offset < data.len() {
            let written = file.write(&data[offset..])?;
            if written == 0 {
                return Err(Errno::EIO);
            }
            offset = offset.saturating_add(written);
        }
        file.close()
    }
}
