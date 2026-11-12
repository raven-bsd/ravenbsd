// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `mkdir` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let parsed = match raven_mkdir::parse(args.iter().skip(1)) {
            Ok(parsed) => parsed,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("mkdir", "[-p] directory ..."));
                return EXIT_USAGE;
            }
        };
        if parsed.operands.is_empty() {
            raven_rt::eprintln!("{}", usage("mkdir", "[-p] directory ..."));
            return EXIT_USAGE;
        }
        let mut status = EXIT_OK;
        for path in parsed.operands {
            let result = if parsed.flags.p {
                create_parents(path)
            } else {
                fs::mkdir(&fs::resolve(path), 0o755)
            };
            if let Err(error) = result {
                raven_rt::eprintln!("{}", line("mkdir", path, error));
                status = EXIT_ERROR;
            }
        }
        status
    }

    fn create_parents(path: &str) -> Result<(), Errno> {
        let resolved = fs::resolve(path);
        for prefix in raven_mkdir::parents(&resolved) {
            match fs::mkdir(&prefix, 0o755) {
                // An existing directory is fine. A mount root answers EINVAL because the
                // forwarded path is exactly "/" on the mounted server; it is already present.
                Ok(()) | Err(Errno::EEXIST) | Err(Errno::EINVAL) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
