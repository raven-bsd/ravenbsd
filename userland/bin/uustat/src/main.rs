// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `uustat` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use core::fmt;

    use raven_rt::entry;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use uucp::config::{CONFIG_PATH, Config};
    use uucp::runtime::{TargetSpool, default_user, read_text};
    use uucp::spool::SpoolError;
    use uustat::{UustatError, parse_args, run};

    entry!(main);

    struct Console;

    impl fmt::Write for Console {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            raven_rt::print!("{text}");
            Ok(())
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!(
                    "{}",
                    usage("uustat", "[-a] [-q] [-s system] [-u user] [-k job]")
                );
                return EXIT_USAGE;
            }
        };
        let config = match load_config() {
            Ok(config) => config,
            Err(()) => {
                raven_rt::eprintln!("uustat: config: invalid /etc/uucp/config");
                return EXIT_ERROR;
            }
        };
        let user = default_user(args.env_get("USER"));
        let mut spool = TargetSpool;
        let mut out = Console;
        match run(&options, &config, &mut spool, &user, &mut out) {
            Ok(()) => EXIT_OK,
            Err(UustatError::Queue(error)) => {
                raven_rt::eprintln!("uustat: {error}");
                EXIT_ERROR
            }
            Err(UustatError::Output) => {
                raven_rt::eprintln!("uustat: output error");
                EXIT_ERROR
            }
        }
    }

    fn load_config() -> Result<Config, ()> {
        match read_text(CONFIG_PATH) {
            Ok(text) => Config::parse(&text).map_err(|_| ()),
            Err(SpoolError::NotFound) => Ok(Config::default()),
            Err(_) => Err(()),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
