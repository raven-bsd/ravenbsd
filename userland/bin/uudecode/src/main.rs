// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `uudecode` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;
    use uudecode::{decode, parse, safe_output_name};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("uudecode", "[-o output] [input]"));
                return EXIT_USAGE;
            }
        };
        let Some(input) = options.input else {
            raven_rt::eprintln!("uudecode: standard input is not available in this image");
            return EXIT_USAGE;
        };
        if input == "-" {
            raven_rt::eprintln!("uudecode: standard input is not available in this image");
            return EXIT_USAGE;
        }
        let encoded = match fs::read_to_vec(&with_file_scheme(&fs::resolve(input))) {
            Ok(data) => data,
            Err(error) => {
                raven_rt::eprintln!("{}", line("uudecode", input, error));
                return EXIT_ERROR;
            }
        };
        let decoded = match decode(&encoded) {
            Ok(decoded) => decoded,
            Err(_) => {
                raven_rt::eprintln!("uudecode: invalid encoded input");
                return EXIT_ERROR;
            }
        };
        let output = options.output.unwrap_or(&decoded.name);
        if options.output.is_none() && !safe_output_name(output) {
            raven_rt::eprintln!("uudecode: unsafe encoded file name");
            return EXIT_ERROR;
        }
        if let Err(error) = write_file(output, &decoded.data, decoded.mode) {
            raven_rt::eprintln!("{}", line("uudecode", output, error));
            return EXIT_ERROR;
        }
        EXIT_OK
    }

    fn write_file(path: &str, data: &[u8], mode: u32) -> Result<(), Errno> {
        let target = with_file_scheme(&fs::resolve(path));
        let mut file = fs::open(&target, O_WRONLY | O_CREAT | O_TRUNC)?;
        write_all(&mut file, data)?;
        file.close()?;
        fs::chmod(&target, mode & 0o7777)
    }

    fn write_all(file: &mut fs::File, data: &[u8]) -> Result<(), Errno> {
        let mut offset = 0usize;
        while offset < data.len() {
            let written = file.write(data.get(offset..).unwrap_or(&[]))?;
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
