// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `time` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::String;
    use alloc::vec::Vec;

    use raven_abi::types::clock;
    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_rt::spawn::{RtSys, SpawnOptions, Sys};
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use time::{format as format_times, parse_args};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("time", "[-p] utility [argument ...]"));
                return EXIT_USAGE;
            }
        };
        let image = match raven_rt::map_initfs() {
            Ok(image) => image,
            Err(_) => return EXIT_ERROR,
        };
        let initfs = match Initfs::parse(image) {
            Ok(initfs) => initfs,
            Err(_) => return EXIT_ERROR,
        };
        let program = if options.command[0].contains('/') {
            String::from(options.command[0])
        } else {
            format!("/bin/{}", options.command[0])
        };
        let argv: Vec<&str> = options.command.clone();
        let spawn_options = SpawnOptions {
            args: &argv,
            env: &[],
        };
        let start = match timestamp() {
            Ok(value) => value,
            Err(_) => return EXIT_ERROR,
        };
        let child = match raven_rt::spawn::spawn(&initfs, &program, &spawn_options) {
            Ok(child) => child,
            Err(_) => {
                raven_rt::eprintln!("time: {program}: cannot execute");
                return EXIT_ERROR;
            }
        };
        let mut code = EXIT_ERROR;
        if RtSys.proc_wait(child, &mut code).is_err() {
            return EXIT_ERROR;
        }
        let end = timestamp().unwrap_or(start);
        raven_rt::print!("{}", format_times(end - start, 0.0, 0.0, options.portable));
        if code == 0 { EXIT_OK } else { code }
    }

    fn timestamp() -> Result<f64, ()> {
        raven_rt::clock_get(clock::MONOTONIC)
            .map(|time| time.tv_sec as f64 + time.tv_nsec as f64 / 1_000_000_000.0)
            .map_err(|_| ())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
