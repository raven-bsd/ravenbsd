// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `asa` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    extern crate alloc;

    use alloc::vec::Vec;

    use asa::{interpret, parse};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let operands = match parse(args.iter().skip(1)) {
            Ok(operands) => operands,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("asa", "[file ...]"));
                return EXIT_USAGE;
            }
        };
        if operands.is_empty() {
            return match read_stdin() {
                Ok(data) => emit(&data),
                Err(error) => {
                    raven_rt::eprintln!("asa: standard input: {}", error.description());
                    EXIT_ERROR
                }
            };
        }
        let mut status = EXIT_OK;
        for path in operands {
            let data = if path == "-" {
                read_stdin()
            } else {
                fs::read_to_vec(&with_file_scheme(&fs::resolve(path)))
            };
            match data {
                Ok(data) => {
                    if emit(&data) != EXIT_OK {
                        status = EXIT_ERROR;
                    }
                }
                Err(error) => {
                    raven_rt::eprintln!("{}", line("asa", path, error));
                    status = EXIT_ERROR;
                }
            }
        }
        status
    }

    fn read_stdin() -> Result<Vec<u8>, Errno> {
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = raven_rt::debug_read(&mut buffer, 1_000_000_000)?;
            if read == 0 {
                return Ok(data);
            }
            data.extend_from_slice(buffer.get(..read).unwrap_or(&[]));
        }
    }

    fn emit(data: &[u8]) -> i32 {
        let output = interpret(data);
        match write_all(&output) {
            Ok(()) => EXIT_OK,
            Err(error) => {
                raven_rt::eprintln!("asa: write: {}", error.description());
                EXIT_ERROR
            }
        }
    }

    fn write_all(data: &[u8]) -> Result<(), Errno> {
        let mut offset = 0usize;
        while offset < data.len() {
            let written = raven_rt::debug_write(data.get(offset..).unwrap_or(&[]))?;
            if written == 0 {
                return Err(Errno::EIO);
            }
            offset = offset.saturating_add(written);
        }
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
