// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `zcat` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::ToString;

    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;
    use zcat::{decode, parse_args, source_name};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1).map(ToString::to_string)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("zcat", "file ..."));
                return EXIT_USAGE;
            }
        };
        let mut status = EXIT_OK;
        for path in &options.files {
            if let Err(error) = process(path) {
                raven_rt::eprintln!("{}", line("zcat", path, error));
                status = EXIT_ERROR;
            }
        }
        status
    }

    fn process(path: &str) -> Result<(), Errno> {
        let source = source_name(path);
        let data = fs::read_to_vec(&with_file_scheme(&fs::resolve(&source)))?;
        let decoded = decode(&data).map_err(|_| Errno::EINVAL)?;
        let mut offset = 0usize;
        while offset < decoded.len() {
            let written = raven_rt::debug_write(decoded.get(offset..).unwrap_or(&[]))?;
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
