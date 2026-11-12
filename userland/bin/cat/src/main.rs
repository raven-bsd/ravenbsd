// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `cat` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_proto_fs::O_RDONLY;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let parsed = match raven_cat::parse(args.iter().skip(1)) {
            Ok(parsed) => parsed,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("cat", "[file ...]"));
                return EXIT_USAGE;
            }
        };
        if parsed.operands.is_empty() {
            raven_rt::eprintln!("{}", usage("cat", "[file ...]"));
            return EXIT_USAGE;
        }
        let mut status = EXIT_OK;
        for path in parsed.operands {
            if let Err(error) = copy_file(path) {
                raven_rt::eprintln!("{}", line("cat", path, error));
                status = EXIT_ERROR;
            }
        }
        status
    }

    fn copy_file(path: &str) -> Result<(), raven_tool_core::Errno> {
        let full = with_file_scheme(&fs::resolve(path));
        let mut file = fs::open(&full, O_RDONLY)?;
        let mut buffer = [0u8; 4096];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                return Ok(());
            }
            write_all(buffer.get(..read).unwrap_or(&[]))?;
        }
    }

    fn write_all(data: &[u8]) -> Result<(), raven_tool_core::Errno> {
        let mut offset = 0usize;
        while offset < data.len() {
            let written = raven_rt::debug_write(data.get(offset..).unwrap_or(&[]))?;
            if written == 0 {
                return Err(raven_tool_core::Errno::EIO);
            }
            offset = offset.saturating_add(written);
        }
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
