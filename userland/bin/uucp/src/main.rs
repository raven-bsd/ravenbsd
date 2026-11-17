// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `uucp` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use uucp::config::{CONFIG_PATH, Config};
    use uucp::runtime::{TargetSpool, default_user, read_text};
    use uucp::spool::SpoolError;
    use uucp::{UucpError, parse_args, run};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!(
                    "{}",
                    usage(
                        "uucp",
                        "[-cCdfjmrR] [-g grade] [-n user] source ... destination"
                    )
                );
                return EXIT_USAGE;
            }
        };
        let config = match load_config() {
            Ok(config) => config,
            Err(error) => {
                raven_rt::eprintln!("uucp: config: {error:?}");
                return EXIT_ERROR;
            }
        };
        let user = default_user(args.env_get("USER"));
        let mut spool = TargetSpool;
        match run(&options, &config, &mut spool, &user) {
            Ok(ids) => {
                if options.print_id {
                    for id in ids {
                        raven_rt::println!("{id}");
                    }
                }
                EXIT_OK
            }
            Err(error) => {
                report(&error);
                EXIT_ERROR
            }
        }
    }

    fn load_config() -> Result<Config, SpoolError> {
        match read_text(CONFIG_PATH) {
            Ok(text) => Config::parse(&text).map_err(|_| SpoolError::InvalidPath),
            Err(SpoolError::NotFound) => Ok(Config::default()),
            Err(error) => Err(error),
        }
    }

    fn report(error: &UucpError) {
        match error {
            UucpError::Spool(SpoolError::NotFound) => raven_rt::eprintln!("uucp: file not found"),
            UucpError::Queue(queue) => raven_rt::eprintln!("uucp: {queue}"),
            UucpError::UnknownSystem(system) => {
                raven_rt::eprintln!("uucp: unknown system `{system}`")
            }
            UucpError::InvalidAddress => raven_rt::eprintln!("uucp: invalid address"),
            UucpError::NotDirectory => raven_rt::eprintln!("uucp: destination is not a directory"),
            UucpError::CrossPeer => {
                raven_rt::eprintln!("uucp: remote-to-remote copy is not supported")
            }
            UucpError::Spool(_) => raven_rt::eprintln!("uucp: spool error"),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
