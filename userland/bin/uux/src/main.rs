// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `uux` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::vec::Vec;

    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use uucp::config::{CONFIG_PATH, Config};
    use uucp::runtime::{TargetSpool, default_user, read_text};
    use uucp::spool::SpoolError;
    use uux::{UuxError, parse_args, reads_stdin, run};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!(
                    "{}",
                    usage(
                        "uux",
                        "[-bcCjnz] [-a user] [-g grade] [-x debug] command-string"
                    )
                );
                return EXIT_USAGE;
            }
        };
        let config = match load_config() {
            Ok(config) => config,
            Err(()) => {
                raven_rt::eprintln!("uux: config: invalid /etc/uucp/config");
                return EXIT_ERROR;
            }
        };
        let stdin = match reads_stdin(&options.command) {
            Ok(true) => match read_console() {
                Ok(data) => data,
                Err(error) => {
                    raven_rt::eprintln!("uux: tty:console: {error:?}");
                    return EXIT_ERROR;
                }
            },
            Ok(false) => Vec::new(),
            Err(_) => {
                raven_rt::eprintln!("uux: invalid command string");
                return EXIT_USAGE;
            }
        };
        let user = default_user(args.env_get("USER"));
        let mut spool = TargetSpool;
        match run(&options, &config, &mut spool, &user, &stdin) {
            Ok(id) => {
                if options.print_id {
                    raven_rt::println!("{id}");
                }
                EXIT_OK
            }
            Err(error) => {
                report(&error);
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

    fn read_console() -> Result<Vec<u8>, SpoolError> {
        let mut file = fs::open("tty:console", 0).map_err(|_| SpoolError::Io)?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|_| SpoolError::Io)?;
            if count == 0 {
                break;
            }
            data.extend_from_slice(buffer.get(..count).unwrap_or(&[]));
        }
        Ok(data)
    }

    fn report(error: &UuxError) {
        match error {
            UuxError::Spool(SpoolError::NotFound) => raven_rt::eprintln!("uux: file not found"),
            UuxError::Queue(queue) => raven_rt::eprintln!("uux: {queue}"),
            UuxError::NotRemote => raven_rt::eprintln!("uux: command has no `system!` prefix"),
            UuxError::UnknownSystem(system) => {
                raven_rt::eprintln!("uux: unknown system `{system}`")
            }
            UuxError::MissingInput => raven_rt::eprintln!("uux: input redirection needs a file"),
            UuxError::InvalidCommand => raven_rt::eprintln!("uux: invalid command string"),
            UuxError::Spool(_) => raven_rt::eprintln!("uux: spool error"),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
