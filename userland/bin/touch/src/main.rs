// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `touch` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_proto_fs::{O_CREAT, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let parsed = match raven_touch::parse(args.iter().skip(1)) {
            Ok(parsed) => parsed,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("touch", "file ..."));
                return EXIT_USAGE;
            }
        };
        if parsed.operands.is_empty() {
            raven_rt::eprintln!("{}", usage("touch", "file ..."));
            return EXIT_USAGE;
        }
        let mut status = EXIT_OK;
        for path in parsed.operands {
            if let Err(error) = touch(path) {
                raven_rt::eprintln!("{}", line("touch", path, error));
                status = EXIT_ERROR;
            }
        }
        status
    }

    fn touch(path: &str) -> Result<(), Errno> {
        let resolved = fs::resolve(path);
        let full = with_file_scheme(&resolved);
        let file = fs::open(&full, O_WRONLY | O_CREAT)?;
        file.close()
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
