// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `uncompress` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::ToString;

    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;
    use uncompress::{decode, output_name, parse_args};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1).map(ToString::to_string)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("uncompress", "[-c] file ..."));
                return EXIT_USAGE;
            }
        };
        let mut status = EXIT_OK;
        for path in &options.files {
            if let Err(error) = process(path, options.stdout) {
                raven_rt::eprintln!("{}", line("uncompress", path, error));
                status = EXIT_ERROR;
            }
        }
        status
    }

    fn process(path: &str, stdout: bool) -> Result<(), Errno> {
        let source = if path.ends_with(".Z") {
            path.to_string()
        } else {
            alloc::format!("{path}.Z")
        };
        let resolved = with_file_scheme(&fs::resolve(&source));
        let stat = fs::stat(&resolved)?;
        let data = fs::read_to_vec(&resolved)?;
        let decoded = decode(&data).map_err(|_| Errno::EINVAL)?;
        if stdout {
            return write_stdout(&decoded);
        }
        let target = output_name(&source);
        write_file(&target, &decoded)?;
        let _ = fs::chmod(&with_file_scheme(&fs::resolve(&target)), stat.mode & 0o7777);
        let _ = fs::unlink(&with_file_scheme(&fs::resolve(&source)));
        Ok(())
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

    fn write_stdout(data: &[u8]) -> Result<(), Errno> {
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
