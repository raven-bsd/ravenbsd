// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `file` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_file::{Kind, describe, line, parse};
    use raven_proto_fs::O_RDONLY;
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("file", "[-b] [-i] path ..."));
                return EXIT_USAGE;
            }
        };
        if options.operands.is_empty() {
            raven_rt::eprintln!("{}", usage("file", "[-b] [-i] path ..."));
            return EXIT_USAGE;
        }
        let mut status = EXIT_OK;
        for path in options.operands {
            match describe_path(path, options.mime) {
                Ok(description) => {
                    raven_rt::println!("{}", line(path, &description, options.brief))
                }
                Err(error) => {
                    raven_rt::eprintln!("file: {path}: {}", error.description());
                    status = EXIT_ERROR;
                }
            }
        }
        status
    }

    fn describe_path(path: &str, mime: bool) -> Result<alloc::string::String, Errno> {
        let resolved = fs::resolve(path);
        if let Ok(target) = fs::readlink(&resolved) {
            return Ok(describe(Kind::Symlink, Some(&target), &[], mime));
        }
        let full = with_file_scheme(&resolved);
        let stat = fs::stat(&full)?;
        let kind = Kind::from_mode(stat.mode);
        if kind != Kind::Regular {
            return Ok(describe(kind, None, &[], mime));
        }
        let mut file = fs::open(&full, O_RDONLY)?;
        let mut sample = [0u8; 4096];
        let read = file.read(&mut sample)?;
        Ok(describe(
            kind,
            None,
            sample.get(..read).unwrap_or(&[]),
            mime,
        ))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
