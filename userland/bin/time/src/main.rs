// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `time` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
// The target reads the reaped child's CPU counters through the raw PROC_TIMES syscall; the only
// unsafe block is the scalar wrapper, with a SAFETY comment.
#![cfg_attr(not(target_os = "none"), forbid(unsafe_code))]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::String;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::clock;
    use raven_initfs::Initfs;
    use raven_rt::entry;
    use raven_rt::spawn::{RtSys, SpawnOptions};
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use time::{format as format_times, parse_args};

    /// `proc_times(out[4], count)`; fields 2 and 3 are the reaped-child user/system nanoseconds.
    const PROC_TIMES: usize = 0x4C;

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
        let (user, sys) = child_times();
        raven_rt::eprint!("{}", format_times(end - start, user, sys, options.portable));
        if code == 0 { EXIT_OK } else { code }
    }

    fn timestamp() -> Result<f64, ()> {
        raven_rt::clock_get(clock::MONOTONIC)
            .map(|time| time.tv_sec as f64 + time.tv_nsec as f64 / 1_000_000_000.0)
            .map_err(|_| ())
    }

    fn child_times() -> (f64, f64) {
        let mut values = [0u64; 4];
        // SAFETY: PROC_TIMES writes four u64 values to the pointer; the array is valid for
        // exactly four elements.
        let result = unsafe {
            raven_rt::sys::syscall6(
                PROC_TIMES,
                values.as_mut_ptr() as usize,
                values.len(),
                0,
                0,
                0,
                0,
            )
        };
        if Errno::from_ret(result).is_err() {
            return (0.0, 0.0);
        }
        (
            values[2] as f64 / 1_000_000_000.0,
            values[3] as f64 / 1_000_000_000.0,
        )
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
