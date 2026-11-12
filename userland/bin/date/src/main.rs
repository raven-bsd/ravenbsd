// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `date` utility. Protocol version 1 has no wall clock; print monotonic uptime as
//! `up <seconds>s`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use core::fmt;

    use raven_abi::types::clock;
    use raven_date::{Action, format_uptime, parse_args};
    use raven_rt::entry;

    entry!(main);

    const TOOL: &str = "date";

    struct Console;

    impl fmt::Write for Console {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            raven_rt::print!("{text}");
            Ok(())
        }
    }

    fn seconds() -> Result<u64, raven_abi::errno::Errno> {
        let time = raven_rt::clock_get(clock::MONOTONIC)?;
        u64::try_from(time.tv_sec).map_err(|_| raven_abi::errno::Errno::EOVERFLOW)
    }

    fn main(args: raven_rt::Args) -> i32 {
        let action = match parse_args(args.iter().skip(1)) {
            Ok(action) => action,
            Err(error) => {
                raven_rt::eprintln!("{error}");
                return 2;
            }
        };
        let seconds = match seconds() {
            Ok(seconds) => seconds,
            Err(error) => {
                raven_rt::eprintln!("{TOOL}: {}", error.description());
                return 1;
            }
        };
        match action {
            Action::Print => {
                let mut console = Console;
                if format_uptime(seconds, &mut console).is_err() {
                    return 1;
                }
                raven_rt::println!();
                0
            }
            Action::Selftest => {
                raven_rt::println!("{TOOL}: selftest ok");
                0
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
