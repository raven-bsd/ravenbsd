// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `cal` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use cal::{civil_from_unix, parse_args, render};
    use raven_abi::types::clock;
    use raven_rt::entry;
    use raven_tool_core::Errno;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let today = match raven_rt::clock_get(clock::REALTIME) {
            Ok(time) => {
                let (year, month, _day) = civil_from_unix(time.tv_sec);
                (year, month)
            }
            Err(error) => {
                raven_rt::eprintln!("cal: clock: {}", error.description());
                return EXIT_ERROR;
            }
        };
        let options = match parse_args(args.iter().skip(1), today) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("cal", "[-3 | -y] [month [year]]"));
                return EXIT_USAGE;
            }
        };
        match write_all(render(&options).as_bytes()) {
            Ok(()) => EXIT_OK,
            Err(error) => {
                raven_rt::eprintln!("cal: write: {}", error.description());
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
