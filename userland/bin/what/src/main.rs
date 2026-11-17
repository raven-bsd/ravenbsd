// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `what` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use what::{extract, parse_args, render};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("what", "[-s] file ..."));
                return EXIT_USAGE;
            }
        };
        let mut status = EXIT_OK;
        for file in &options.files {
            match raven_rt::fs::read_to_vec(file) {
                Ok(data) => {
                    if let Err(error) =
                        write_all(render(&extract(&data, options.first_only)).as_bytes())
                    {
                        raven_rt::eprintln!("what: write: {}", error.description());
                        return EXIT_ERROR;
                    }
                }
                Err(error) => {
                    raven_rt::eprintln!("what: {file}: {}", error.description());
                    status = EXIT_ERROR;
                }
            }
        }
        status
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
