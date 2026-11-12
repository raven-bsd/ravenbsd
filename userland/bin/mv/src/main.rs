// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `mv` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_proto_fs::{S_IFDIR, S_IFMT};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let parsed = match raven_mv::parse(args.iter().skip(1)) {
            Ok(parsed) => parsed,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("mv", "source target"));
                return EXIT_USAGE;
            }
        };
        let Some((source, target)) = raven_mv::pair(&parsed.operands) else {
            raven_rt::eprintln!("{}", usage("mv", "source target"));
            return EXIT_USAGE;
        };
        let target_is_dir = fs::stat(&with_file_scheme(&fs::resolve(target)))
            .map(|stat| stat.mode & S_IFMT == S_IFDIR)
            .unwrap_or(false);
        let destination = raven_mv::destination(source, target, target_is_dir);
        match rename(source, &destination) {
            Ok(()) => EXIT_OK,
            Err(error) => {
                raven_rt::eprintln!("{}", line("mv", source, error));
                EXIT_ERROR
            }
        }
    }

    fn rename(source: &str, target: &str) -> Result<(), Errno> {
        fs::rename(&fs::resolve(source), &fs::resolve(target))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
